use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_HTTP_PORT: u16 = 2080;
pub const DEFAULT_SOCKS_PORT: u16 = 1080;

/// Transports we can generate an Xray configuration for.
pub const TRANSPORTS: &[&str] = &[
    "tcp",
    "ws",
    "h2",
    "grpc",
    "httpupgrade",
    "xhttp",
    "quic",
    "kcp",
];

/// VMess encryption methods offered by Xray.
pub const ENCRYPTIONS: &[&str] = &["auto", "aes-128-gcm", "chacha20-poly1305", "none", "zero"];

/// What Xray does with UDP traffic to port 443 while XUDP is active.
pub const UDP443_POLICIES: &[&str] = &["reject", "allow", "skip"];

/// TLS fingerprints offered by Xray (uTLS).
pub const FINGERPRINTS: &[&str] = &[
    "",
    "chrome",
    "firefox",
    "safari",
    "ios",
    "android",
    "edge",
    "360",
    "qq",
    "random",
    "randomized",
];

fn d_security() -> String {
    "auto".into()
}
fn d_protocol() -> String {
    "vmess".into()
}
fn d_network() -> String {
    "tcp".into()
}
fn d_path() -> String {
    "/".into()
}
fn d_log_level() -> String {
    "warning".into()
}
fn d_mode() -> String {
    if cfg!(any(windows, target_os = "linux")) {
        "system"
    } else {
        "manual"
    }
    .into()
}
fn d_true() -> bool {
    true
}
fn d_mux_concurrency() -> i32 {
    8
}
fn d_mux_xudp_concurrency() -> i32 {
    16
}
fn d_mux_xudp_udp443() -> String {
    "reject".into()
}
fn d_timeout() -> u32 {
    10
}
fn d_tcp_header() -> String {
    "none".into()
}

/// A VMess server profile. `address`/`port` are the *connection* endpoints;
/// the TLS server name (SNI) and HTTP Host are stored independently so that a
/// user can share one server between many SNI configurations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerProfile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub uuid: String,
    /// The protocol of the outbound: `vmess`, `vless` or `trojan`.
    ///
    /// For `trojan` the `uuid` field carries the password.
    #[serde(default = "d_protocol")]
    pub protocol: String,
    #[serde(default = "d_security")]
    pub security: String,
    #[serde(default = "d_network")]
    pub network: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub sni: String,
    #[serde(default)]
    pub alpn: Vec<String>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default)]
    pub ws_host: String,
    #[serde(default = "d_path")]
    pub ws_path: String,
    #[serde(default)]
    pub grpc_service: String,
    #[serde(default)]
    pub h2_host: String,
    #[serde(default = "d_path")]
    pub h2_path: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub flow: String,
    #[serde(default)]
    pub alter_id: u32,
    /// SHA-256 of the server's leaf certificate (hex).
    ///
    /// Xray 26 removed `allowInsecure`; the supported way to trust a
    /// certificate that does not match the SNI is to *pin* it. When the
    /// profile has `allow_insecure` set and no pin, the presented certificate
    /// is fetched without verification and pinned for that session only.
    #[serde(default)]
    pub pinned_cert_sha256: String,
    // ---- remarks / extra headers ----
    #[serde(default)]
    pub remark: String,
    #[serde(default)]
    pub ws_headers: BTreeMap<String, String>,
    // ---- advanced ----
    /// Per-server DNS override (comma separated). Empty = use the global one.
    #[serde(default)]
    pub dns: String,
    #[serde(default)]
    pub mux_enabled: bool,
    #[serde(default = "d_mux_concurrency")]
    pub mux_concurrency: i32,
    /// Carry UDP over the multiplexed connection (XUDP).
    ///
    /// Only used when the installed Xray-core supports it; the app probes the
    /// core before it writes the fields.
    #[serde(default)]
    pub mux_xudp_enabled: bool,
    /// How many UDP streams may share the multiplexed connection.
    #[serde(default = "d_mux_xudp_concurrency")]
    pub mux_xudp_concurrency: i32,
    /// What happens to UDP traffic to port 443: `reject`, `allow` or `skip`.
    #[serde(default = "d_mux_xudp_udp443")]
    pub mux_xudp_udp443: String,
    /// Client-side connect timeout in seconds.
    #[serde(default = "d_timeout")]
    pub timeout_secs: u32,
    /// Per-server local proxy ports; 0 = use the global setting.
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub socks_port: u16,
    // ---- transport specifics ----
    #[serde(default = "d_tcp_header")]
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
    // ---- measured state (not part of the tunnel config) ----
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub last_test: String,
}

