//! "Start with Windows" support via the per-user Run registry key
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`). No admin required.

use anyhow::{Context, Result};
use std::path::Path;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "SniTunnelV2";

/// The command line written to the Run key. Quoted so paths with spaces work.
pub fn run_value(exe: &Path) -> String {
    format!("\"{}\" gui", exe.display())
}

/// Is the autostart entry currently present?
pub fn is_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ)
    else {
        return false;
    };
    key.get_value::<String, _>(VALUE_NAME)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Add or remove the autostart entry for the currently running executable.
pub fn set_enabled(enabled: bool) -> Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
        .context("open the Run registry key")?;
    if enabled {
        let exe = std::env::current_exe().context("locate the current executable")?;
        key.set_value(VALUE_NAME, &run_value(&exe))
            .context("write the Run registry value")?;
    } else {
        let _ = key.delete_value(VALUE_NAME);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_quotes_path_and_uses_gui() {
        let v = run_value(Path::new(r"C:\Program Files\Sni Tunnel\sni-tunnel.exe"));
        assert_eq!(v, "\"C:\\Program Files\\Sni Tunnel\\sni-tunnel.exe\" gui");
    }
}
