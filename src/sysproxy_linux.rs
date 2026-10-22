//! GNOME/Cinnamon system proxy with ownership-aware, crash-safe restoration.
//! Other desktops retain the explicit local-proxy workflow.
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    process::Stdio,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub proxy_enable: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_override: Option<String>,
    pub auto_config_url: Option<String>,
    pub auto_detect: Option<u32>,
    #[serde(default)]
    pub desktop: BTreeMap<String, String>,
    #[serde(default)]
    pub applied: BTreeMap<String, String>,
}
fn run(args: &[&str]) -> Result<String> {
    let mut child = std::process::Command::new("gsettings")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Run desktop proxy settings tool")?;
    let began = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if began.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Desktop settings service timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output()?;
    anyhow::ensure!(
        out.status.success(),
        "Desktop proxy setting failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
fn get(key: &str) -> Result<String> {
    let (schema, key) = key.split_once('|').context("Invalid proxy key")?;
    run(&["get", schema, key])
}
fn set(key: &str, value: &str) -> Result<()> {
    let (schema, key) = key.split_once('|').context("Invalid proxy key")?;
    run(&["set", schema, key, value])?;
    Ok(())
}
fn journal() -> std::path::PathBuf {
    crate::util::app_dir().join("desktop-proxy-backup.json")
}
fn desired(http: u16, socks: u16) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for (schema, port) in [("http", http), ("https", http), ("socks", socks)] {
        values.insert(
            format!("org.gnome.system.proxy.{schema}|host"),
            "'127.0.0.1'".into(),
        );
        values.insert(
            format!("org.gnome.system.proxy.{schema}|port"),
            port.to_string(),
        );
    }
    values.insert("org.gnome.system.proxy|mode".into(), "'manual'".into());
    values
}
pub fn capture() -> Result<Snapshot> {
    let mut snap = Snapshot::default();
    for key in desired(2080, 1080).keys() {
        snap.desktop.insert(key.clone(), get(key)?);
    }
    Ok(snap)
}
pub fn proxy_server_value(http: u16, socks: u16) -> String {
    format!("http=127.0.0.1:{http};socks=127.0.0.1:{socks}")
}
pub fn enable(http: u16, socks: u16) -> Result<Snapshot> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    anyhow::ensure!(["gnome","cinnamon","unity","budgie"].iter().any(|d|desktop.contains(d)),
        "This desktop has no supported system proxy adapter. Configure HTTP/SOCKS manually, or use TUN mode");
    let mut snap = capture()?;
    snap.applied = desired(http, socks);
    crate::migration::private_write(&journal(), &serde_json::to_vec_pretty(&snap)?)?;
    for (key, value) in &snap.applied {
        if let Err(e) = set(key, value) {
            let _ = restore(&snap);
            return Err(e);
        }
    }
    Ok(snap)
}
/// Restore only values still owned by this session, preserving external edits.
pub fn restore(snap: &Snapshot) -> Result<()> {
    for (key, prior) in snap.desktop.iter().rev() {
        if get(key)? == *snap.applied.get(key).unwrap_or(&String::new()) {
            set(key, prior)?;
        }
    }
    if journal().exists() {
        std::fs::remove_file(journal())?;
    }
    Ok(())
}
pub fn recover_if_needed() -> Result<bool> {
    if !journal().exists() {
        return Ok(false);
    }
    let snap: Snapshot = serde_json::from_slice(&std::fs::read(journal())?)?;
    restore(&snap)?;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_plan_does_not_touch_pac_dns_or_ignore_hosts() {
        let plan = desired(2080, 1080);
        assert_eq!(plan.len(), 7);
        assert_eq!(plan["org.gnome.system.proxy|mode"], "'manual'");
        assert!(plan
            .keys()
            .all(|k| !k.contains("ignore") && !k.contains("autoconfig")));
    }
}
