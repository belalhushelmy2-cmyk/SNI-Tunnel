//! Configuration persistence. Credentials (VMess UUIDs) are sealed with DPAPI
//! before they ever touch disk; every other field is stored as plain JSON so
//! the file stays inspectable and editable.

use crate::dpapi;
use crate::model::{AppConfig, ServerProfile, Settings, SniProfile};
use crate::util;
use anyhow::{Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const FORMAT_VERSION: u32 = 1;

/// The protocol of a profile that does not specify one.
fn default_protocol() -> String {
    "vmess".to_string()
}

/// The encryption method of a profile that does not specify one.
fn default_security() -> String {
    "auto".to_string()
}

/// The transport of a profile that does not specify one.
fn default_network() -> String {
    "tcp".to_string()
}

/// The number of UDP streams of a profile that does not specify one.
fn default_mux_xudp_concurrency() -> i32 {
    16
}

/// The UDP/443 policy of a profile that does not specify one.
fn default_mux_xudp_udp443() -> String {
    "reject".to_string()
}

/// The on-disk representation of a server profile.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerFile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    port: u16,
    /// DPAPI-sealed UUID, base64 encoded. Never stores the raw UUID.
    #[serde(default)]
    uuid_sealed: String,
    #[serde(default = "default_security")]
    security: String,
    /// The outbound protocol. Configuration files written by older versions do
    /// not have this field, so it defaults to VMess.
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default = "default_network")]
    network: String,
    #[serde(default)]
    tls: bool,
    #[serde(default)]
    sni: String,
    #[serde(default)]
    alpn: Vec<String>,
    #[serde(default)]
    allow_insecure: bool,
    #[serde(default)]
    pinned_cert_sha256: String,
    #[serde(default)]
    ws_host: String,
    #[serde(default)]
    ws_path: String,
    #[serde(default)]
    grpc_service: String,
    #[serde(default)]
    h2_host: String,
    #[serde(default)]
    h2_path: String,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    pub flow: String,
    #[serde(default)]
    pub alter_id: u32,
    #[serde(default)]
    pub remark: String,
    #[serde(default)]
    pub ws_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub dns: String,
    #[serde(default)]
    pub mux_enabled: bool,
    #[serde(default)]
    pub mux_concurrency: i32,
    #[serde(default)]
    pub mux_xudp_enabled: bool,
    #[serde(default = "default_mux_xudp_concurrency")]
    pub mux_xudp_concurrency: i32,
    #[serde(default = "default_mux_xudp_udp443")]
    pub mux_xudp_udp443: String,
    #[serde(default)]
    pub timeout_secs: u32,
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub socks_port: u16,
    #[serde(default)]
    pub tcp_header: String,
    #[serde(default)]
    pub grpc_multi: bool,
    #[serde(default)]
    pub xhttp_mode: String,
    #[serde(default)]
    pub kcp_seed: String,
    #[serde(default)]
    pub kcp_header: String,
    #[serde(default)]
    pub quic_security: String,
    #[serde(default)]
    pub quic_header: String,
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub last_test: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    servers: Vec<ServerFile>,
    #[serde(default)]
    sni_profiles: Vec<SniProfile>,
    #[serde(default)]
    selected_server: Option<String>,
    #[serde(default)]
    selected_sni_profile: Option<String>,
    #[serde(default)]
    settings: Settings,
}

fn seal(plain: &str) -> String {
    if plain.trim().is_empty() {
        return String::new();
    }
    match dpapi::protect(plain.as_bytes()) {
        Ok(c) => base64::engine::general_purpose::STANDARD.encode(c),
        Err(e) => {
            log::warn!("DPAPI sealing failed: {e}");
            String::new()
        }
    }
}

fn unseal(sealed: &str) -> String {
    if sealed.trim().is_empty() {
        return String::new();
    }
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(sealed) else {
        return String::new();
    };
    match dpapi::unprotect(&raw) {
        Ok(p) => String::from_utf8(p).unwrap_or_default(),
        Err(e) => {
            log::warn!("DPAPI unseal failed: {e}");
            String::new()
        }
    }
}

