//! Connection diagnostics and configuration comparison.
//!
//! Two jobs:
//!
//! 1. **Show the effective configuration** that the app hands to Xray-core,
//!    with every secret redacted, together with the settings that actually
//!    reach the wire (SNI, transport, ALPN, fingerprint, MUX) and the traffic
//!    that *bypasses* the tunnel.
//! 2. **Compare** the app's profile with a configuration exported by another
//!    client (Nekoray / NetMod / v2rayN), so the differences that change the
//!    behaviour on the wire are visible without ever printing a credential.

use crate::model::{resolve_endpoint, AppConfig, ServerProfile};
use crate::vmess;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;

/// Redact a secret: show only a short, non-reversible hint.
pub fn redact(secret: &str) -> String {
    if secret.trim().is_empty() {
        return "(empty)".into();
    }
    let digest = crate::util::short_hash(secret);
    format!("***{digest}***")
}

/// The Xray configuration the app would generate, with the secrets removed.
pub fn redacted_config(cfg: &Value) -> Value {
    crate::effective_diag::redact(cfg)
}

/// The settings that end up in the TLS ClientHello and in the connection to the
/// server - the ones a network operator can see.
#[derive(Debug, Clone, Default)]
pub struct WireSettings {
    pub address: String,
    pub port: u16,
    pub protocol: String,
    pub transport: String,
    pub tls: bool,
    pub sni: String,
    pub alpn: Vec<String>,
    pub fingerprint: String,
    pub ws_host: String,
    pub ws_path: String,
    pub mux_enabled: bool,
    pub mux_concurrency: i32,
    pub dns: String,
    pub allow_insecure: bool,
    pub pinned_cert: String,
    pub mux_xudp_enabled: bool,
    pub mux_xudp_concurrency: i32,
    pub mux_xudp_udp443: String,
}

impl WireSettings {
    pub fn from_profile(p: &ServerProfile) -> Self {
        let ep = resolve_endpoint(p, None);
        Self {
            address: ep.address.clone(),
            port: ep.port,
            protocol: ep.protocol.clone(),
            transport: ep.network.clone(),
            tls: ep.tls,
            sni: ep.sni.clone(),
            alpn: ep.alpn.clone(),
            fingerprint: ep.fingerprint.clone(),
            ws_host: ep.ws_host.clone(),
            ws_path: ep.ws_path.clone(),
            mux_enabled: ep.mux_enabled,
            mux_concurrency: ep.mux_concurrency,
            mux_xudp_enabled: ep.mux_xudp_enabled,
            mux_xudp_concurrency: ep.mux_xudp_concurrency,
            mux_xudp_udp443: ep.mux_xudp_udp443.clone(),
            dns: ep.dns.clone(),
            allow_insecure: ep.allow_insecure,
            pinned_cert: ep.pinned_cert_sha256.clone(),
        }
    }

    /// A human readable summary of the settings that matter.
    pub fn report(&self) -> String {
        let alpn = if self.alpn.is_empty() {
            "(none)".to_string()
        } else {
            self.alpn.join(",")
        };
        format!(
            "server      : {}:{} ({})\n\
             transport   : {}\n\
             TLS         : {}\n\
             SNI         : {}\n\
             ALPN        : {}\n\
             fingerprint : {}\n\
             WS host     : {}\n\
             WS path     : {}\n\
             MUX         : {} (concurrency {})\n\
             XUDP        : {} (streams {}, UDP/443 {})\n\
             DNS         : {}\n\
             certificate : {}",
            self.address,
            self.port,
            self.protocol,
            self.transport,
            if self.tls { "on" } else { "off" },
            if self.sni.is_empty() {
                "(none)"
            } else {
                &self.sni
            },
            alpn,
            if self.fingerprint.is_empty() {
                "(none)"
            } else {
                &self.fingerprint
            },
            if self.ws_host.is_empty() {
                "(none)"
            } else {
                &self.ws_host
            },
            if self.ws_path.is_empty() {
                "(none)"
            } else {
                &self.ws_path
            },
            if self.mux_enabled { "on" } else { "off" },
            self.mux_concurrency,
            if self.mux_xudp_enabled { "on" } else { "off" },
            self.mux_xudp_concurrency,
            self.mux_xudp_udp443,
            if self.dns.is_empty() {
                "(system resolver)"
            } else {
                &self.dns
            },
            if !self.pinned_cert.is_empty() {
                "pinned (sha256 verified)"
            } else if self.allow_insecure {
                "unverified (not allowed by Xray 26 - pin it instead)"
            } else {
                "verified"
            }
        )
    }
}

