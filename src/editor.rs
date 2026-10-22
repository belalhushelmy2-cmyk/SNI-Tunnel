//! Server editor logic.
//!
//! This module is deliberately free of I/O and of UI types so it can be unit
//! tested: it maps a [`ServerProfile`] to the flat set of strings the editor
//! shows, validates them, and maps them back.
//!
//! The four knobs that must stay independent — *server address*, *TLS SNI*,
//! *WebSocket Host* and *WebSocket path* — map to four separate fields here,
//! so editing one can never change another.

use crate::model::{
    is_valid_uuid, normalize_transport, ServerProfile, ENCRYPTIONS, FINGERPRINTS, TRANSPORTS,
    UDP443_POLICIES,
};
use std::collections::BTreeMap;

/// Flat representation of a server profile as shown in the editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorFields {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub address: String,
    pub port: String,
    pub uuid: String,
    pub alter_id: String,
    pub security: String,
    pub network: String,
    pub tls: bool,
    pub sni: String,
    pub alpn: String,
    pub fingerprint: String,
    pub allow_insecure: bool,
    pub pinned_cert_sha256: String,
    pub ws_host: String,
    pub ws_path: String,
    pub ws_headers: String,
    pub grpc_service: String,
    pub grpc_multi: bool,
    pub h2_host: String,
    pub h2_path: String,
    pub xhttp_mode: String,
    pub tcp_header: String,
    pub kcp_seed: String,
    pub kcp_header: String,
    pub quic_security: String,
    pub quic_header: String,
    pub dns: String,
    pub mux_enabled: bool,
    pub mux_concurrency: String,
    pub mux_xudp_enabled: bool,
    pub mux_xudp_concurrency: String,
    pub mux_xudp_udp443: String,
    pub timeout_secs: String,
    pub http_port: String,
    pub socks_port: String,
    pub remark: String,
}