impl Default for ServerProfile {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            address: String::new(),
            port: 443,
            uuid: String::new(),
            protocol: d_protocol(),
            security: d_security(),
            network: d_network(),
            tls: true,
            sni: String::new(),
            alpn: Vec::new(),
            allow_insecure: false,
            ws_host: String::new(),
            ws_path: d_path(),
            grpc_service: String::new(),
            h2_host: String::new(),
            h2_path: d_path(),
            fingerprint: String::new(),
            flow: String::new(),
            alter_id: 0,
            pinned_cert_sha256: String::new(),
            remark: String::new(),
            ws_headers: BTreeMap::new(),
            dns: String::new(),
            mux_enabled: false,
            mux_concurrency: d_mux_concurrency(),
            mux_xudp_enabled: false,
            mux_xudp_concurrency: d_mux_xudp_concurrency(),
            mux_xudp_udp443: d_mux_xudp_udp443(),
            timeout_secs: d_timeout(),
            http_port: 0,
            socks_port: 0,
            tcp_header: d_tcp_header(),
            grpc_multi: false,
            xhttp_mode: String::new(),
            kcp_seed: String::new(),
            kcp_header: String::new(),
            quic_security: String::new(),
            quic_header: String::new(),
            latency_ms: 0,
            last_test: String::new(),
        }
    }
}

/// A named override of the TLS/transport parameters of a server. Selecting an
/// SNI profile changes *only* SNI / Host / path / ALPN (and optionally the
/// dial address), never the VMess credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SniProfile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub tls: bool,
    /// Legacy Linux overrides inherit transport security unless SNI explicitly enables TLS.
    #[serde(default)]
    pub inherit_tls: bool,
    #[serde(default)]
    pub sni: String,
    #[serde(default)]
    pub connect_address: String,
    #[serde(default)]
    pub connect_port: u16,
    #[serde(default)]
    pub ws_host: String,
    #[serde(default)]
    pub ws_path: String,
    #[serde(default)]
    pub alpn: Vec<String>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default)]
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HealthSettings {
    pub enabled: bool,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub reconnect_attempts: u32,
    pub notifications: bool,
}
impl Default for HealthSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 60,
            timeout_secs: 10,
            reconnect_attempts: 5,
            notifications: true,
        }
    }
}
impl HealthSettings {
    pub fn interval(&self) -> u64 {
        self.interval_secs.clamp(15, 3600)
    }
    pub fn timeout(&self) -> u64 {
        self.timeout_secs.clamp(3, 30)
    }
    pub fn attempts(&self) -> u32 {
        self.reconnect_attempts.clamp(1, 10)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Use installed NetMod 1.6.1 for the remote leg, with modern native TUN.
    #[serde(default)]
    pub netmod_compatibility: bool,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "d_mode")]
    pub mode: String,
    #[serde(default = "d_true")]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub health: HealthSettings,
    #[serde(default)]
    pub performance_mode: crate::performance::PerformanceMode,
    #[serde(default = "d_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "d_true")]
    pub verify_core_hash: bool,
    /// Block traffic from escaping the tunnel when the VMess server is down.
    #[serde(default = "d_true")]
    pub kill_switch: bool,
    #[serde(default)]
    pub custom_dns: String,
    #[serde(default = "d_log_level")]
    pub log_level: String,
    /// What the log shows: `simple` (the connection events) or `advanced`
    /// (everything, including the core's debug output).
    #[serde(default = "d_log_mode")]
    pub log_mode: String,
    // ---- the defaults for newly created server profiles ----
    /// Should a new profile have MUX switched on?
    #[serde(default)]
    pub default_mux_enabled: bool,
    /// The MUX concurrency a new profile gets.
    #[serde(default = "d_default_mux_concurrency")]
    pub default_mux_concurrency: i32,
    /// Should a new profile carry UDP over the multiplexed connection?
    #[serde(default)]
    pub default_mux_xudp_enabled: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub compact_layout: bool,
}

