//! Read-only, unprivileged Linux TCP accounting. These are device socket
//! counters, not a claim that each socket traversed the VPN. No packet capture;
//! destinations and usage history are never displayed, logged or saved.
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct AppUsage {
    pub name: String,
    pub upload: u64,
    pub download: u64,
    pub upload_rate: u64,
    pub download_rate: u64,
    pub connections: usize,
}

#[derive(Clone, Debug)]
struct Socket {
    id: String,
    app: String,
    upload: u64,
    download: u64,
}

#[derive(Default)]
pub struct Monitor {
    previous: HashMap<String, Socket>,
    totals: HashMap<String, AppUsage>,
    baseline: bool,
}

impl Monitor {
    /// Resume without attributing traffic transferred while monitoring was off.
    pub fn rebaseline(&mut self) {
        self.previous.clear();
        self.baseline = false;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, sockets: Vec<Socket>, elapsed: Duration) -> Vec<AppUsage> {
        for row in self.totals.values_mut() {
            row.upload_rate = 0;
            row.download_rate = 0;
            row.connections = 0;
        }
        let mut next = HashMap::new();
        for socket in sockets.into_iter().take(20_000) {
            // A socket shared by multiple processes must only be counted once.
            if next.contains_key(&socket.id) {
                continue;
            }
            let old = self
                .previous
                .get(&socket.id)
                .filter(|old| old.app == socket.app);
            let (up, down) = if !self.baseline {
                (0, 0)
            } else if let Some(old) = old {
                (
                    socket.upload.saturating_sub(old.upload),
                    socket.download.saturating_sub(old.download),
                )
            } else {
                (socket.upload, socket.download)
            };
            // Bounded session history; never evict an old app then count its
            // lifetime counters again. Unknown additional apps are skipped.
            if self.totals.contains_key(&socket.app) || self.totals.len() < 512 {
                let row = self
                    .totals
                    .entry(socket.app.clone())
                    .or_insert_with(|| AppUsage {
                        name: socket.app.clone(),
                        ..AppUsage::default()
                    });
                row.upload = row.upload.saturating_add(up);
                row.download = row.download.saturating_add(down);
                row.upload_rate = row.upload_rate.saturating_add(up);
                row.download_rate = row.download_rate.saturating_add(down);
                row.connections += 1;
            }
            next.insert(socket.id.clone(), socket);
        }
        self.previous = next;
        self.baseline = true;
        let seconds = elapsed.as_secs_f64().max(0.001);
        let mut rows: Vec<_> = self.totals.values().cloned().collect();
        for row in &mut rows {
            row.upload_rate = (row.upload_rate as f64 / seconds) as u64;
            row.download_rate = (row.download_rate as f64 / seconds) as u64;
        }
        rows.sort_by(|a, b| {
            b.upload
                .saturating_add(b.download)
                .cmp(&a.upload.saturating_add(a.download))
                .then_with(|| a.name.cmp(&b.name))
        });
        rows
    }

    #[cfg(target_os = "linux")]
    pub async fn sample(
        &mut self,
        proxy_ports: [u16; 2],
        elapsed: Duration,
    ) -> Result<Vec<AppUsage>, String> {
        let output = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::process::Command::new("ss")
                .args(["-tinepHO"])
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| "TCP reader timed out; monitoring paused.".to_string())?
        .map_err(|_| "TCP reader unavailable. Install iproute2 (iproute on Fedora).".to_string())?;
        if !output.status.success() {
            return Err("Linux denied access to TCP counters; monitoring paused.".into());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let sockets = parse_ss(&text, proxy_ports, |pid| {
            // exe basename groups browser worker processes without exposing
            // executable paths or command-line arguments in the UI.
            let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
            Some(
                path.file_name()?
                    .to_string_lossy()
                    .trim_end_matches(" (deleted)")
                    .to_string(),
            )
        });
        Ok(self.update(sockets, elapsed))
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn sample(&mut self, _: [u16; 2], _: Duration) -> Result<Vec<AppUsage>, String> {
        Err("Per-app byte accounting is currently available on Linux only.".into())
    }
}

fn field<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(prefix))
}

