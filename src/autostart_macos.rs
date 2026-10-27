//! Per-user LaunchAgent, activated at the next login.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
fn entry_path() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/LaunchAgents/com.snitunnel.v2.plist")
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn run_value(exe: &Path) -> String {
    format!("{} gui", exe.display())
}
pub fn is_enabled() -> bool {
    entry_path().is_file()
}
pub fn set_enabled(enabled: bool) -> Result<()> {
    let path = entry_path();
    if enabled {
        let exe = std::env::current_exe().context("Locate installed application")?;
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.snitunnel.v2</string>
<key>ProgramArguments</key><array><string>{}</string><string>gui</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#,
            xml(&exe.to_string_lossy())
        );
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, content)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