/// A difference between the app's profile and an imported configuration.
#[derive(Debug, Clone)]
pub struct Difference {
    pub setting: &'static str,
    pub theirs: String,
    pub ours: String,
    /// Does this change what the network sees?
    pub on_the_wire: bool,
}

/// Compare the app's profile with a configuration exported by another client.
///
/// `imported` may be an Xray configuration, a sing-box configuration or a
/// Clash configuration. Only the settings that change the behaviour are
/// compared, and no credential is ever printed.
pub fn compare(ours: &ServerProfile, imported: &str) -> Result<Vec<Difference>> {
    let theirs = match parse_imported(imported)? {
        Imported::Xray(v) => settings_from_xray(&v),
        Imported::SingBox(v) => settings_from_singbox(&v),
        Imported::Clash(v) => settings_from_clash(&v),
    };
    let mine = WireSettings::from_profile(ours);
    Ok(diff(&mine, &theirs))
}

fn diff(mine: &WireSettings, theirs: &WireSettings) -> Vec<Difference> {
    let mut out = Vec::new();
    let mut push = |setting: &'static str, ours: String, theirs: String, wire: bool| {
        if ours != theirs {
            out.push(Difference {
                setting,
                theirs,
                ours,
                on_the_wire: wire,
            });
        }
    };

    push(
        "server address",
        mine.address.clone(),
        theirs.address.clone(),
        true,
    );
    push(
        "server port",
        mine.port.to_string(),
        theirs.port.to_string(),
        true,
    );
    push(
        "protocol",
        mine.protocol.clone(),
        theirs.protocol.clone(),
        true,
    );
    push(
        "transport",
        mine.transport.clone(),
        theirs.transport.clone(),
        true,
    );
    push("TLS", yes_no(mine.tls), yes_no(theirs.tls), true);
    push("TLS SNI", mine.sni.clone(), theirs.sni.clone(), true);
    push("ALPN", mine.alpn.join(","), theirs.alpn.join(","), true);
    push(
        "TLS fingerprint",
        mine.fingerprint.clone(),
        theirs.fingerprint.clone(),
        true,
    );
    push(
        "WS host",
        mine.ws_host.clone(),
        theirs.ws_host.clone(),
        true,
    );
    push(
        "WS path",
        mine.ws_path.clone(),
        theirs.ws_path.clone(),
        true,
    );
    push(
        "MUX",
        format!("{} ({})", yes_no(mine.mux_enabled), mine.mux_concurrency),
        format!(
            "{} ({})",
            yes_no(theirs.mux_enabled),
            theirs.mux_concurrency
        ),
        true,
    );
    push("DNS", mine.dns.clone(), theirs.dns.clone(), false);
    out
}

fn yes_no(b: bool) -> String {
    if b {
        "on".into()
    } else {
        "off".into()
    }
}

/// A configuration exported by another client.
pub enum Imported {
    Xray(Value),
    SingBox(Value),
    Clash(Value),
}

/// Parse an exported configuration. JSON is parsed as Xray or sing-box,
/// YAML is treated as a Clash configuration.
pub fn parse_imported(text: &str) -> Result<Imported> {
    // A file written by a Windows editor can start with a UTF-8 BOM.
    let trimmed = text.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let v: Value =
            serde_json::from_str(trimmed).context("the imported file is not valid JSON")?;
        // Xray outbounds have a "protocol", sing-box outbounds a "type".
        let first = v
            .get("outbounds")
            .and_then(|o| o.as_array())
            .and_then(|a| a.first());
        let is_xray = first
            .and_then(|o| o.get("protocol"))
            .and_then(|p| p.as_str())
            .is_some();
        return Ok(if is_xray {
            Imported::Xray(v)
        } else {
            Imported::SingBox(v)
        });
    }
    if trimmed.contains("proxies:") || trimmed.contains("proxy-groups:") {
        return Ok(Imported::Clash(parse_clash(trimmed)));
    }
    anyhow::bail!(
        "the imported file is not recognised (expected an Xray/sing-box JSON or a Clash YAML)"
    )
}