fn parse_ss(
    text: &str,
    proxy_ports: [u16; 2],
    mut name: impl FnMut(u32) -> Option<String>,
) -> Vec<Socket> {
    let mut rows = Vec::new();
    let mut names = HashMap::new();
    let mut seen = HashSet::new();
    for line in text.lines().take(20_000) {
        let columns: Vec<_> = line.split_whitespace().take(5).collect();
        if columns.len() < 5 {
            continue;
        }
        let remote = columns[4];
        // Ignore local IPC and the relay side of loopback proxy connections.
        // Include only the original client's connection TO our HTTP/SOCKS port.
        let Some((address, port)) = remote.rsplit_once(':') else {
            continue;
        };
        let address = address.trim_matches(['[', ']']);
        let local = address
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|ip| ip.is_loopback())
            || address.starts_with("::ffff:127.");
        if local
            && !port
                .parse::<u16>()
                .ok()
                .is_some_and(|port| proxy_ports.contains(&port))
        {
            continue;
        }
        let Some(cookie) = field(line, "sk:").filter(|id| *id != "0") else {
            continue;
        };
        let Some(inode) = field(line, "ino:") else {
            continue;
        };
        let id = format!("{cookie}:{inode}");
        if seen.contains(&id) {
            continue;
        }
        let Some(pid) = line
            .split("pid=")
            .nth(1)
            .and_then(|part| part.split(',').next())
            .and_then(|part| part.parse::<u32>().ok())
        else {
            continue;
        };
        let app = names.entry(pid).or_insert_with(|| name(pid));
        let Some(app) = app else {
            continue;
        };
        if [
            "xray",
            "xray.exe",
            "tun2socks",
            "badvpn-tun2socks",
            "sni-tunnel",
        ]
        .contains(&app.as_str())
        {
            continue;
        }
        // ACKed payload avoids counting TCP retransmissions twice; SYN consumes
        // one sequence number. RX payload is supplied directly by TCP_INFO.
        let upload = field(line, "bytes_acked:")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            .saturating_sub(1);
        let download = field(line, "bytes_received:")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        seen.insert(id.clone());
        rows.push(Socket {
            id,
            app: app.clone(),
            upload,
            download,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn socket(id: &str, app: &str, upload: u64, download: u64) -> Socket {
        Socket {
            id: id.into(),
            app: app.into(),
            upload,
            download,
        }
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn live_socket_transfer_is_attributed_to_its_app_without_counting_relay_side() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        let mut monitor = Monitor::default();
        let step = Duration::from_secs(1);
        monitor.sample([port, 0], step).await.unwrap();
        let up = vec![7u8; 32768];
        let down = vec![8u8; 16384];
        client.write_all(&up).await.unwrap();
        server.read_exact(&mut vec![0; up.len()]).await.unwrap();
        server.write_all(&down).await.unwrap();
        client.read_exact(&mut vec![0; down.len()]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let executable = std::env::current_exe().unwrap();
        let name = executable.file_name().unwrap().to_string_lossy();
        let rows = monitor.sample([port, 0], step).await.unwrap();
        let row = rows
            .iter()
            .find(|r| r.name == name)
            .expect("test app attributed by its real PID");
        assert_eq!(row.upload, up.len() as u64);
        assert_eq!(row.download, down.len() as u64);
        assert_eq!(row.connections, 1, "server side must not be double counted");
    }
    #[test]
    fn baseline_deltas_closed_sockets_and_resume() {
        let mut monitor = Monitor::default();
        let step = Duration::from_secs(2);
        assert_eq!(
            monitor.update(vec![socket("a", "browser", 900, 1000)], step)[0].upload,
            0
        );
        let row = &monitor.update(
            vec![
                socket("a", "browser", 1100, 1600),
                socket("b", "browser", 100, 200),
            ],
            step,
        )[0];
        assert_eq!(
            (
                row.upload,
                row.download,
                row.upload_rate,
                row.download_rate,
                row.connections
            ),
            (300, 800, 150, 400, 2)
        );
        let row = &monitor.update(vec![], step)[0];
        assert_eq!(
            (row.upload, row.download, row.upload_rate, row.connections),
            (300, 800, 0, 0)
        );
        monitor.rebaseline();
        let row = &monitor.update(vec![socket("c", "browser", 50_000, 80_000)], step)[0];
        assert_eq!((row.upload, row.download), (300, 800));
        monitor.reset();
        assert_eq!(
            monitor.update(vec![socket("c", "browser", 50_000, 80_000)], step)[0].upload,
            0
        );
    }
    #[test]
    fn duplicate_sockets_counter_resets_and_application_changes() {
        let mut monitor = Monitor::default();
        let step = Duration::from_secs(1);
        monitor.update(vec![], step);
        let rows = monitor.update(
            vec![socket("a", "one", 100, 200), socket("a", "one", 100, 200)],
            step,
        );
        assert_eq!(rows[0].upload, 100);
        assert_eq!(rows[0].connections, 1);
        assert_eq!(
            monitor.update(vec![socket("a", "one", 5, 10)], step)[0].upload,
            100
        );
        let rows = monitor.update(vec![socket("a", "two", 70, 80)], step);
        assert_eq!(rows.iter().find(|r| r.name == "two").unwrap().download, 80);
    }
    #[test]
    fn parses_real_ss_format_and_excludes_relays_ipc_and_unknown_owners() {
        let fixture = "ESTAB 0 0 10.4.2.1:123 1.1.1.1:443 users:((\"browser\",pid=20,fd=5)) ino:42 sk:abc bytes_acked:201 bytes_received:500\n\
ESTAB 0 0 [::1]:888 [::1]:2080 users:((\"browser\",pid=20,fd=6)) ino:43 sk:def bytes_acked:101 bytes_received:100\n\
ESTAB 0 0 127.0.0.1:2080 127.0.0.1:888 users:((\"relay\",pid=21,fd=7)) ino:44 sk:123 bytes_acked:100 bytes_received:100\n\
ESTAB 0 0 10.4.2.1:222 1.1.1.1:443 users:((\"xray\",pid=21,fd=8)) ino:45 sk:456 bytes_acked:100 bytes_received:100\n\
ESTAB 0 0 10.4.2.1:333 1.1.1.1:443 ino:46 sk:789 bytes_acked:100 bytes_received:100";
        let rows = parse_ss(fixture, [2080, 1080], |pid| {
            Some(if pid == 20 { "browser" } else { "xray" }.into())
        });
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].upload, rows[0].download), (200, 500));
        assert_eq!(rows[1].app, "browser");
    }
}