fn default_http_port() -> u16 {
    DEFAULT_HTTP_PORT
}
fn default_socks_port() -> u16 {
    DEFAULT_SOCKS_PORT
}

/// The log shows the connection events until the user asks for more.
fn d_log_mode() -> String {
    "simple".into()
}

/// The MUX concurrency a new profile gets.
fn d_default_mux_concurrency() -> i32 {
    8
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            netmod_compatibility: false,
            http_port: DEFAULT_HTTP_PORT,
            socks_port: DEFAULT_SOCKS_PORT,
            mode: d_mode(),
            auto_reconnect: true,
            health: HealthSettings::default(),
            performance_mode: crate::performance::PerformanceMode::Balanced,
            minimize_to_tray: true,
            start_with_windows: false,
            verify_core_hash: true,
            kill_switch: true,
            custom_dns: String::new(),
            log_level: d_log_level(),
            log_mode: d_log_mode(),
            default_mux_enabled: false,
            default_mux_concurrency: d_default_mux_concurrency(),
            default_mux_xudp_enabled: false,
            reduced_motion: false,
            compact_layout: false,
        }
    }
}

impl Settings {
    /// The MUX settings a *newly created* profile gets.
    ///
    /// A profile that already exists keeps its own settings - the global
    /// defaults are only used when a profile is created.
    pub fn apply_new_profile_defaults(&self, p: &mut ServerProfile) {
        p.mux_enabled = self.default_mux_enabled;
        p.mux_concurrency = self.default_mux_concurrency.max(1);
        p.mux_xudp_enabled = self.default_mux_xudp_enabled;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub servers: Vec<ServerProfile>,
    #[serde(default)]
    pub sni_profiles: Vec<SniProfile>,
    #[serde(default)]
    pub selected_server: Option<String>,
    #[serde(default)]
    pub selected_sni_profile: Option<String>,
    #[serde(default)]
    pub settings: Settings,
}

impl AppConfig {
    pub fn server_by_id(&self, id: &str) -> Option<&ServerProfile> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn selected_server_profile(&self) -> Option<&ServerProfile> {
        self.selected_server
            .as_deref()
            .and_then(|id| self.server_by_id(id))
            .or_else(|| self.servers.first())
    }

    pub fn selected_sni(&self) -> Option<&SniProfile> {
        self.selected_sni_profile
            .as_deref()
            .and_then(|id| self.sni_profiles.iter().find(|p| p.id == id))
    }
}

/// Fully resolved connection parameters used to build an Xray outbound.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub uuid: String,
    pub protocol: String,
    pub security: String,
    pub network: String,
    pub tls: bool,
    pub sni: String,
    pub alpn: Vec<String>,
    pub allow_insecure: bool,
    pub ws_host: String,
    pub ws_path: String,
    pub grpc_service: String,
    pub h2_host: String,
    pub h2_path: String,
    pub fingerprint: String,
    pub flow: String,
    pub alter_id: u32,
    pub pinned_cert_sha256: String,
    pub remark: String,
    pub ws_headers: BTreeMap<String, String>,
    pub dns: String,
    pub mux_enabled: bool,
    pub mux_concurrency: i32,
    pub mux_xudp_enabled: bool,
    pub mux_xudp_concurrency: i32,
    pub mux_xudp_udp443: String,
    pub timeout_secs: u32,
    pub http_port: u16,
    pub socks_port: u16,
    pub tcp_header: String,
    pub grpc_multi: bool,
    pub xhttp_mode: String,
    pub kcp_seed: String,
    pub kcp_header: String,
    pub quic_security: String,
    pub quic_header: String,
}

impl Default for Endpoint {
    fn default() -> Self {
        Self {
            name: String::new(),
            address: String::new(),
            port: 0,
            uuid: String::new(),
            protocol: "vmess".into(),
            security: "auto".into(),
            network: "tcp".into(),
            tls: false,
            sni: String::new(),
            alpn: Vec::new(),
            allow_insecure: false,
            ws_host: String::new(),
            ws_path: "/".into(),
            grpc_service: String::new(),
            h2_host: String::new(),
            h2_path: "/".into(),
            fingerprint: String::new(),
            flow: String::new(),
            alter_id: 0,
            pinned_cert_sha256: String::new(),
            remark: String::new(),
            ws_headers: BTreeMap::new(),
            dns: String::new(),
            mux_enabled: false,
            mux_concurrency: 8,
            mux_xudp_enabled: false,
            mux_xudp_concurrency: 16,
            mux_xudp_udp443: "reject".into(),
            timeout_secs: 10,
            http_port: 0,
            socks_port: 0,
            tcp_header: "none".into(),
            grpc_multi: false,
            xhttp_mode: String::new(),
            kcp_seed: String::new(),
            kcp_header: String::new(),
            quic_security: String::new(),
            quic_header: String::new(),
        }
    }
}