/// Extract the interesting settings from an Xray configuration.
fn settings_from_xray(v: &Value) -> WireSettings {
    let mut s = WireSettings::default();
    let Some(ob) = v.get("outbounds").and_then(|o| o.as_array()).and_then(|a| {
        a.iter()
            .find(|o| matches!(o["protocol"].as_str(), Some("vmess" | "vless" | "trojan")))
    }) else {
        return s;
    };
    s.protocol = ob
        .get("protocol")
        .and_then(|p| p.as_str())
        .unwrap_or_default()
        .to_string();
    // VMess / VLESS
    if let Some(vnext) = ob.pointer("/settings/vnext/0") {
        s.address = vnext
            .get("address")
            .and_then(|a| a.as_str())
            .unwrap_or_default()
            .to_string();
        s.port = vnext.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
    }
    // Trojan
    if let Some(srv) = ob.pointer("/settings/servers/0") {
        s.address = srv
            .get("address")
            .and_then(|a| a.as_str())
            .unwrap_or_default()
            .to_string();
        s.port = srv.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
    }
    let stream = ob.get("streamSettings");
    if let Some(st) = stream {
        s.transport = st
            .get("network")
            .and_then(|n| n.as_str())
            .unwrap_or("tcp")
            .to_string();
        s.tls = st.get("security").and_then(|x| x.as_str()) == Some("tls");
        if let Some(tls) = st.get("tlsSettings") {
            s.sni = tls
                .get("serverName")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            s.fingerprint = tls
                .get("fingerprint")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            s.alpn = tls
                .get("alpn")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
        }
        if let Some(ws) = st.get("wsSettings") {
            s.ws_path = ws
                .get("path")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            s.ws_host = ws
                .get("host")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    ws.pointer("/headers/Host")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
        }
    }
    if let Some(mux) = ob.get("mux") {
        s.mux_enabled = mux
            .get("enabled")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        s.mux_concurrency = mux.get("concurrency").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
    }
    if let Some(dns) = v.get("dns") {
        if let Some(servers) = dns.get("servers").and_then(|x| x.as_array()) {
            s.dns = servers
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
                .join(",");
        }
    }
    s
}

/// Extract the interesting settings from a sing-box configuration.
fn settings_from_singbox(v: &Value) -> WireSettings {
    let mut s = WireSettings::default();
    let Some(ob) = v
        .get("outbounds")
        .and_then(|o| o.as_array())
        .and_then(|a| a.iter().find(|o| o.get("type").is_some()))
    else {
        return s;
    };
    s.protocol = ob
        .get("type")
        .and_then(|p| p.as_str())
        .unwrap_or_default()
        .to_string();
    s.address = ob
        .get("server")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    s.port = ob.get("server_port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
    if let Some(tls) = ob.get("tls") {
        s.tls = true;
        s.sni = tls
            .get("server_name")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        s.fingerprint = tls
            .get("utls")
            .and_then(|u| u.get("fingerprint"))
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        s.alpn = tls
            .get("alpn")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
    }
    if let Some(tr) = ob.get("transport") {
        s.transport = tr
            .get("type")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        // sing-box puts the WebSocket fields directly under "transport"; some
        // exports nest them under "ws".
        let ws = tr.get("ws").unwrap_or(tr);
        if s.transport == "ws" {
            s.ws_path = ws
                .get("path")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            s.ws_host = ws
                .get("headers")
                .and_then(|h| h.get("Host"))
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
        }
    }
    if let Some(mux) = ob.get("multiplex") {
        s.mux_enabled = mux
            .get("enabled")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        s.mux_concurrency = mux
            .get("max_connections")
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as i32;
    }
    s
}

/// A deliberately small reader for the fields of a Clash proxy entry.
///
/// Only the first proxy of the file is inspected; the goal is to compare the
/// settings that reach the wire, not to be a full YAML implementation.
fn parse_clash(text: &str) -> Value {
    let mut map = serde_json::Map::new();
    let mut in_proxies = false;
    let mut first = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            in_proxies = line.trim_start().starts_with("proxies:");
            if in_proxies {
                first = true;
            }
            continue;
        }
        if !in_proxies || !first {
            continue;
        }
        let t = line.trim();
        if let Some((k, v)) = t.split_once(':') {
            let key = k.trim().trim_start_matches('-').trim();
            let value = v.trim().trim_matches('"').trim_matches('\'').to_string();
            if value.is_empty() {
                continue;
            }
            match key {
                "name" => {
                    if map.contains_key("name") {
                        first = false; // the next entry
                        continue;
                    }
                    map.insert("name".into(), Value::String(value));
                }
                "server" => {
                    map.insert("server".into(), Value::String(value));
                }
                "port" => {
                    map.insert("port".into(), Value::String(value));
                }
                "uuid" => {
                    map.insert("uuid".into(), Value::String(value));
                }
                "sni" => {
                    map.insert("sni".into(), Value::String(value));
                }
                "network" => {
                    map.insert("network".into(), Value::String(value));
                }
                "alpn" => {
                    map.insert("alpn".into(), Value::String(value));
                }
                "client-fingerprint" => {
                    map.insert("fingerprint".into(), Value::String(value));
                }
                "servername" => {
                    map.insert("sni".into(), Value::String(value));
                }
                _ => {}
            }
            // Nested blocks (ws-opts) are handled below.
            if key == "ws-opts" {
                // The following indented lines belong to this block.
                continue;
            }
        }
    }
    Value::Object(map)
}