impl From<&ServerProfile> for ServerFile {
    fn from(s: &ServerProfile) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            address: s.address.clone(),
            port: s.port,
            uuid_sealed: seal(&s.uuid),
            security: s.security.clone(),
            protocol: s.protocol.clone(),
            network: s.network.clone(),
            tls: s.tls,
            sni: s.sni.clone(),
            alpn: s.alpn.clone(),
            allow_insecure: s.allow_insecure,
            pinned_cert_sha256: s.pinned_cert_sha256.clone(),
            ws_host: s.ws_host.clone(),
            ws_path: s.ws_path.clone(),
            grpc_service: s.grpc_service.clone(),
            h2_host: s.h2_host.clone(),
            h2_path: s.h2_path.clone(),
            fingerprint: s.fingerprint.clone(),
            flow: s.flow.clone(),
            alter_id: s.alter_id,
            remark: s.remark.clone(),
            ws_headers: s.ws_headers.clone(),
            dns: s.dns.clone(),
            mux_enabled: s.mux_enabled,
            mux_concurrency: s.mux_concurrency,
            mux_xudp_enabled: s.mux_xudp_enabled,
            mux_xudp_concurrency: s.mux_xudp_concurrency,
            mux_xudp_udp443: s.mux_xudp_udp443.clone(),
            timeout_secs: s.timeout_secs,
            http_port: s.http_port,
            socks_port: s.socks_port,
            tcp_header: s.tcp_header.clone(),
            grpc_multi: s.grpc_multi,
            xhttp_mode: s.xhttp_mode.clone(),
            kcp_seed: s.kcp_seed.clone(),
            kcp_header: s.kcp_header.clone(),
            quic_security: s.quic_security.clone(),
            quic_header: s.quic_header.clone(),
            latency_ms: s.latency_ms,
            last_test: s.last_test.clone(),
        }
    }
}

impl From<ServerFile> for ServerProfile {
    fn from(f: ServerFile) -> Self {
        let mut id = f.id;
        if id.is_empty() {
            id = uuid::Uuid::new_v4().to_string();
        }
        Self {
            id,
            name: f.name,
            address: f.address,
            port: f.port,
            uuid: unseal(&f.uuid_sealed),
            security: if f.security.trim().is_empty() {
                default_security()
            } else {
                f.security
            },
            protocol: if f.protocol.trim().is_empty() {
                default_protocol()
            } else {
                f.protocol
            },
            network: if f.network.trim().is_empty() {
                default_network()
            } else {
                f.network
            },
            tls: f.tls,
            sni: f.sni,
            alpn: f.alpn,
            allow_insecure: f.allow_insecure,
            pinned_cert_sha256: f.pinned_cert_sha256,
            ws_host: f.ws_host,
            ws_path: f.ws_path,
            grpc_service: f.grpc_service,
            h2_host: f.h2_host,
            h2_path: f.h2_path,
            fingerprint: f.fingerprint,
            flow: f.flow,
            alter_id: f.alter_id,
            remark: f.remark,
            ws_headers: f.ws_headers,
            dns: f.dns,
            mux_enabled: f.mux_enabled,
            mux_concurrency: f.mux_concurrency,
            mux_xudp_enabled: f.mux_xudp_enabled,
            mux_xudp_concurrency: f.mux_xudp_concurrency,
            mux_xudp_udp443: f.mux_xudp_udp443,
            timeout_secs: f.timeout_secs,
            http_port: f.http_port,
            socks_port: f.socks_port,
            tcp_header: f.tcp_header,
            grpc_multi: f.grpc_multi,
            xhttp_mode: f.xhttp_mode,
            kcp_seed: f.kcp_seed,
            kcp_header: f.kcp_header,
            quic_security: f.quic_security,
            quic_header: f.quic_header,
            latency_ms: f.latency_ms,
            last_test: f.last_test,
        }
    }
}

