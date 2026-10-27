//! Platforms without a tray adapter retain the ordinary application window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Connect,
    Disconnect,
    Test,
    Quit,
}
pub fn is_available() -> bool {
    false
}
pub fn spawn() -> anyhow::Result<()> {
    Ok(())
}
pub fn poll() -> Vec<TrayCommand> {
    Vec::new()
}
pub fn shutdown() {}
