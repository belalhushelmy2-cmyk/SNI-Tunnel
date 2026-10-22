//! "Start with Windows" equivalent for Linux: an autostart `.desktop` entry
//! in `~/.config/autostart`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "sni-tunnel-v2.desktop";

fn autostart_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
            PathBuf::from(home).join(".config")
        });
    base.join("autostart")
}

fn entry_path() -> PathBuf {
    autostart_dir().join(FILE_NAME)
}

/// The command line used for the autostart entry.
pub fn run_value(exe: &Path) -> String {
    format!("\"{}\" gui", exe.display())
}

/// Is the autostart entry present?
pub fn is_enabled() -> bool {
    entry_path().is_file()
}

/// Add or remove the autostart entry.
pub fn set_enabled(enabled: bool) -> Result<()> {
    let path = entry_path();
    if enabled {
        // The AppImage payload can live in a cache or temporary FUSE mount.
        // Register the user-facing image so it remains launchable after logout.
        let exe = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_file())
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .context("locate the current executable")?;
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=SNI Tunnel V2\nComment=VMess tunnel with a custom TLS SNI\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            run_value(&exe)
        );
        let dir = autostart_dir();
        std::fs::create_dir_all(&dir).context("create the autostart directory")?;
        std::fs::write(&path, content).context("write the autostart entry")?;
    } else {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}
