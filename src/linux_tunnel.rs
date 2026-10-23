//! Native Slint front end over the existing, separately elevated Linux helper.
use crate::{
    compat,
    engine::{Phase, StatsEvent},
    logging::Logger,
    migration,
    model::Endpoint,
    util,
};
use anyhow::{Context, Result};
use std::{path::PathBuf, process::Stdio, time::Instant};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    sync::mpsc,
};

pub struct LinuxTunnel {
    child: Child,
    stdin: Option<ChildStdin>,
    config: PathBuf,
    pub lines: mpsc::Receiver<String>,
    reader: tokio::task::JoinHandle<()>,
    started: Option<Instant>,
    previous: Option<(u64, u64, Instant)>,
    up: u64,
    down: u64,
}
impl LinuxTunnel {
    pub async fn start(
        ep: &Endpoint,
        settings: &crate::model::Settings,
        logger: &Logger,
    ) -> Result<Self> {
        let helper = std::env::current_exe()?
            .parent()
            .unwrap()
            .join("network.py");
        anyhow::ensure!(
            helper.is_file(),
            "Linux TUN helper is missing; install the complete V2 package"
        );
        let config = util::app_dir().join(format!("linux-tun-{}.json", uuid::Uuid::new_v4()));
        let value = compat::config(ep, util::free_port(), "", "warning")?;
        migration::private_write(&config, &serde_json::to_vec(&value)?)?;
        let mut command = tokio::process::Command::new("pkexec");
        command
            .arg("/usr/bin/python3")
            .arg(&helper)
            .arg("--config")
            .arg(&config);
        command
            .arg("--health-settings")
            .arg(serde_json::to_string(&settings.health)?);
        if settings.kill_switch {
            command.arg("--kill-switch");
        }
        if !settings.auto_reconnect {
            command.arg("--no-recovery");
        }
        let spawned = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&config);
                return Err(e).context("Start Linux permission helper");
            }
        };
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, lines) = mpsc::channel(256);
        let tx_error = tx.clone();
        let error_logger = logger.clone();
        tokio::spawn(async move {
            let mut input = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = input.next_line().await {
                error_logger.warn(line.clone());
                if tx_error.send(format!("HELPER: {line}")).await.is_err() {
                    break;
                }
            }
        });
        let reader = tokio::spawn(async move {
            let mut input = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = input.next_line().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            config,
            lines,
            reader,
            started: None,
            previous: None,
            up: 0,
            down: 0,
        })
    }
    pub async fn verify(&mut self) -> Result<()> {
        let input = self.stdin.as_mut().context("Helper input is closed")?;
        input.write_all(b"verify\n").await?;
        input.flush().await?;
        Ok(())
    }
    pub fn phase(line: &str) -> Option<Phase> {
        if line.starts_with("CONNECTED:") || line.starts_with("VERIFIED:") {
            Some(Phase::FullForwarding)
        } else if line.starts_with("RECONNECTING:") {
            Some(Phase::Reconnecting)
        } else if line.starts_with("DEGRADED:") {
            Some(Phase::Degraded)
        } else if line.starts_with("CONFIGURING:") {
            Some(Phase::Configuring)
        } else if line.starts_with("VERIFYING:") {
            Some(Phase::Verifying)
        } else if line.starts_with("ERROR:") {
            Some(Phase::Error)
        } else {
            None
        }
    }
    pub fn stats(&mut self) -> StatsEvent {
        let read = |name| {
            std::fs::read_to_string(format!("/sys/class/net/sni0/statistics/{name}_bytes"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        };
        let mut up_speed = 0;
        let mut down_speed = 0;
        if let (Some(tx), Some(rx)) = (read("tx"), read("rx")) {
            let now = Instant::now();
            if let Some((old_tx, old_rx, then)) = self.previous {
                let elapsed = now.duration_since(then).as_secs_f64().max(0.001);
                let up = tx.saturating_sub(old_tx);
                let down = rx.saturating_sub(old_rx);
                self.up += up;
                self.down += down;
                up_speed = (up as f64 / elapsed) as u64;
                down_speed = (down as f64 / elapsed) as u64;
            }
            self.previous = Some((tx, rx, now));
        } else {
            self.previous = None;
        }
        StatsEvent {
            up: self.up,
            down: self.down,
            up_speed,
            down_speed,
            uptime_secs: self.started.map(|s| s.elapsed().as_secs()).unwrap_or(0),
            ..Default::default()
        }
    }
    pub fn mark_connected(&mut self) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }
    pub fn exit_code(&mut self) -> Option<i32> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|s| s.code().unwrap_or(-1))
    }
    pub async fn stop(mut self) {
        if let Some(mut input) = self.stdin.take() {
            let _ = input.write_all(b"stop\n").await;
            let _ = input.flush().await;
        }
        if tokio::time::timeout(std::time::Duration::from_secs(35), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
        self.reader.abort();
        let _ = std::fs::remove_file(&self.config);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_verified_lines_report_success() {
        assert_eq!(
            LinuxTunnel::phase("DEGRADED: probe failed"),
            Some(Phase::Degraded)
        );
        assert_eq!(
            LinuxTunnel::phase("VERIFYING: starting"),
            Some(Phase::Verifying)
        );
        assert_eq!(
            LinuxTunnel::phase("CONNECTED: HTTPS and UDP verified"),
            Some(Phase::FullForwarding)
        );
        assert_eq!(LinuxTunnel::phase("Xray process started"), None);
    }
}
