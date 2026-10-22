//! Copy-first, non-destructive migration. V2 never writes into a previous store.
use crate::{
    model::{AppConfig, SniProfile},
    store, util, vmess,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Report {
    pub profiles: usize,
    pub backup: PathBuf,
}

pub fn migrate(source: &Path) -> Result<Report> {
    let target = util::app_dir();
    anyhow::ensure!(
        source.canonicalize()? != target.canonicalize()?,
        "Source and V2 store must differ"
    );
    anyhow::ensure!(
        !target.join("config.json").exists() && !target.join("secret.key").exists(),
        "V2 already has data; migration will not overwrite it"
    );
    let native = source.join("config.json");
    let legacy = source.join("profile.json");
    anyhow::ensure!(
        native.is_file() || legacy.is_file(),
        "No supported configuration found"
    );
    let backup = target
        .join("backups")
        .join(format!("previous-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&backup)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o700))?;
    }
    // Back up original bytes, including the key, before parsing or writing new profiles.
    for name in ["config.json", "secret.key", "profile.json"] {
        if source.join(name).is_file() {
            let bytes = std::fs::read(source.join(name))?;
            private_write(&backup.join(name), &bytes)?;
            anyhow::ensure!(
                std::fs::read(backup.join(name))? == bytes,
                "Backup verification failed"
            );
        }
    }
    let before;
    if native.is_file() {
        if backup.join("secret.key").exists() {
            private_write(
                &target.join("secret.key"),
                &std::fs::read(backup.join("secret.key"))?,
            )?;
        }
        let bytes = std::fs::read(backup.join("config.json"))?;
        let doc: Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            doc["version"].as_u64().unwrap_or(0) <= 1,
            "Unsupported store version; backup retained"
        );
        private_write(&target.join("config.json"), &bytes)?;
        before = store::load_checked().context("Read copied native store; backup retained")?;
    } else {
        let document: Value = serde_json::from_slice(&std::fs::read(backup.join("profile.json"))?)?;
        before = convert_legacy(&document)?;
        store::save(&before)?;
    }
    let after = store::load_checked()?;
    anyhow::ensure!(
        serde_json::to_value(&before)? == serde_json::to_value(&after)?,
        "Migration verification failed; original and backup retained"
    );
    let receipt = serde_json::json!({"migration_version":1,"from":source.to_string_lossy(),
        "profiles":after.servers.len(),"verified":true,"backup":backup.to_string_lossy()});
    private_write(
        &target.join("migration.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(Report {
        profiles: after.servers.len(),
        backup,
    })
}

pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut stream = options
        .open(path)
        .with_context(|| format!("Create {} without replacement", path.display()))?;
    stream.write_all(bytes)?;
    stream.sync_all()?;
    Ok(())
}