impl EditorFields {
    /// Build the editor fields from a stored profile.
    pub fn from_profile(p: &ServerProfile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            protocol: p.protocol.clone(),
            address: p.address.clone(),
            port: p.port.to_string(),
            uuid: p.uuid.clone(),
            alter_id: p.alter_id.to_string(),
            security: p.security.clone(),
            network: p.network.clone(),
            tls: p.tls,
            sni: p.sni.clone(),
            alpn: p.alpn.join(", "),
            fingerprint: p.fingerprint.clone(),
            allow_insecure: p.allow_insecure,
            pinned_cert_sha256: p.pinned_cert_sha256.clone(),
            ws_host: p.ws_host.clone(),
            ws_path: p.ws_path.clone(),
            ws_headers: format_headers(&p.ws_headers),
            grpc_service: p.grpc_service.clone(),
            grpc_multi: p.grpc_multi,
            h2_host: p.h2_host.clone(),
            h2_path: p.h2_path.clone(),
            xhttp_mode: p.xhttp_mode.clone(),
            tcp_header: p.tcp_header.clone(),
            kcp_seed: p.kcp_seed.clone(),
            kcp_header: p.kcp_header.clone(),
            quic_security: p.quic_security.clone(),
            quic_header: p.quic_header.clone(),
            dns: p.dns.clone(),
            mux_enabled: p.mux_enabled,
            mux_concurrency: p.mux_concurrency.to_string(),
            mux_xudp_enabled: p.mux_xudp_enabled,
            mux_xudp_concurrency: p.mux_xudp_concurrency.to_string(),
            mux_xudp_udp443: p.mux_xudp_udp443.clone(),
            timeout_secs: p.timeout_secs.to_string(),
            http_port: if p.http_port == 0 {
                String::new()
            } else {
                p.http_port.to_string()
            },
            socks_port: if p.socks_port == 0 {
                String::new()
            } else {
                p.socks_port.to_string()
            },
            remark: p.remark.clone(),
        }
    }

    /// Validate the fields and fold them back into a profile.
    ///
    /// `base` supplies the values that are not edited here (id, measured
    /// latency, last test result). Every other field comes from the editor.
    pub fn to_profile(&self, base: &ServerProfile) -> Result<ServerProfile, String> {
        let address = self.address.trim().to_string();
        if address.is_empty() {
            return Err("Server address is required".into());
        }
        let port: u16 = self
            .port
            .trim()
            .parse()
            .map_err(|_| "Server port must be a number between 1 and 65535".to_string())?;
        if port == 0 {
            return Err("Server port must be between 1 and 65535".into());
        }
        let uuid = self.uuid.trim().to_string();
        if uuid.is_empty() {
            return Err(match self.protocol.as_str() {
                "trojan" => "Password is required".into(),
                _ => "UUID is required".into(),
            });
        }
        let protocol = if self.protocol.trim().is_empty() {
            "vmess".to_string()
        } else {
            self.protocol.trim().to_ascii_lowercase()
        };
        if !matches!(protocol.as_str(), "vmess" | "vless" | "trojan") {
            return Err(format!("Unsupported protocol: {protocol}"));
        }
        if (protocol == "vmess" || protocol == "vless") && !is_valid_uuid(&uuid) {
            return Err("UUID is not a valid UUID".into());
        }
        let network = normalize_transport(&self.network);
        if !TRANSPORTS.contains(&network.as_str()) {
            return Err(format!(
                "Unsupported transport '{}' (supported: {})",
                self.network,
                TRANSPORTS.join(", ")
            ));
        }
        let alter_id: u32 = self
            .alter_id
            .trim()
            .parse()
            .map_err(|_| "Alter ID must be a number".to_string())?;
        let mux_concurrency: i32 = self
            .mux_concurrency
            .trim()
            .parse()
            .map_err(|_| "Mux concurrency must be a number".to_string())?;
        if mux_concurrency < 0 {
            return Err("Mux concurrency cannot be negative".into());
        }
        let mux_xudp_concurrency: i32 = self
            .mux_xudp_concurrency
            .trim()
            .parse()
            .map_err(|_| "XUDP concurrency must be a number".to_string())?;
        if mux_xudp_concurrency < 0 {
            return Err("XUDP concurrency cannot be negative".into());
        }
        let mux_xudp_udp443 = self.mux_xudp_udp443.trim().to_ascii_lowercase();
        if !UDP443_POLICIES.contains(&mux_xudp_udp443.as_str()) {
            return Err(format!(
                "Unknown UDP/443 policy '{}' (supported: {})",
                mux_xudp_udp443,
                UDP443_POLICIES.join(", ")
            ));
        }
        let timeout_secs: u32 = self
            .timeout_secs
            .trim()
            .parse()
            .map_err(|_| "Timeout must be a number of seconds".to_string())?;
        let http_port = parse_optional_port(&self.http_port, "HTTP proxy port")?;
        let socks_port = parse_optional_port(&self.socks_port, "SOCKS5 proxy port")?;

        let mut ws_path = self.ws_path.trim().to_string();
        if network == "ws" || network == "httpupgrade" || network == "xhttp" {
            if ws_path.is_empty() {
                ws_path = "/".into();
            }
            if !ws_path.starts_with('/') {
                return Err("WebSocket path must start with '/'".into());
            }
        }
        if network == "grpc" && self.grpc_service.trim().is_empty() {
            return Err("gRPC needs a service name".into());
        }

        let mut p = base.clone();
        p.id = if self.id.trim().is_empty() {
            base.id.clone()
        } else {
            self.id.trim().to_string()
        };
        p.name = self.name.trim().to_string();
        p.protocol = protocol;
        p.address = address;
        p.port = port;
        p.uuid = uuid;
        p.alter_id = alter_id;
        let security = if self.security.trim().is_empty() {
            "auto".to_string()
        } else {
            self.security.trim().to_string()
        };
        if !ENCRYPTIONS.contains(&security.as_str()) {
            return Err(format!("Unknown encryption method: {security}"));
        }
        p.security = security;
        p.network = network;
        p.tls = self.tls;
        // The SNI is *only* the TLS server name. It never feeds back into the
        // dial address or the WebSocket Host header.
        p.sni = self.sni.trim().to_string();
        p.alpn = split_list(&self.alpn);
        let fingerprint = self.fingerprint.trim().to_string();
        if !FINGERPRINTS.contains(&fingerprint.as_str()) {
            return Err(format!("Unknown TLS fingerprint: {fingerprint}"));
        }
        p.fingerprint = fingerprint;
        p.allow_insecure = self.allow_insecure;
        let pin = self.pinned_cert_sha256.trim();
        if !pin.is_empty()
            && pin.split(',').any(|p| {
                let hex = p.trim().replace(':', "");
                hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err("Certificate pin must be a SHA-256 hex fingerprint verified with your server administrator".into());
        }
        p.pinned_cert_sha256 = pin.to_string();
        p.ws_host = self.ws_host.trim().to_string();
        p.ws_path = ws_path;
        p.ws_headers = parse_headers(&self.ws_headers)?;
        p.grpc_service = self.grpc_service.trim().to_string();
        p.grpc_multi = self.grpc_multi;
        p.h2_host = self.h2_host.trim().to_string();
        p.h2_path = self.h2_path.trim().to_string();
        p.xhttp_mode = self.xhttp_mode.trim().to_string();
        p.tcp_header = if self.tcp_header.trim().is_empty() {
            "none".into()
        } else {
            self.tcp_header.trim().to_string()
        };
        p.kcp_seed = self.kcp_seed.trim().to_string();
        p.kcp_header = self.kcp_header.trim().to_string();
        p.quic_security = self.quic_security.trim().to_string();
        p.quic_header = self.quic_header.trim().to_string();
        p.dns = self.dns.trim().to_string();
        p.mux_enabled = self.mux_enabled;
        p.mux_concurrency = mux_concurrency;
        p.mux_xudp_enabled = self.mux_xudp_enabled;
        p.mux_xudp_concurrency = mux_xudp_concurrency;
        p.mux_xudp_udp443 = mux_xudp_udp443;
        p.timeout_secs = timeout_secs;
        p.http_port = http_port;
        p.socks_port = socks_port;
        p.remark = self.remark.trim().to_string();
        Ok(p)
    }
}

