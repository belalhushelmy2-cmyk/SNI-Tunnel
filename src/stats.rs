//! Real traffic accounting. Counters are incremented by the local proxy as it
//! relays bytes, so the numbers reflect actual user traffic through the tunnel.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct Traffic {
    pub up: AtomicU64,
    pub down: AtomicU64,
    pub active_conns: AtomicI64,
    pub total_conns: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub up: u64,
    pub down: u64,
    pub active_conns: i64,
    pub total_conns: u64,
}

impl Traffic {
    #[allow(dead_code)]
    pub fn reset(&self) {
        self.up.store(0, Ordering::Relaxed);
        self.down.store(0, Ordering::Relaxed);
        self.active_conns.store(0, Ordering::Relaxed);
        self.total_conns.store(0, Ordering::Relaxed);
    }

    pub fn add_up(&self, n: u64) {
        self.up.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_down(&self, n: u64) {
        self.down.fetch_add(n, Ordering::Relaxed);
    }

    pub fn conn_opened(&self) {
        self.active_conns.fetch_add(1, Ordering::Relaxed);
        self.total_conns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn conn_closed(&self) {
        self.active_conns.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            up: self.up.load(Ordering::Relaxed),
            down: self.down.load(Ordering::Relaxed),
            active_conns: self.active_conns.load(Ordering::Relaxed),
            total_conns: self.total_conns.load(Ordering::Relaxed),
        }
    }

    /// Build a snapshot from absolute counters (e.g. the TUN adapter's byte
    /// counters), where the first value is the *received* and the second the
    /// *sent* traffic.
    pub fn from_counters(received: u64, sent: u64) -> Snapshot {
        Snapshot {
            up: sent,
            down: received,
            active_conns: 0,
            total_conns: 0,
        }
    }
}

/// Counts the connections that the tunnel currently carries.
///
/// In TUN mode the operating system does not know about them: the core takes
/// the packets off the adapter itself, so there is no socket in the system's
/// connection table. The core does log every connection it takes over
/// ("... accepted tcp:<destination> [tun-in >> proxy]") and every connection it
/// closes ("connection ends > writeto tcp:<socket>->..."), so the open
/// connections are tracked from that log.
#[derive(Debug, Default)]
pub struct ConnCounter {
    open: Mutex<HashSet<String>>,
}

impl ConnCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one line of the core's log into the counter.
    pub fn observe(&self, line: &str) {
        // "from tcp:198.18.0.1:52818 accepted tcp:104.26.12.205:443 ..."
        if let Some(sock) = socket_after(line, "from tcp:") {
            self.open.lock().unwrap().insert(format!("tcp:{sock}"));
        }
        // "app/proxyman/inbound: connection ends > writeto tcp:198.18.0.1:52818->..."
        if line.contains("connection ends") {
            if let Some(sock) = socket_after(line, "writeto tcp:") {
                self.open.lock().unwrap().remove(&format!("tcp:{sock}"));
            }
        }
    }

    /// The number of connections that are open right now.
    pub fn active(&self) -> u64 {
        self.open.lock().unwrap().len() as u64
    }
}

/// The socket in a log line, e.g. `198.18.0.1:52818` after `from tcp:`.
fn socket_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let sock = rest.split_whitespace().next()?;
    let sock = sock.split("->").next()?;
    if sock.is_empty() {
        None
    } else {
        Some(sock.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_connections_are_counted() {
        let c = ConnCounter::new();
        assert_eq!(c.active(), 0);
        c.observe("from tcp:198.18.0.1:52818 accepted tcp:104.26.12.205:443 [tun-in >> proxy]");
        c.observe("from tcp:198.18.0.1:52819 accepted tcp:104.26.12.205:443 [tun-in >> proxy]");
        assert_eq!(c.active(), 2);
    }

    #[test]
    fn closed_connections_are_removed() {
        let c = ConnCounter::new();
        c.observe("from tcp:198.18.0.1:52818 accepted tcp:104.26.12.205:443 [tun-in >> proxy]");
        c.observe(
            "app/proxyman/inbound: connection ends > writeto tcp:198.18.0.1:52818->104.26.12.205:443: EOF",
        );
        assert_eq!(c.active(), 0);
    }

    #[test]
    fn udp_flows_are_not_counted() {
        let c = ConnCounter::new();
        c.observe("from udp:198.18.0.1:62972 accepted udp:1.1.1.1:53 [tun-in >> proxy]");
        assert_eq!(c.active(), 0);
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let c = ConnCounter::new();
        c.observe("2026/10/21 13:42:56 [Warning] something else");
        assert_eq!(c.active(), 0);
    }
}