pub fn convert_legacy(doc: &Value) -> Result<AppConfig> {
    let mut cfg = AppConfig::default();
    cfg.settings.mode = "tun".into();
    cfg.settings.auto_reconnect = doc["auto_reconnect"].as_bool().unwrap_or(true);
    let records = doc
        .get("servers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let fallback = doc
        .get("vmess")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let records = if records.is_empty() {
        fallback
            .map(|link| vec![serde_json::json!({"id":"migrated","vmess":link})])
            .unwrap_or_default()
    } else {
        records
    };
    for record in records {
        let raw = record["vmess"]
            .as_str()
            .context("Saved profile has no VMess data")?;
        let mut parsed =
            vmess::parse_any(raw).context("Parse previous profile; source and backup preserved")?;
        anyhow::ensure!(
            parsed.len() == 1,
            "A saved server must contain exactly one profile"
        );
        let mut profile = parsed.remove(0);
        profile.id = record["id"].as_str().unwrap_or("migrated").to_string();
        if let Some(name) = record["name"].as_str() {
            profile.name = name.into();
        }
        // Preserve the effective legacy global override, including an existing enabled MUX.
        use base64::Engine as _;
        let raw_doc = if let Some(encoded) = raw.strip_prefix("vmess://") {
            let padded = format!("{}{}", encoded, "=".repeat((4 - encoded.len() % 4) % 4));
            base64::engine::general_purpose::STANDARD
                .decode(padded)
                .ok()
                .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
        } else {
            serde_json::from_str::<Value>(raw).ok()
        };
        let explicit_mux = raw_doc
            .as_ref()
            .is_some_and(|value| value.get("mux_enabled").is_some());
        if doc["override_mux"]
            .as_bool()
            .unwrap_or(doc.get("mux").is_some())
            || !explicit_mux
        {
            profile.mux_enabled = doc["mux"].as_bool().unwrap_or(true);
            profile.mux_concurrency = 8;
        }
        cfg.servers.push(profile);
    }
    cfg.selected_server = doc["selected"]
        .as_str()
        .map(str::to_string)
        .or_else(|| cfg.servers.first().map(|p| p.id.clone()));
    let snis = doc["snis"].as_array().cloned().unwrap_or_default();
    for sni in snis {
        cfg.sni_profiles.push(SniProfile {
            inherit_tls: true,
            tls: cfg
                .selected_server_profile()
                .is_some_and(|profile| profile.tls)
                || sni["sni"].as_str().is_some_and(|value| !value.is_empty()),
            id: sni["id"].as_str().unwrap_or_default().into(),
            name: sni["name"].as_str().unwrap_or_default().into(),
            sni: sni["sni"].as_str().unwrap_or_default().into(),
            ws_host: sni["ws-host"].as_str().unwrap_or_default().into(),
            ws_path: sni["ws-path"].as_str().unwrap_or_default().into(),
            allow_insecure: sni["insecure"].as_bool().unwrap_or(false),
            ..Default::default()
        });
    }
    cfg.selected_sni_profile = doc["selected_sni"].as_str().map(str::to_string);
    // The legacy launcher uses the currently edited root overrides at connect
    // time, even when they differ from a named SNI profile. Preserve both.
    let active = SniProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Previous active overrides".into(),
        inherit_tls: true,
        tls: cfg
            .selected_server_profile()
            .is_some_and(|profile| profile.tls)
            || doc["sni"].as_str().is_some_and(|value| !value.is_empty()),
        sni: doc["sni"].as_str().unwrap_or_default().into(),
        ws_host: doc["ws-host"].as_str().unwrap_or_default().into(),
        ws_path: doc["ws-path"].as_str().unwrap_or_default().into(),
        allow_insecure: doc["insecure"].as_bool().unwrap_or(false),
        ..Default::default()
    };
    let has_root = doc["insecure"].as_bool().unwrap_or(false)
        || ["sni", "ws-host", "ws-path"].iter().any(|key| {
            doc[key]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty())
        });
    if !has_root {
        cfg.selected_sni_profile = None;
    }
    let matches = cfg.selected_sni().is_some_and(|saved| {
        saved.sni == active.sni
            && saved.ws_host == active.ws_host
            && saved.ws_path == active.ws_path
            && saved.allow_insecure == active.allow_insecure
    });
    if has_root && !matches {
        cfg.selected_sni_profile = Some(active.id.clone());
        cfg.sni_profiles.push(active);
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blank_legacy_overrides_keep_imported_tls_and_security() {
        let raw=serde_json::json!({"add":"server.example","port":443,"id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"ws","tls":"tls","sni":"ea.com","allowInsecure":true}).to_string();
        let cfg=convert_legacy(&serde_json::json!({"servers":[{"id":"one","vmess":raw}],"sni":"","ws-host":"","ws-path":"","insecure":false})).unwrap();
        assert!(cfg.selected_sni().is_none());
        let endpoint = crate::model::resolve_endpoint(&cfg.servers[0], cfg.selected_sni());
        assert!(endpoint.tls && endpoint.allow_insecure);
    }
    #[test]
    fn legacy_credentials_overrides_and_mux_survive() {
        let raw = serde_json::json!({"add":"server.example","port":443,"id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"ws","tls":"tls","sni":"ea.com","host":"front","path":"/ws"}).to_string();
        let cfg = convert_legacy(&serde_json::json!({"servers":[{"id":"original-id","name":"Original","vmess":raw}],"selected":"original-id","mux":true,"sni":"ea.com"})).unwrap();
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].sni, "ea.com");
        assert!(cfg.servers[0].mux_enabled);
        assert_eq!(cfg.selected_server.as_deref(), Some("original-id"));
        assert_eq!(cfg.servers[0].ws_path, "/ws");
        assert!(cfg.selected_sni_profile.is_some());
        let endpoint = crate::model::resolve_endpoint(&cfg.servers[0], cfg.selected_sni());
        assert!(endpoint.tls);
        assert_eq!(endpoint.sni, "ea.com");
    }
}
