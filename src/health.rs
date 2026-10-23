//! Health notification policy. Only verified forwarding generates success.
use crate::engine::Phase;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Notifications {
    verified: bool,
    recovering: bool,
    last: HashMap<&'static str, Instant>,
    pub history: VecDeque<String>,
}
impl Notifications {
    pub fn transition(
        &mut self,
        phase: Phase,
        now: Instant,
    ) -> Option<(&'static str, &'static str)> {
        let event = match phase {
            Phase::FullForwarding => {
                let previous = self.verified;
                self.verified = true;
                if self.recovering {
                    self.recovering = false;
                    Some(("Tunnel Restored", "IPv4 system and proxy traffic verified; review Connection Health for protection status."))
                } else if !previous {
                    Some((
                        "Full Tunnel Active",
                        "System and proxy IPv4 traffic verified. IPv6 protection checked.",
                    ))
                } else {
                    None
                }
            }
            Phase::Reconnecting if !self.verified => {
                self.recovering = true;
                None
            }
            Phase::Degraded | Phase::Reconnecting if self.verified && !self.recovering => {
                self.recovering = true;
                Some(("Tunnel Connection Lost", "Verification failed. Check SNI Tunnel for recovery and traffic protection status."))
            }
            Phase::Error if self.recovering => Some((
                "Tunnel Recovery Failed",
                "Your connection could not be restored. Check SNI Tunnel.",
            )),
            Phase::Disconnected => {
                self.verified = false;
                self.recovering = false;
                None
            }
            _ => None,
        };
        let (title, body) = event?;
        if self
            .last
            .get(title)
            .is_some_and(|at| now.duration_since(*at) < Duration::from_secs(60))
        {
            return None;
        }
        self.last.insert(title, now);
        if self.history.len() == 30 {
            self.history.pop_front();
        }
        self.history.push_back(format!("{title}: {body}"));
        Some((title, body))
    }
}

/// A bounded worker keeps desktop notification services off the GUI thread.
pub fn worker(
    logger: crate::logging::Logger,
) -> std::sync::mpsc::SyncSender<(&'static str, &'static str)> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<(&'static str, &'static str)>(8);
    std::thread::spawn(move || {
        for (title, body) in rx {
            let mut command = notification_command(title, body);
            command
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            match command.spawn() {
                Ok(mut child) => {
                    let began = Instant::now();
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                if !status.success() {
                                    logger.warn("Desktop notification service rejected the notification; see in-app history");
                                }
                                break;
                            }
                            Err(_) => break,
                            Ok(None) if began.elapsed() > Duration::from_secs(10) => {
                                let _ = child.kill();
                                let _ = child.wait();
                                logger.warn(
                                    "Desktop notification service timed out; see in-app history",
                                );
                                break;
                            }
                            _ => std::thread::sleep(Duration::from_millis(100)),
                        }
                    }
                }
                Err(e) => logger.warn(format!("Desktop notifications unavailable: {e}")),
            }
        }
    });
    tx
}
#[cfg(not(windows))]
fn notification_command(title: &str, body: &str) -> std::process::Command {
    let mut c = std::process::Command::new("notify-send");
    c.args([
        "--app-name=SNI Tunnel",
        "--expire-time=8000",
        "--",
        title,
        body,
    ]);
    c
}
#[cfg(windows)]
fn notification_command(title: &str, body: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    // Native Windows notification-area messages work for installed and portable
    // builds without a separately registered toast AppUserModelID. Fixed text
    // is passed as environment data, never PowerShell source.
    let script = r#"
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$icon = New-Object System.Windows.Forms.NotifyIcon
try {
    $icon.Icon = [System.Drawing.SystemIcons]::Information
    $icon.Text = 'SNI Tunnel'
    $icon.Visible = $true
    $icon.ShowBalloonTip(4000, $env:SNI_NOTIFICATION_TITLE, $env:SNI_NOTIFICATION_BODY, [System.Windows.Forms.ToolTipIcon]::Info)
    Start-Sleep -Seconds 4
} finally { $icon.Dispose() }
"#;
    let mut c = std::process::Command::new("powershell.exe");
    c.creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("SNI_NOTIFICATION_TITLE", title)
        .env("SNI_NOTIFICATION_BODY", body);
    c
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_verified_success_and_recovery_notify() {
        let mut n = Notifications::default();
        let now = Instant::now();
        assert!(n.transition(Phase::CoreReady, now).is_none());
        assert!(n.transition(Phase::Forwarding, now).is_none());
        assert_eq!(
            n.transition(Phase::FullForwarding, now).unwrap().0,
            "Full Tunnel Active"
        );
        assert!(n.transition(Phase::FullForwarding, now).is_none());
        assert!(n.transition(Phase::Degraded, now).is_some());
        assert!(n.transition(Phase::Reconnecting, now).is_none());
        assert_eq!(
            n.transition(Phase::FullForwarding, now).unwrap().0,
            "Tunnel Restored"
        );
        assert!(n.transition(Phase::Disconnected, now).is_none());
        assert!(n.transition(Phase::Error, now).is_none());
    }
    #[test]
    fn repeated_exhaustion_is_rate_limited() {
        let mut n = Notifications::default();
        let now = Instant::now();
        n.transition(Phase::FullForwarding, now);
        n.transition(Phase::Degraded, now);
        assert!(n.transition(Phase::Error, now).is_some());
        assert!(n
            .transition(Phase::Error, now + Duration::from_secs(10))
            .is_none());
        assert!(n
            .transition(Phase::Error, now + Duration::from_secs(61))
            .is_some());
    }
}
