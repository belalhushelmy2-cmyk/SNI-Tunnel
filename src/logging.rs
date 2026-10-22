//! In-app log bus: fans out to the UI and to an append-only log file, and scrubs
//! secrets (UUIDs) before they reach either sink.
//!
//! Two modes decide what the user sees:
//!
//! * **Simple** - only the connection events, the warnings and the errors.
//!   The events are the NetMod style lifecycle lines ("Xray-core started
//!   successfully", "VPN routes installed", ...).
//! * **Advanced** - everything, including the core's debug output, the
//!   routing decisions and the DNS queries.
//!
//! The log *file* always gets every line, so a problem can be analysed after
//! the fact no matter which mode was on.

use crate::util;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::broadcast;

/// What the log shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogMode {
    /// Only the connection events, the warnings and the errors.
    Simple,
    /// Everything, including the core's debug output.
    Advanced,
}

impl LogMode {
    pub fn label(self) -> &'static str {
        match self {
            LogMode::Simple => "Simple",
            LogMode::Advanced => "Advanced",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "simple" | "" => Some(LogMode::Simple),
            "advanced" | "debug" | "full" => Some(LogMode::Advanced),
            _ => None,
        }
    }

    /// The level of the core that fits the mode.
    pub fn core_log_level(self) -> &'static str {
        match self {
            LogMode::Simple => "warning",
            LogMode::Advanced => "debug",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub ts: String,
    pub level: String,
    pub text: String,
}

impl LogLine {
    pub fn render(&self) -> String {
        format!("[{}] {:<5} {}", self.ts, self.level, self.text)
    }

    /// The line as it is shown in the log panel.
    pub fn display(&self) -> String {
        format!("[{}] {}", self.ts, self.text)
    }
}

#[derive(Clone)]
pub struct Logger {
    tx: broadcast::Sender<LogLine>,
    secrets: Arc<Mutex<Vec<String>>>,
    file: Arc<Mutex<Option<std::fs::File>>>,
    mode: Arc<RwLock<LogMode>>,
    history: Arc<Mutex<(u64, VecDeque<LogLine>)>>,
}

impl Logger {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(util::app_log_path())
            .ok();
        Self {
            tx,
            secrets: Arc::new(Mutex::new(Vec::new())),
            file: Arc::new(Mutex::new(file)),
            mode: Arc::new(RwLock::new(LogMode::Simple)),
            history: Arc::new(Mutex::new((0, VecDeque::new()))),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    /// Bounded, scrubbed history survives late UI startup and mode changes.
    pub fn snapshot_since(&self, revision: u64) -> Option<(u64, Vec<LogLine>)> {
        let history = self.history.lock().unwrap();
        if history.0 == revision { return None; }
        Some((history.0, history.1.iter().filter(|l| self.visible(&l.level)).cloned().collect()))
    }

    pub fn clear_history(&self) {
        let mut history = self.history.lock().unwrap();
        history.1.clear();
        history.0 += 1;
    }

    /// Register values that must never appear in a log line.
    pub fn set_secrets(&self, secrets: Vec<String>) {
        let mut g = self.secrets.lock().unwrap();
        *g = secrets.into_iter().filter(|s| !s.is_empty()).collect();
    }

    /// The mode that decides what the log shows.
    pub fn mode(&self) -> LogMode {
        *self.mode.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Switch the mode. The file keeps every line either way.
    pub fn set_mode(&self, mode: LogMode) {
        if let Ok(mut m) = self.mode.write() {
            *m = mode;
            drop(m);
            self.history.lock().unwrap().0 += 1;
        }
    }

    /// Is a line of this level shown in the current mode?
    fn visible(&self, level: &str) -> bool {
        match self.mode() {
            LogMode::Advanced => true,
            // The simple mode shows the lifecycle events and the problems.
            LogMode::Simple => matches!(level, "EVENT" | "WARN" | "ERROR"),
        }
    }

    pub fn log(&self, level: &str, msg: impl Into<String>) {
        let raw: String = msg.into();
        let text = {
            let secrets = self.secrets.lock().unwrap();
            util::scrub_sensitive(&util::scrub_all(&raw, &secrets))
        };
        let line = LogLine {
            ts: util::now_hms(),
            level: level.to_string(),
            text,
        };
        // The file always gets everything.
        if let Ok(mut f) = self.file.lock() {
            if let Some(file) = f.as_mut() {
                let _ = writeln!(file, "{}", line.render());
            }
        }
        {
            let mut history = self.history.lock().unwrap();
            history.1.push_back(line.clone());
            if history.1.len() > 2000 { history.1.pop_front(); }
            history.0 += 1;
        }
        if !self.visible(level) {
            return;
        }
        let _ = self.tx.send(line);
    }

    /// A connection lifecycle event (the NetMod style lines).
    pub fn event(&self, m: impl Into<String>) {
        self.log("EVENT", m);
    }

    pub fn info(&self, m: impl Into<String>) {
        self.log("INFO", m);
    }
    pub fn warn(&self, m: impl Into<String>) {
        self.log("WARN", m);
    }
    pub fn error(&self, m: impl Into<String>) {
        self.log("ERROR", m);
    }
    pub fn debug(&self, m: impl Into<String>) {
        self.log("DEBUG", m);
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

/// Bridges the `log` facade used by third-party crates into our log bus.
struct Bridge {
    inner: Logger,
}

impl log::Log for Bridge {
    fn enabled(&self, _m: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        self.inner
            .log(record.level().as_str(), format!("{}", record.args()));
    }
    fn flush(&self) {}
}

static BRIDGE: std::sync::OnceLock<Logger> = std::sync::OnceLock::new();

pub fn install_global(logger: &Logger) {
    let l = logger.clone();
    let _ = BRIDGE.set(l);
    let bridge = Bridge {
        inner: logger.clone(),
    };
    let _ = log::set_boxed_logger(Box::new(bridge));
    log::set_max_level(log::LevelFilter::Info);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_logger() -> Logger {
        let mut logger = Logger::new();
        logger.file = Arc::new(Mutex::new(None));
        logger
    }
    #[test]
    fn startup_history_mode_switch_and_clear() {
        let logger = test_logger();
        logger.set_secrets(vec!["private-value".into()]);
        logger.info("early private-value");
        logger.warn("visible warning");
        let (rev, simple) = logger.snapshot_since(u64::MAX).unwrap();
        assert_eq!(simple.len(), 1);
        assert!(logger.snapshot_since(rev).is_none());
        logger.set_mode(LogMode::Advanced);
        let (rev, advanced) = logger.snapshot_since(rev).unwrap();
        assert_eq!(advanced.len(), 2);
        assert!(!advanced[0].text.contains("private-value"));
        logger.clear_history();
        assert!(logger.snapshot_since(rev).unwrap().1.is_empty());
    }
    #[test]
    fn history_retains_latest_two_thousand_lines() {
        let logger = test_logger();
        for i in 0..2005 { logger.event(i.to_string()); }
        let (_, lines) = logger.snapshot_since(u64::MAX).unwrap();
        assert_eq!(lines.len(), 2000);
        assert_eq!(lines[0].text, "5");
        assert_eq!(lines.last().unwrap().text, "2004");
    }
}