pub fn load_checked() -> Result<AppConfig> {
    let text = std::fs::read_to_string(util::config_path()).context("Read saved profiles")?;
    let file: ConfigFile = serde_json::from_str(&text).context("Parse saved profiles")?;
    anyhow::ensure!(
        file.version <= FORMAT_VERSION,
        "Configuration is from a newer version"
    );
    for server in &file.servers {
        anyhow::ensure!(
            server.uuid_sealed.is_empty() || !unseal(&server.uuid_sealed).is_empty(),
            "Saved credentials cannot be decrypted; original data is preserved"
        );
    }
    Ok(AppConfig {
        servers: file.servers.into_iter().map(Into::into).collect(),
        sni_profiles: file.sni_profiles,
        selected_server: file.selected_server,
        selected_sni_profile: file.selected_sni_profile,
        settings: file.settings,
    })
}

pub fn load() -> AppConfig {
    match load_checked() {
        Ok(config) => config,
        Err(error) => {
            if util::config_path().exists() {
                log::error!(
                    "Unable to load profiles: {error:#}. Existing data will not be overwritten."
                );
            }
            AppConfig::default()
        }
    }
}

pub fn save(cfg: &AppConfig) -> Result<()> {
    let file = ConfigFile {
        version: FORMAT_VERSION,
        servers: cfg.servers.iter().map(Into::into).collect(),
        sni_profiles: cfg.sni_profiles.clone(),
        selected_server: cfg.selected_server.clone(),
        selected_sni_profile: cfg.selected_sni_profile.clone(),
        settings: cfg.settings.clone(),
    };
    for (profile, stored) in cfg.servers.iter().zip(&file.servers) {
        anyhow::ensure!(
            profile.uuid.is_empty() || !stored.uuid_sealed.is_empty(),
            "Could not seal credentials; existing configuration was preserved"
        );
    }
    let text = serde_json::to_string_pretty(&file).context("serialize config")?;
    let path = util::config_path();
    save_at(&path, text.as_bytes())
}

