//! System tray for platforms without the Windows tray implementation.
//!
//! The window itself works everywhere; on this platform the tray icon is not
//! available, so `spawn` reports that and `poll` never produces commands.

use anyhow::{bail, Result};

pub const MENU_SHOW: &str = "sni-tunnel.show";
pub const MENU_CONNECT: &str = "sni-tunnel.connect";
pub const MENU_DISCONNECT: &str = "sni-tunnel.disconnect";
pub const MENU_TEST: &str = "sni-tunnel.test";
pub const MENU_QUIT: &str = "sni-tunnel.quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Connect,
    Disconnect,
    Test,
    Quit,
}

/// Create the tray icon on a background thread and start pumping messages.
pub fn spawn() -> Result<()> {
    bail!("the system tray is not available on this platform")
}

/// Drain the commands the tray produced.
pub fn poll() -> Vec<TrayCommand> {
    Vec::new()
}