/// Merge a server with an optional SNI profile. Address / SNI / Host remain
/// independent knobs.
pub fn resolve_endpoint(server: &ServerProfile, sni: Option<&SniProfile>) -> Endpoint {
    let mut ep = Endpoint {
        name: server.name.clone(),
        address: server.address.clone(),
        port: server.port,
        uuid: server.uuid.clone(),
        protocol: server.protocol.clone(),
        security: server.security.clone(),
        network: server.network.clone(),
        tls: server.tls,
        sni: server.sni.clone(),
        alpn: server.alpn.clone(),
        allow_insecure: server.allow_insecure,
        ws_host: server.ws_host.clone(),
        ws_path: server.ws_path.clone(),
        grpc_service: server.grpc_service.clone(),
        h2_host: server.h2_host.clone(),
        h2_path: server.h2_path.clone(),
        fingerprint: server.fingerprint.clone(),
        flow: server.flow.clone(),
        alter_id: server.alter_id,
        pinned_cert_sha256: server.pinned_cert_sha256.clone(),
        remark: server.remark.clone(),
        ws_headers: server.ws_headers.clone(),
        dns: server.dns.clone(),
        mux_enabled: server.mux_enabled,
        mux_concurrency: server.mux_concurrency,
        mux_xudp_enabled: server.mux_xudp_enabled,
        mux_xudp_concurrency: server.mux_xudp_concurrency,
        mux_xudp_udp443: server.mux_xudp_udp443.clone(),
        timeout_secs: server.timeout_secs,
        http_port: server.http_port,
        socks_port: server.socks_port,
        tcp_header: server.tcp_header.clone(),
        grpc_multi: server.grpc_multi,
        xhttp_mode: server.xhttp_mode.clone(),
        kcp_seed: server.kcp_seed.clone(),
        kcp_header: server.kcp_header.clone(),
        quic_security: server.quic_security.clone(),
        quic_header: server.quic_header.clone(),
    };

    if let Some(p) = sni {
        ep.name = if p.name.is_empty() {
            ep.name
        } else {
            format!("{} [{}]", ep.name, p.name)
        };
        if !p.connect_address.trim().is_empty() {
            ep.address = p.connect_address.trim().to_string();
        }
        if p.connect_port != 0 {
            ep.port = p.connect_port;
        }
        ep.tls = if p.inherit_tls {
            server.tls || !p.sni.trim().is_empty()
        } else {
            p.tls
        };
        if !p.sni.trim().is_empty() {
            ep.sni = p.sni.trim().to_string();
        }
        if !p.ws_host.trim().is_empty() {
            ep.ws_host = p.ws_host.trim().to_string();
        }
        if !p.ws_path.trim().is_empty() {
            ep.ws_path = p.ws_path.clone();
        }
        if !p.alpn.is_empty() {
            ep.alpn = p.alpn.clone();
        }
        if !p.fingerprint.trim().is_empty() {
            ep.fingerprint = p.fingerprint.trim().to_string();
        }
        ep.allow_insecure = p.allow_insecure;
    }

    // Default the SNI to the dial address when TLS is enabled but no explicit
    // server name was provided (mirrors how most clients behave).
    if ep.tls && ep.sni.trim().is_empty() {
        ep.sni = ep.address.clone();
    }
    if ep.ws_host.trim().is_empty() {
        ep.ws_host = if !ep.sni.is_empty() {
            ep.sni.clone()
        } else {
            ep.address.clone()
        };
    }
    if ep.h2_host.trim().is_empty() {
        ep.h2_host = ep.ws_host.clone();
    }
    ep
}