/// Extract the settings from the (simplified) Clash proxy.
fn settings_from_clash(v: &Value) -> WireSettings {
    let get = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let s = WireSettings {
        address: get("server"),
        port: get("port").parse().unwrap_or(0),
        protocol: "vmess".into(),
        transport: {
            let n = get("network");
            if n.is_empty() {
                "tcp".into()
            } else {
                n
            }
        },
        tls: !get("sni").is_empty(),
        sni: get("sni"),
        alpn: get("alpn")
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
        fingerprint: get("fingerprint"),
        ..Default::default()
    };
    s
}

/// Which traffic does *not* go through the tunnel?
///
/// The list is what an operator can see on the wire, so it is the place where
/// a "main quota" instead of a "gaming quota" can come from.
pub fn bypass_summary(cfg: &AppConfig, tun_active: bool) -> Vec<String> {
    let mut out = Vec::new();
    let Some(server) = cfg.selected_server_profile() else {
        return out;
    };
    let ep = resolve_endpoint(server, cfg.selected_sni());

    // The connection to the server itself always leaves directly, otherwise
    // the tunnel could not reach the server.
    out.push(format!(
        "the connection to the server {}:{} (unavoidable, it carries the tunnel)",
        ep.address, ep.port
    ));

    if !tun_active {
        out.push(
            "system proxy mode: only applications that read the proxy settings are tunnelled"
                .to_string(),
        );
        return out;
    }

    out.push("the local network (on-link subnets) is never tunnelled".to_string());
    out.push(
        "the DNS of the physical adapters is not tunnelled unless the tunnel hijacks it"
            .to_string(),
    );
    if ep.tls {
        out.push(format!(
            "the TLS SNI of the tunnel is '{}' - this is what the operator sees",
            if ep.sni.is_empty() {
                "the server address"
            } else {
                &ep.sni
            }
        ));
    }
    out
}

/// The host the public IP is measured with. The same host is used for the
/// ordinary and for the proxied request, so the two addresses are comparable.
pub const MEASUREMENT_HOST: &str = "api.ipify.org";

/// The result of a full-tunnel verification.
#[derive(Debug, Clone, Default)]
pub struct TunnelVerdict {
    /// The exit address of an ordinary (unproxied) system request.
    pub ordinary_exit: Option<String>,
    /// The exit address of a request that went through the local proxy.
    pub proxied_exit: Option<String>,
    /// The exit address observed before the tunnel was started.
    pub baseline: Option<String>,
    /// The bytes the TUN adapter carried during the test.
    pub tunnel_bytes: Option<(u64, u64)>,
    /// The bytes the physical uplink sent during the test (a leak indicator).
    pub uplink_bytes: Option<u64>,
    /// The open connections the TUN carried.
    pub connections: u32,
    /// The split routes that point into the TUN: `(ipv4, ipv6)`.
    pub routes: (usize, usize),
    /// The addresses of the host the measurement uses.
    pub measurement_addrs: Vec<String>,
    /// What went wrong (empty = everything is fine).
    pub problems: Vec<String>,
    /// What is worth knowing even when everything is fine.
    pub notes: Vec<String>,
}