fn parse_optional_port(text: &str, what: &str) -> Result<u16, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(0);
    }
    let p: u16 = t
        .parse()
        .map_err(|_| format!("{what} must be a number between 1 and 65535"))?;
    if p == 0 {
        return Err(format!("{what} must be between 1 and 65535"));
    }
    Ok(p)
}

/// Split a comma separated list into trimmed, non-empty entries.
pub fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Format extra headers as one `key: value` per line.
pub fn format_headers(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse extra headers written as `key: value` per line.
pub fn parse_headers(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| format!("header line {}: expected 'key: value'", n + 1))?;
        let k = k.trim();
        if k.is_empty() {
            return Err(format!("header line {}: empty name", n + 1));
        }
        out.insert(k.to_string(), v.trim().to_string());
    }
    Ok(out)
}

/// Duplicate a profile: new id, name suffixed, measured state cleared.
pub fn duplicate_profile(p: &ServerProfile) -> ServerProfile {
    let mut copy = p.clone();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.name = if p.name.trim().is_empty() {
        format!("{}:{} (copy)", p.address, p.port)
    } else {
        format!("{} (copy)", p.name)
    };
    copy.latency_ms = 0;
    copy.last_test.clear();
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ServerProfile {
        ServerProfile {
            name: "Tokyo".into(),
            address: "1.2.3.4".into(),
            port: 443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            sni: "ea.com".into(),
            ws_host: "front.example".into(),
            ws_path: "/ws".into(),
            ..Default::default()
        }
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let p = sample();
        let fields = EditorFields::from_profile(&p);
        let back = fields.to_profile(&p).unwrap();
        assert_eq!(back.address, p.address);
        assert_eq!(back.port, p.port);
        assert_eq!(back.uuid, p.uuid);
        assert_eq!(back.sni, p.sni);
        assert_eq!(back.ws_host, p.ws_host);
        assert_eq!(back.ws_path, p.ws_path);
        assert_eq!(back.network, p.network);
        assert!(back.tls);
    }

    #[test]
    fn editing_sni_does_not_touch_address_or_host() {
        let p = sample();
        let mut fields = EditorFields::from_profile(&p);
        fields.sni = "other.example".into();
        let back = fields.to_profile(&p).unwrap();
        assert_eq!(back.sni, "other.example");
        assert_eq!(back.address, "1.2.3.4");
        assert_eq!(back.ws_host, "front.example");
        assert_eq!(back.ws_path, "/ws");
    }

    #[test]
    fn editing_host_does_not_touch_sni() {
        let p = sample();
        let mut fields = EditorFields::from_profile(&p);
        fields.ws_host = "cdn.example".into();
        let back = fields.to_profile(&p).unwrap();
        assert_eq!(back.ws_host, "cdn.example");
        assert_eq!(back.sni, "ea.com");
        assert_eq!(back.address, "1.2.3.4");
    }

    #[test]
    fn imported_sni_is_shown_in_the_editor() {
        let p = sample(); // sni = ea.com
        let fields = EditorFields::from_profile(&p);
        assert_eq!(fields.sni, "ea.com");
    }

    #[test]
    fn validation_rejects_bad_values() {
        let p = sample();
        let base = EditorFields::from_profile(&p);

        let mut f = base.clone();
        f.address = "  ".into();
        assert!(f.to_profile(&p).is_err());

        let mut f = base.clone();
        f.port = "70000".into();
        assert!(f.to_profile(&p).is_err());

        let mut f = base.clone();
        f.uuid = "not-a-uuid".into();
        assert!(f.to_profile(&p).is_err());

        let mut f = base.clone();
        f.network = "carrier-pigeon".into();
        assert!(f.to_profile(&p).is_err());

        let mut f = base.clone();
        f.ws_path = "ws".into();
        assert!(f.to_profile(&p).is_err());
    }

    #[test]
    fn headers_round_trip() {
        let mut h = BTreeMap::new();
        h.insert("X-Token".to_string(), "abc".to_string());
        let text = format_headers(&h);
        assert_eq!(text, "X-Token: abc");
        assert_eq!(parse_headers(&text).unwrap(), h);
        assert!(parse_headers("no colon").is_err());
    }

    #[test]
    fn duplicate_gets_a_new_id_and_clears_measurements() {
        let mut p = sample();
        p.latency_ms = 42;
        p.last_test = "ok".into();
        let d = duplicate_profile(&p);
        assert_ne!(d.id, p.id);
        assert!(d.name.ends_with("(copy)"));
        assert_eq!(d.latency_ms, 0);
        assert!(d.last_test.is_empty());
        assert_eq!(d.uuid, p.uuid);
    }

    #[test]
    fn optional_ports_default_to_zero() {
        let p = sample();
        let fields = EditorFields::from_profile(&p);
        assert_eq!(fields.http_port, "");
        let back = fields.to_profile(&p).unwrap();
        assert_eq!(back.http_port, 0);
    }
}