impl Endpoint {
    /// Validate that the endpoint has the minimum required fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.address.trim().is_empty() {
            return Err("Server address is empty".into());
        }
        if self.port == 0 {
            return Err("Server port is 0".into());
        }
        if self.uuid.trim().is_empty() {
            return Err(match self.protocol.as_str() {
                "trojan" => "Password is empty".into(),
                _ => "UUID is empty".into(),
            });
        }
        if self.protocol == "vmess" || self.protocol == "vless" {
            if !is_valid_uuid(self.uuid.trim()) {
                return Err("UUID is not a valid UUID".into());
            }
        } else if !matches!(self.protocol.as_str(), "vmess" | "vless" | "trojan") {
            return Err(format!("Unsupported protocol: {}", self.protocol));
        }
        if !TRANSPORTS.contains(&self.network.as_str()) {
            return Err(format!(
                "Unsupported transport: {} (supported: {})",
                self.network,
                TRANSPORTS.join(", ")
            ));
        }
        if self.network == "grpc" && self.grpc_service.trim().is_empty() {
            return Err("gRPC needs a service name".into());
        }
        if self.network == "ws" && !self.ws_path.trim().is_empty() && !self.ws_path.starts_with('/')
        {
            return Err("WebSocket path must start with '/'".into());
        }
        Ok(())
    }
}

/// Normalize a transport name coming from an import or the editor.
pub fn normalize_transport(net: &str) -> String {
    match net.trim().to_ascii_lowercase().as_str() {
        "" | "tcp" | "raw" | "none" => "tcp".into(),
        "ws" | "websocket" => "ws".into(),
        "h2" | "http" | "h2c" | "http2" => "h2".into(),
        "grpc" | "gun" => "grpc".into(),
        "httpupgrade" | "http-upgrade" => "httpupgrade".into(),
        "xhttp" | "splithttp" | "split-http" => "xhttp".into(),
        "quic" => "quic".into(),
        "kcp" | "mkcp" => "kcp".into(),
        other => other.to_string(),
    }
}

pub fn is_valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sni_profile_only_touches_transport_fields() {
        let mut s = ServerProfile {
            name: "srv".into(),
            ..Default::default()
        };
        s.address = "1.2.3.4".into();
        s.port = 443;
        s.uuid = "b831381d-6324-4d53-ad4f-8cda48b30811".into();
        s.tls = true;
        s.sni = "original.example".into();
        s.network = "ws".into();

        let p = SniProfile {
            tls: true,
            sni: "cdn.example".into(),
            ws_host: "cdn.example".into(),
            connect_address: "5.6.7.8".into(),
            ..Default::default()
        };
        let ep = resolve_endpoint(&s, Some(&p));
        // credentials untouched
        assert_eq!(ep.uuid, s.uuid);
        // independent knobs applied
        assert_eq!(ep.address, "5.6.7.8");
        assert_eq!(ep.port, 443);
        assert_eq!(ep.sni, "cdn.example");
        assert_eq!(ep.ws_host, "cdn.example");
        // original profile untouched
        assert_eq!(s.sni, "original.example");
        assert_eq!(s.address, "1.2.3.4");
    }

    #[test]
    fn sni_defaults_to_address() {
        let s = ServerProfile {
            address: "example.com".into(),
            port: 443,
            tls: true,
            ..Default::default()
        };
        let ep = resolve_endpoint(&s, None);
        assert_eq!(ep.sni, "example.com");
    }
}

#[cfg(test)]
mod health_settings_tests {
    use super::*;
    #[test]
    fn old_preferences_survive_new_health_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{"auto_reconnect":false,"kill_switch":false,"custom_dns":"192.0.2.53"}"#,
        )
        .unwrap();
        assert!(!settings.auto_reconnect && !settings.kill_switch);
        assert_eq!(settings.custom_dns, "192.0.2.53");
        assert_eq!(settings.health.interval(), 60);
        assert!(settings.health.enabled && settings.health.notifications);
        let again: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(again, settings);
    }
    #[test]
    fn defaults_and_bounds_are_safe() {
        assert!(Settings::default().auto_reconnect);
        let health: HealthSettings = serde_json::from_str(
            r#"{"interval_secs":0,"timeout_secs":999,"reconnect_attempts":999}"#,
        )
        .unwrap();
        assert_eq!(health.interval(), 15);
        assert_eq!(health.timeout(), 30);
        assert_eq!(health.attempts(), 10);
    }
}