impl TunnelVerdict {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }

    /// A multi line report for the log and the dashboard.
    pub fn report(&self) -> String {
        let mut out = format!(
            "ordinary exit : {}\n\
             proxied exit  : {}\n\
             exit before   : {}\n\
             TUN routes    : {} IPv4, {} IPv6\n\
             measured host : {}\n\
             tunnel bytes  : {}\n\
             uplink bytes  : {}\n\
             connections   : {}\n\
             result        : {}",
            self.ordinary_exit.as_deref().unwrap_or("(failed)"),
            self.proxied_exit.as_deref().unwrap_or("(failed)"),
            self.baseline.as_deref().unwrap_or("(unknown)"),
            self.routes.0,
            self.routes.1,
            if self.measurement_addrs.is_empty() {
                "(unknown)".to_string()
            } else {
                self.measurement_addrs.join(", ")
            },
            match self.tunnel_bytes {
                Some((r, t)) => format!("{r} in / {t} out"),
                None => "(no counter)".into(),
            },
            match self.uplink_bytes {
                Some(b) => format!("{b} out on the physical adapter"),
                None => "(no counter)".into(),
            },
            self.connections,
            if self.ok() {
                "available checks passed; see unmeasured checks"
            } else {
                "FAILED"
            }
        );
        for n in &self.notes {
            out.push_str(&format!("\nnote          : {n}"));
        }
        for p in &self.problems {
            out.push_str(&format!("\nproblem       : {p}"));
        }
        out
    }
}