fn save_at(path: &std::path::Path, text: &[u8]) -> Result<()> {
    use std::io::Write;
    // Loading defaults after a read/parse failure must never destroy user data.
    match std::fs::read_to_string(path) {
        Ok(existing) => {
            let file: ConfigFile = serde_json::from_str(&existing).context(
                "Existing configuration is invalid; back it up and repair it before saving",
            )?;
            anyhow::ensure!(
                file.version <= FORMAT_VERSION,
                "Configuration belongs to a newer version; refusing to overwrite it"
            );
            for server in &file.servers {
                anyhow::ensure!(server.uuid_sealed.is_empty() || !unseal(&server.uuid_sealed).is_empty(),
                    "Existing credentials could not be recovered; refusing to overwrite the configuration");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("Read existing configuration before saving"),
    }
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut stream = options
            .open(&tmp)
            .context("Create private configuration temporary file")?;
        stream.write_all(text).context("Write configuration")?;
        stream.sync_all().context("Flush configuration")?;
        drop(stream);
        std::fs::rename(&tmp, path).context("Commit configuration")?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?
                .sync_all()
                .context("Flush configuration directory")?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Import/export helper: dump a plain (unsealed) profile list as JSON that can
/// be shared, explicitly excluding the UUID for safety.
pub fn export_scrubbed(cfg: &AppConfig) -> Result<String> {
    let mut servers = cfg.servers.clone();
    for s in &mut servers {
        s.uuid.clear();
    }
    Ok(serde_json::to_string_pretty(&servers)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ServerProfile;
    use std::collections::BTreeMap;

    fn sample() -> ServerProfile {
        let mut headers = BTreeMap::new();
        headers.insert("X-Token".to_string(), "abc".to_string());
        ServerProfile {
            id: "id-1".into(),
            name: "Tokyo".into(),
            address: "1.2.3.4".into(),
            port: 8443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            sni: "ea.com".into(),
            ws_host: "front.example".into(),
            ws_path: "/ray".into(),
            ws_headers: headers,
            dns: "1.1.1.1".into(),
            mux_enabled: true,
            mux_concurrency: 4,
            mux_xudp_enabled: true,
            mux_xudp_concurrency: 12,
            mux_xudp_udp443: "allow".into(),
            timeout_secs: 20,
            http_port: 2081,
            socks_port: 1081,
            grpc_service: "GunService".into(),
            grpc_multi: true,
            xhttp_mode: "stream-one".into(),
            kcp_seed: "seed".into(),
            quic_security: "aes-128-gcm".into(),
            tcp_header: "http".into(),
            remark: "a note".into(),
            latency_ms: 42,
            last_test: "ok".into(),
            ..Default::default()
        }
    }

    #[test]
    fn server_file_round_trip_keeps_every_field() {
        let p = sample();
        let file = ServerFile::from(&p);
        let back: ServerProfile = file.into();
        assert_eq!(back.id, p.id);
        assert_eq!(back.name, p.name);
        assert_eq!(back.address, p.address);
        assert_eq!(back.port, p.port);
        assert_eq!(back.network, p.network);
        assert_eq!(back.sni, p.sni);
        assert_eq!(back.ws_host, p.ws_host);
        assert_eq!(back.ws_path, p.ws_path);
        assert_eq!(back.ws_headers, p.ws_headers);
        assert_eq!(back.dns, p.dns);
        assert_eq!(back.mux_enabled, p.mux_enabled);
        assert_eq!(back.mux_concurrency, p.mux_concurrency);
        assert_eq!(back.mux_xudp_enabled, p.mux_xudp_enabled);
        assert_eq!(back.mux_xudp_concurrency, p.mux_xudp_concurrency);
        assert_eq!(back.mux_xudp_udp443, p.mux_xudp_udp443);
        assert_eq!(back.timeout_secs, p.timeout_secs);
        assert_eq!(back.http_port, p.http_port);
        assert_eq!(back.socks_port, p.socks_port);
        assert_eq!(back.grpc_service, p.grpc_service);
        assert_eq!(back.grpc_multi, p.grpc_multi);
        assert_eq!(back.xhttp_mode, p.xhttp_mode);
        assert_eq!(back.kcp_seed, p.kcp_seed);
        assert_eq!(back.quic_security, p.quic_security);
        assert_eq!(back.tcp_header, p.tcp_header);
        assert_eq!(back.remark, p.remark);
        assert_eq!(back.latency_ms, p.latency_ms);
        assert_eq!(back.last_test, p.last_test);
        assert!(back.tls);
    }

    #[test]
    fn an_old_configuration_file_still_loads() {
        // A file written before the protocol/transport fields existed.
        let json = r#"{
            "version": 1,
            "servers": [{
                "id": "old-1",
                "name": "old",
                "address": "1.2.3.4",
                "port": 443,
                "uuid_sealed": "",
                "tls": true,
                "sni": "ea.com"
            }],
            "settings": {}
        }"#;
        let file: ConfigFile = serde_json::from_str(json).expect("parse");
        let profile: ServerProfile = file.servers.into_iter().next().unwrap().into();
        assert_eq!(profile.protocol, "vmess");
        assert_eq!(profile.security, "auto");
        assert_eq!(profile.network, "tcp");
        assert_eq!(profile.sni, "ea.com");
        // MUX stays off for a profile that was stored before MUX existed.
        assert!(!profile.mux_enabled);
        assert!(!profile.mux_xudp_enabled);
        assert_eq!(profile.mux_xudp_concurrency, 16);
        assert_eq!(profile.mux_xudp_udp443, "reject");
    }

    #[test]
    fn uuid_is_sealed_on_disk_and_restored() {
        // Both DPAPI and the per-user Linux key must preserve credentials.
        let p = sample();
        let file = ServerFile::from(&p);
        assert!(!file.uuid_sealed.is_empty());
        assert!(!file.uuid_sealed.contains(&p.uuid));
        let back: ServerProfile = file.into();
        assert_eq!(back.uuid, p.uuid);
    }

    #[test]
    fn atomic_save_preserves_invalid_existing_file() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"broken original").unwrap();
        assert!(save_at(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken original");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_save_replaces_existing_file_and_uses_private_permissions() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("config.json");
        let first = br#"{"version":1,"servers":[]}"#;
        let second = br#"{"version":1,"servers":[],"selected_server":"edited"}"#;
        save_at(&path, first).unwrap();
        save_at(&path, second).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), second);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
