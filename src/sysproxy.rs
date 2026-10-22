//! Windows system proxy configuration and restoration.
//!
//! Sets the current-user WinINet proxy (HKCU\...\Internet Settings) so that
//! proxy-aware applications use our local listeners, and restores the exact
//! previous state afterwards. A snapshot is also persisted to disk so a crash
//! cannot leave the machine permanently proxied.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
const INTERNET_OPTION_REFRESH: u32 = 37;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub proxy_enable: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_override: Option<String>,
    pub auto_config_url: Option<String>,
    pub auto_detect: Option<u32>,
}

fn key() -> Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(INTERNET_SETTINGS, KEY_READ | KEY_WRITE)
        .context("open Internet Settings registry key")
}

/// Read the current proxy configuration.
pub fn capture() -> Result<Snapshot> {
    let k = key()?;
    Ok(Snapshot {
        proxy_enable: k.get_value("ProxyEnable").ok(),
        proxy_server: k.get_value("ProxyServer").ok(),
        proxy_override: k.get_value("ProxyOverride").ok(),
        auto_config_url: k.get_value("AutoConfigURL").ok(),
        auto_detect: k.get_value("AutoDetect").ok(),
    })
}

/// The `ProxyServer` value that routes proxy-aware apps through our listeners.
pub fn proxy_server_value(http_port: u16, socks_port: u16) -> String {
    format!("http=127.0.0.1:{http_port};https=127.0.0.1:{http_port};socks=127.0.0.1:{socks_port}")
}

/// Capture the current state, then point the system proxy at our listeners.
/// Returns the snapshot needed to restore.
pub fn enable(http_port: u16, socks_port: u16) -> Result<Snapshot> {
    let snap = capture()?;
    let k = key()?;
    k.set_value("ProxyEnable", &1u32)?;
    k.set_value("ProxyServer", &proxy_server_value(http_port, socks_port))?;
    k.set_value("ProxyOverride", &"<local>".to_string())?;
    // A PAC script would take precedence over the manual proxy, so clear it
    // while we are connected (restored on disconnect).
    let _ = k.delete_value("AutoConfigURL");
    k.set_value("AutoDetect", &0u32)?;
    notify();
    let _ = persist(&snap);
    Ok(snap)
}

/// Restore a previously captured snapshot.
pub fn restore(snap: &Snapshot) -> Result<()> {
    let k = key()?;
    set_or_delete_u32(&k, "ProxyEnable", snap.proxy_enable)?;
    set_or_delete_str(&k, "ProxyServer", snap.proxy_server.as_deref())?;
    set_or_delete_str(&k, "ProxyOverride", snap.proxy_override.as_deref())?;
    set_or_delete_str(&k, "AutoConfigURL", snap.auto_config_url.as_deref())?;
    set_or_delete_u32(&k, "AutoDetect", snap.auto_detect)?;
    notify();
    let _ = clear_persisted();
    Ok(())
}

/// Convenience for an app that starts up after a crash: if a snapshot was
/// persisted but not restored, restore it now.
pub fn recover_if_needed() -> Result<bool> {
    match load_persisted() {
        Some(snap) => {
            restore(&snap)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn set_or_delete_u32(k: &RegKey, name: &str, v: Option<u32>) -> Result<()> {
    match v {
        Some(v) => k.set_value(name, &v)?,
        None => {
            let _ = k.delete_value(name);
        }
    }
    Ok(())
}

fn set_or_delete_str(k: &RegKey, name: &str, v: Option<&str>) -> Result<()> {
    match v {
        Some(v) => k.set_value(name, &v.to_string())?,
        None => {
            let _ = k.delete_value(name);
        }
    }
    Ok(())
}

/// Tell running WinINet apps that settings changed.
fn notify() {
    unsafe {
        windows_sys::Win32::Networking::WinInet::InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null(),
            0,
        );
        windows_sys::Win32::Networking::WinInet::InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null(),
            0,
        );
    }
}

fn backup_path() -> PathBuf {
    crate::util::app_dir().join("proxy-backup.json")
}

fn persist(snap: &Snapshot) -> Result<()> {
    std::fs::write(backup_path(), serde_json::to_string_pretty(snap)?)?;
    Ok(())
}

fn load_persisted() -> Option<Snapshot> {
    let text = std::fs::read_to_string(backup_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn clear_persisted() -> Result<()> {
    let _ = std::fs::remove_file(backup_path());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_server_string_has_all_schemes() {
        let s = proxy_server_value(2080, 1080);
        assert_eq!(
            s,
            "http=127.0.0.1:2080;https=127.0.0.1:2080;socks=127.0.0.1:1080"
        );
    }

    #[test]
    fn snapshot_is_serializable() {
        let s = Snapshot {
            proxy_enable: Some(1),
            proxy_server: Some("x".into()),
            ..Default::default()
        };
        let t = serde_json::to_string(&s).unwrap();
        let back: Snapshot = serde_json::from_str(&t).unwrap();
        assert_eq!(back.proxy_enable, Some(1));
        assert_eq!(back.proxy_server.as_deref(), Some("x"));
    }
}