/// Verify that ordinary system traffic really uses the tunnel.
///
/// The hard checks (all of them have to pass):
///   1. the system routes really point into the TUN adapter,
///   2. the host that is used for the measurement is routed through the TUN,
///   3. the TUN adapter's byte counters grow while the ordinary request runs,
///   4. the exit address of an ordinary request and the exit address of a
///      request through the local proxy are the same (only available when the
///      local proxy runs, i.e. in the system proxy mode).
///
/// Everything else (the exit address before connecting, the relation between
/// the exit address and the server's address) is reported as a note, because it
/// depends on the environment: the machine may already have been behind the
/// same address through another VPN.
pub async fn verify_full_tunnel(
    proxy_port: Option<u16>,
    server_addrs: &[String],
    baseline: Option<String>,
    active_conns: Option<u64>,
    timeout: Duration,
) -> TunnelVerdict {
    let mut v = TunnelVerdict {
        baseline: baseline.clone(),
        ..Default::default()
    };

    // 1. The system routing table has to point into the TUN, otherwise no
    //    ordinary traffic can reach the tunnel at all.
    if let Some((v4, v6)) = crate::tun::installed_routes().await {
        v.routes = (v4, v6);
        if v4 < 2 {
            v.problems.push(format!(
                "the system's IPv4 routes do not point into the TUN ({v4} of 2 split routes found)"
            ));
        }
        // IPv6 is deliberately blocked by this tunnel. Some Windows installs
        // have IPv6 disabled or no IPv6 uplink, so the IPv6 split routes may
        // be absent without affecting the working IPv4 tunnel. Treat that as
        // a real failure only when IPv6 connectivity is available and could
        // otherwise bypass the tunnel.
        if v6 < 2 {
            match crate::netcheck::public_ip_v6(Duration::from_secs(8)).await {
                Ok(_) => v.problems.push(format!(
                    "IPv6 is reachable but only {v6} of 2 IPv6 block routes point into the TUN"
                )),
                Err(_) => v.notes.push(format!(
                    "IPv6 is unavailable on this network; {v6} IPv6 block routes were detected"
                )),
            }
        }
    } else {
        v.problems
            .push("routing table unavailable; cannot verify full tunnel".into());
    }

    // The host that is used for the measurement has to be routed through the
    // TUN as well - otherwise the measurement would not prove anything.
    match crate::netcheck::system_dns_lookup(MEASUREMENT_HOST).await {
        Ok(addrs) => {
            let ipv4: Vec<String> = addrs
                .iter()
                .filter(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
                .cloned()
                .collect();
            v.measurement_addrs = addrs.clone();
            let mut into_tun = None;
            for a in &ipv4 {
                if let Some(v) = crate::tun::route_leads_into_tun(a).await {
                    into_tun = Some(v);
                    if v {
                        break;
                    }
                }
            }
            // Route attribution is an extra proof, not the route setup
            // itself.  Windows can return no usable IPv4 address here (DNS
            // can be transiently unavailable, or the resolver may return
            // only IPv6).  Do not reject an otherwise working /1 tunnel on
            // that auxiliary measurement.
            if into_tun == Some(false) {
                v.problems.push(format!(
                    "the host {MEASUREMENT_HOST} ({}) is not routed through the TUN - \
                     the measurement would not prove anything",
                    ipv4.join(", ")
                ));
            } else if into_tun.is_none() {
                v.notes.push(format!(
                    "the route to {MEASUREMENT_HOST} could not be attributed to the TUN"
                ));
            }
        }
        Err(e) => v.notes.push(format!(
            "the address of {MEASUREMENT_HOST} could not be resolved ({e:#})"
        )),
    }

    let tun_before = crate::tun::adapter_traffic(&crate::tun::active_device());
    let uplink_before = crate::tun::uplink_traffic().await;

    // 1. the exit of an ordinary request (no proxy anywhere).
    match crate::netcheck::public_ip(timeout).await {
        Ok(ip) => v.ordinary_exit = Some(ip),
        Err(e) => {
            v.problems.push(format!(
                "the ordinary system request failed ({e:#}) - the system-wide \
                 traffic could not be verified"
            ));
            return v;
        }
    }

    let tun_after = crate::tun::adapter_traffic(&crate::tun::active_device());
    let uplink_after = crate::tun::uplink_traffic().await;

    v.tunnel_bytes = match (tun_before, tun_after) {
        (Some((r0, t0)), Some((r1, t1))) => Some((r1.saturating_sub(r0), t1.saturating_sub(t0))),
        _ => None,
    };
    v.uplink_bytes = match (uplink_before, uplink_after) {
        (Some((_, t0)), Some((_, t1))) => Some(t1.saturating_sub(t0)),
        _ => None,
    };
    v.connections = active_conns.unwrap_or(0) as u32;

    // 3. The exit of a request that goes through the local proxy. The local
    //    proxy only runs in the system proxy mode - in TUN mode there is no
    //    second path to compare with.
    match proxy_port {
        Some(port) => {
            v.proxied_exit = crate::netcheck::public_ip_via_proxy(port, timeout).await.ok();
        }
        None => v.notes.push(
            "the local proxy does not run in TUN mode - the two exit addresses could not be compared"
                .to_string(),
        ),
    }

    let exit = v.ordinary_exit.clone().unwrap_or_default();

    // The decisive test: the ordinary exit and the explicitly proxied exit have
    // to be the same, otherwise the system traffic bypasses the tunnel.
    if proxy_port.is_some() {
        match &v.proxied_exit {
            Some(p) if *p == exit => {}
            Some(p) => v.problems.push(format!(
                "the ordinary request exits via {exit} but the proxied one via {p} - \
                 the system traffic is NOT using the tunnel"
            )),
            None => v.problems.push(
                "the request through the local proxy failed - the system traffic could not be compared"
                    .to_string(),
            ),
        }
    }

    // The tunnel has to have carried the bytes.
    if !v.tunnel_bytes.map(|(r, t)| r > 0 || t > 0).unwrap_or(false) {
        v.problems
            .push("the TUN adapter's counters did not move".to_string());
    }

    // The exit address before connecting is only an observation: the machine
    // may already have been behind the same exit address (another VPN or the
    // same provider), in which case the tunnel works but the address stays.
    if let Some(b) = baseline.as_deref() {
        if b == exit {
            v.notes.push(format!(
                "the exit address is {exit}, the same as before connecting - the machine \
                 was already behind this address (another VPN?), the tunnel itself is \
                 confirmed by the other checks"
            ));
        } else {
            v.notes
                .push(format!("the exit address changed from {b} to {exit}"));
        }
    }
    if let (Some(p), Some(b)) = (v.proxied_exit.as_deref(), baseline.as_deref()) {
        if p != b {
            v.notes.push(format!(
                "the request through the local proxy leaves the machine via {p} \
                 (it was {b} before the tunnel was started)"
            ));
        }
    }

    // The exit address of a tunnel is the address of the server it runs on -
    // that is what proves that the traffic left the machine through the
    // server. If it does not match, the server is probably behind a NAT or a
    // CDN, which is not an error.
    if !server_addrs.is_empty() {
        if server_addrs.iter().any(|a| a == &exit) {
            v.notes.push(format!(
                "the exit address {exit} is the address of the server - the traffic \
                 left the machine through the tunnel"
            ));
        } else {
            v.notes.push(format!(
                "the exit address {exit} is not the address of the server ({}), the \
                 server is probably behind a NAT or a CDN",
                server_addrs.join(", ")
            ));
        }
    }

    v
}

/// The public IP the machine uses *without* the tunnel (the "main quota"
/// address) and the one it uses with it.
pub struct ExitAddresses {
    pub physical: Option<String>,
    pub tunnel: Option<String>,
}

/// Compare the physical and the tunnelled exit address.
pub async fn exit_addresses(tunnel: Option<String>) -> ExitAddresses {
    let physical = crate::netcheck::public_ip(Duration::from_secs(15))
        .await
        .ok();
    ExitAddresses { physical, tunnel }
}

/// Apply a compatibility preset to a profile.
///
/// The presets only set the values that the other clients are known to use
/// for the same kind of profile; they never invent a server, an SNI or a
/// credential.
pub fn apply_preset(_profile: &mut ServerProfile, _preset: &str) -> bool {
    // No client defaults have been established from effective exports.
    false
}
pub const PRESETS: &[(&str, &str)] = &[];

/// Convert a profile into a share link (redacted version for the report).
pub fn profile_summary(p: &ServerProfile) -> String {
    let link = vmess::to_vmess_link(p);
    let s = WireSettings::from_profile(p);
    format!(
        "profile     : {}\n\
         address     : {}:{}\n\
         protocol    : {}\n\
         transport   : {}\n\
         SNI         : {}\n\
         ALPN        : {}\n\
         fingerprint : {}\n\
         WS host     : {}\n\
         WS path     : {}\n\
         MUX         : {} ({})\n\
         share link  : {}",
        if p.name.is_empty() {
            "(unnamed)"
        } else {
            &p.name
        },
        s.address,
        s.port,
        s.protocol,
        s.transport,
        if s.sni.is_empty() { "(none)" } else { &s.sni },
        if s.alpn.is_empty() {
            "(none)".to_string()
        } else {
            s.alpn.join(",")
        },
        if s.fingerprint.is_empty() {
            "(none)"
        } else {
            &s.fingerprint
        },
        if s.ws_host.is_empty() {
            "(none)"
        } else {
            &s.ws_host
        },
        if s.ws_path.is_empty() {
            "(none)"
        } else {
            &s.ws_path
        },
        yes_no(s.mux_enabled),
        s.mux_concurrency,
        redact_link(&link),
    )
}

/// Redact the credential inside a share link.
///
/// A `vmess://` link is base64 encoded JSON, so the credential has to be
/// removed *inside* the payload - otherwise the base64 string still contains
/// it in plain text.
fn redact_link(link: &str) -> String {
    let Some((scheme, rest)) = link.split_once("://") else {
        return link.to_string();
    };
    if scheme == "vmess" {
        let payload = rest.split('#').next().unwrap_or(rest);
        if let Some(raw) = base64_decode(payload) {
            if let Ok(mut v) = serde_json::from_slice::<Value>(&raw) {
                if let Some(user) = v.get_mut("users").and_then(|u| u.as_array_mut()) {
                    for u in user.iter_mut() {
                        if let Some(id) = u.get_mut("id") {
                            *id = Value::String(redact(id.as_str().unwrap_or_default()));
                        }
                    }
                }
                if let Some(id) = v.get_mut("id") {
                    *id = Value::String(redact(id.as_str().unwrap_or_default()));
                }
                let text = serde_json::to_string(&v).unwrap_or_default();
                return format!("{scheme}://{}", base64_encode(text.as_bytes()));
            }
        }
        return format!("{scheme}://(redacted)");
    }
    match rest.split_once('@') {
        Some((user, host)) => format!("{scheme}://{}@{}", redact(user), host),
        None => link.to_string(),
    }
}

/// Decode base64 (standard and URL-safe, with or without padding).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let s = s.trim();
    [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ]
    .iter()
    .find_map(|e| e.decode(s).ok())
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ServerProfile {
        ServerProfile {
            name: "x".into(),
            address: "1.2.3.4".into(),
            port: 443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            sni: "ea.com".into(),
            ws_host: "ea.com".into(),
            ws_path: "/ray".into(),
            fingerprint: "chrome".into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_uuid_is_never_printed() {
        let cfg = serde_json::json!({
            "outbounds": [{
                "protocol": "vmess",
                "settings": { "vnext": [{ "address": "1.2.3.4", "port": 443,
                    "users": [{ "id": "b831381d-6324-4d53-ad4f-8cda48b30811" }] }] }
            }]
        });
        let text = redacted_config(&cfg).to_string();
        assert!(!text.contains("b831381d-6324-4d53-ad4f-8cda48b30811"));
        assert!(text.contains("[redacted]"));
    }

    #[test]
    fn compares_an_xray_export() {
        let imported = r#"{
            "outbounds": [{
                "protocol": "vmess",
                "settings": { "vnext": [{ "address": "1.2.3.4", "port": 443,
                    "users": [{ "id": "x" }] }] },
                "streamSettings": {
                    "network": "ws", "security": "tls",
                    "tlsSettings": { "serverName": "ea.com", "fingerprint": "chrome",
                                     "alpn": ["h2", "http/1.1"] },
                    "wsSettings": { "path": "/ray", "headers": { "Host": "ea.com" } } },
                "mux": { "enabled": true, "concurrency": 8 }
            }]
        }"#;
        let diffs = compare(&sample(), imported).unwrap();
        // The ALPN and the MUX differ - those are the settings that change
        // what the network sees.
        let names: Vec<&str> = diffs.iter().map(|d| d.setting).collect();
        assert!(names.contains(&"ALPN"), "{names:?}");
        assert!(names.contains(&"MUX"), "{names:?}");
        for d in &diffs {
            assert!(!d.ours.contains("b831381d"), "a credential leaked: {d:?}");
        }
    }

    #[test]
    fn compares_a_singbox_export() {
        let imported = r#"{
            "outbounds": [{
                "type": "vmess", "server": "1.2.3.4", "server_port": 443,
                "tls": { "enabled": true, "server_name": "ea.com",
                         "alpn": ["h2", "http/1.1"],
                         "utls": { "enabled": true, "fingerprint": "chrome" } },
                "transport": { "type": "ws", "path": "/ray",
                               "headers": { "Host": "ea.com" } },
                "multiplex": { "enabled": true, "max_connections": 8 }
            }]
        }"#;
        let diffs = compare(&sample(), imported).unwrap();
        let names: Vec<&str> = diffs.iter().map(|d| d.setting).collect();
        assert!(names.contains(&"ALPN"), "{names:?}");
        assert!(names.contains(&"MUX"), "{names:?}");
    }

    #[test]
    fn the_share_link_never_contains_the_uuid() {
        let p = sample();
        let link = vmess::to_vmess_link(&p);
        let redacted = redact_link(&link);
        assert!(!redacted.contains(&p.uuid), "{redacted}");
        // The base64 of the original link must not appear either.
        assert!(!redacted.contains(&base64_encode(p.uuid.as_bytes())));
        // ... and the rest of the link is still readable.
        assert!(redacted.starts_with("vmess://"));
    }

    #[test]
    fn unsupported_presets_do_not_change_connection_settings() {
        let mut p = sample();
        let before = p.clone();
        assert!(!apply_preset(&mut p, "nekoray"));
        assert!(!apply_preset(&mut p, "netmod"));
        assert_eq!(p, before);
    }
}
