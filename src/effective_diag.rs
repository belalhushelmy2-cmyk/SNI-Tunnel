//! Evidence-labelled diagnostics. Never changes profiles, certificates or routes.
use crate::{model::AppConfig, util};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

/// Prefer the exact last-written runtime document. Its existence does not prove
/// that a core is alive or that it belongs to the currently selected profile.
pub fn configuration(cfg: &AppConfig) -> Result<(Value, String)> {
    let path = util::runtime_config_path();
    if path.exists() {
        let mut v: Value = serde_json::from_slice(&std::fs::read(&path)?)
            .context("invalid runtime JSON; refusing to substitute a preview")?;
        if v["outbounds"][0]["tag"] == "netmod-local" {
            let port = v["outbounds"][0]["settings"]["servers"][0]["port"].clone();
            // Only attach the local backend whose listener matches this exact
            // bridge. Files from previous sessions must not be substituted.
            let mut matching_backends = Vec::new();
            for entry in std::fs::read_dir(util::app_dir())?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("netmod-session-") || !name.ends_with(".json") {
                    continue;
                }
                let Ok(bytes) = std::fs::read(entry.path()) else {
                    continue;
                };
                let Ok(backend) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if backend["inbounds"][0]["port"] == port {
                    matching_backends.push(backend);
                }
            }
            if matching_backends.len() == 1 {
                v["compatibilityBackend"] = matching_backends.remove(0);
            }
            if v.get("compatibilityBackend").is_none() {
                v["compatibilityBackendStatus"] = json!(
                    "Session backend unavailable or ambiguous; cannot compare the remote transport"
                );
            }
        }
        return Ok((
            v,
            "Last-written runtime configuration (may be stale; not proof of an active session)"
                .into(),
        ));
    }
    let server = cfg
        .selected_server_profile()
        .context("no selected server")?;
    let ep = crate::model::resolve_endpoint(server, cfg.selected_sni());
    let v = if cfg.settings.netmod_compatibility && cfg.settings.mode == "tun" {
        let mut v =
            crate::compat::config(&ep, 10800, "", &crate::xray::core_log_level(&cfg.settings))?;
        v["sniTunnelFrontend"] = json!({"type":"netmod-tap", "device":"resolved at startup", "dns":["8.8.8.8","8.8.4.4"]});
        v
    } else if cfg.settings.mode == "tun" {
        crate::tun::tun_config(
            crate::tun::DEVICE_NAME,
            crate::tun::TUN_MTU,
            crate::xray::build_outbound(&ep),
            &crate::xray::core_log_level(&cfg.settings),
        )
    } else {
        crate::xray::build_config(&ep, &cfg.settings, 10800)
    };
    Ok((
        v,
        "Preview only: no runtime file; private port/interface/capabilities are unresolved".into(),
    ))
}

/// Conservative structured export: hide credentials, every arbitrary header,
/// URL query/userinfo and free-form labels. No raw core logs are exported.
pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let key = k.to_ascii_lowercase();
                    let sensitive = [
                        "id",
                        "uuid",
                        "password",
                        "pass",
                        "token",
                        "secret",
                        "authorization",
                        "cookie",
                        "key",
                        "email",
                        "name",
                        "remark",
                        "ps",
                        "user",
                        "username",
                    ]
                    .iter()
                    .any(|s| key == *s)
                        || ["token", "secret", "password", "privatekey", "apikey"]
                            .iter()
                            .any(|s| key.ends_with(s));
                    let val = if sensitive {
                        json!("[redacted]")
                    } else if key == "headers" {
                        // Preserve header names for comparison, never arbitrary values.
                        match v.as_object() {
                            Some(h) => Value::Object(
                                h.keys().map(|k| (k.clone(), json!("[redacted]"))).collect(),
                            ),
                            None => json!("[redacted]"),
                        }
                    } else {
                        redact(v)
                    };
                    (k.clone(), val)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        Value::String(s) => {
            let s = if s.contains("://") && s.contains('@') {
                "[redacted URL userinfo]"
            } else {
                s.split(['?', '#']).next().unwrap_or(s)
            };
            json!(util::scrub_sensitive(s))
        }
        _ => v.clone(),
    }
}

fn outbound(v: &Value) -> Result<&Value> {
    let outbounds = v["outbounds"]
        .as_array()
        .context("expected effective Xray JSON with outbounds")?;
    let candidates: Vec<_> = outbounds
        .iter()
        .filter(|o| matches!(o["protocol"].as_str(), Some("vmess" | "vless" | "trojan")))
        .collect();
    if candidates.len() != 1 {
        bail!("expected exactly one VMess/VLESS/Trojan outbound; export the selected connection with an unambiguous outbound (found {})", candidates.len());
    }
    Ok(candidates[0])
}

fn flatten(prefix: &str, v: &Value, rows: &mut BTreeMap<String, Value>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, v) in m {
                flatten(&format!("{prefix}/{k}"), v, rows);
            }
        }
        Value::Array(a) if !a.is_empty() => {
            for (i, v) in a.iter().enumerate() {
                flatten(&format!("{prefix}/{i}"), v, rows);
            }
        }
        _ => {
            rows.insert(prefix.into(), v.clone());
        }
    }
}

fn fields(v: &Value) -> Result<BTreeMap<String, Value>> {
    let mut ob = outbound(v)?.clone();
    // Normalize the old WS Host location without erasing conflicts.
    if let Some(ws) = ob
        .pointer_mut("/streamSettings/wsSettings")
        .and_then(Value::as_object_mut)
    {
        if let Some(h) = ws.get_mut("headers").and_then(Value::as_object_mut) {
            let host = h.keys().find(|k| k.eq_ignore_ascii_case("host")).cloned();
            if let Some(k) = host {
                let host = h.remove(&k).unwrap();
                if !ws.contains_key("host") {
                    ws.insert("host".into(), host);
                } else {
                    ws.insert("legacyHost".into(), host);
                }
            }
        }
        if ws
            .get("headers")
            .and_then(Value::as_object)
            .is_some_and(|h| h.is_empty())
        {
            ws.remove("headers");
        }
    }
    let mut rows = BTreeMap::new();
    flatten("outbound", &ob, &mut rows);
    for k in ["dns", "routing", "inbounds", "outbounds", "policy"] {
        // Other outbounds determine bypass behavior; ordering is significant.
        if let Some(x) = v.get(k) {
            flatten(k, x, &mut rows);
        }
    }
    Ok(rows)
}

fn cell(path: &str, value: Option<&Value>) -> String {
    match value {
        None => "unspecified (core default/unknown)".into(),
        Some(v) => {
            let key = path.rsplit('/').next().unwrap_or(path);
            let safe = if path.contains("/headers/") {
                json!("[redacted]")
            } else {
                redact(&json!({key: v}))[key].clone()
            };
            safe.to_string()
                .replace('|', "\\|")
                .replace(['\n', '\r'], " ")
        }
    }
}

/// Compares the actual documents, retaining missing values as unknown defaults.
/// Equality of redacted values is never evidence of credential equality.
pub fn compare_json(ours: &Value, text: &str) -> Result<String> {
    let theirs: Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}'))
        .context("export effective Xray JSON from NetMod/Nekoray; profile links and sing-box exports are not equivalent")?;
    let a = fields(ours.get("compatibilityBackend").unwrap_or(ours))?;
    let b = fields(&theirs)?;
    let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
    let mut out=String::from("Configuration comparison\nMissing fields are unspecified, not assumed equal across core versions. Credentials/header values are concealed; redacted authentication cannot be verified. Array order is retained.\n\n| Setting | SNI Tunnel | Reference | Status |\n|---|---|---|---|\n");
    if ours.get("compatibilityBackend").is_some() {
        out.insert_str(0, "Comparing the NetMod remote backend. The modern TUN frontend is shown separately in the configuration report.\n");
    }
    for k in keys {
        let av = cell(k, a.get(k));
        let bv = cell(k, b.get(k));
        let status = if av.contains("redacted") || bv.contains("redacted") {
            "concealed / unknown"
        } else if a.get(k) == b.get(k) {
            "same explicit value"
        } else {
            "DIFFERENT / unspecified"
        };
        out.push_str(&format!(
            "| {} | {av} | {bv} | {status} |\n",
            k.replace('|', "\\|")
        ));
    }
    Ok(out)
}

async fn command(program: &std::path::Path, args: &[&str]) -> String {
    let mut c = tokio::process::Command::new(program);
    c.args(args).kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    match tokio::time::timeout(Duration::from_secs(8), c.output()).await {
        Ok(Ok(o)) if o.status.success() => {
            util::scrub_sensitive(&String::from_utf8_lossy(&o.stdout))
        }
        _ => "unavailable".into(),
    }
}

/// Safe, bounded HTTP probes; ordinary requests explicitly disable proxies.
async fn ip_probe(url: &str, proxy: Option<u16>) -> String {
    let mut b = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8));
    if let Some(p) = proxy {
        let Ok(p) = reqwest::Proxy::all(format!("http://127.0.0.1:{p}")) else {
            return "invalid proxy".into();
        };
        b = b.proxy(p);
    }
    let result = async {
        let response = b.build()?.get(url).send().await?.error_for_status()?;
        let text = response.text().await?;
        let ip = if url.contains("cdn-cgi") {
            crate::tun::parse_trace_ip(&text).unwrap_or_default()
        } else {
            text.trim().to_string()
        };
        let parsed = ip
            .parse::<std::net::IpAddr>()
            .context("response was not an IP address")?;
        Ok::<_, anyhow::Error>(parsed.to_string())
    }
    .await;
    result.unwrap_or_else(|_| "failed / unmeasured".into())
}

async fn udp_probe() -> String {
    let probe = async {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect("1.1.1.1:53").await?;
        // A small DNS A query with a per-request transaction ID.
        let id = uuid::Uuid::new_v4();
        let id = &id.as_bytes()[..2];
        let mut q = vec![id[0], id[1], 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x07example\x03com\x00\x00\x01\x00\x01");
        socket.send(&q).await?;
        let mut response = [0u8; 512];
        let n = socket.recv(&mut response).await?;
        anyhow::ensure!(
            n >= 12 && response[..2] == *id && response[2] & 0x80 != 0 && response[3] & 0x0f == 0,
            "invalid DNS response"
        );
        Ok::<_, anyhow::Error>(())
    };
    match tokio::time::timeout(Duration::from_secs(4), probe).await {
        Ok(Ok(())) => "response received from 1.1.1.1:53".into(),
        _ => "failed / inconclusive".into(),
    }
}

pub async fn report(cfg: &AppConfig, document: &Value, source: &str) -> String {
    let core = command(&crate::corebin::xray_path(), &["version"]).await;
    let (ip1, ip2) = tokio::join!(
        ip_probe("https://api.ipify.org", None),
        ip_probe("https://www.cloudflare.com/cdn-cgi/trace", None)
    );
    let selected = cfg
        .selected_server_profile()
        .map(|p| crate::model::resolve_endpoint(p, cfg.selected_sni()));
    let port = selected
        .as_ref()
        .map(|p| {
            if p.http_port == 0 {
                cfg.settings.http_port
            } else {
                p.http_port
            }
        })
        .unwrap_or(cfg.settings.http_port);
    // A TUN-only runtime has no HTTP listener. Do not attribute another process's listener to this app.
    let proxy = if document["inbounds"]
        .as_array()
        .is_some_and(|a| a.iter().any(|i| i["protocol"] == "socks"))
    {
        ip_probe("https://api.ipify.org", Some(port)).await
    } else {
        "not tested: this runtime has no local HTTP proxy".into()
    };
    let routes = if cfg!(windows) {
        command(std::path::Path::new("route.exe"), &["print"]).await
    } else {
        command(
            std::path::Path::new("ip"),
            &["route", "show", "table", "all"],
        )
        .await
    };
    let counters = crate::tun::adapter_traffic(crate::tun::DEVICE_NAME);
    let sockets = if cfg!(windows) {
        command(std::path::Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command", "$p = @(Get-Process xray -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id); if ($p.Count -eq 0) { 'No Xray process running' } else { Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $_.OwningProcess -in $p -and $_.State -eq 'Established' } | Select-Object RemoteAddress,RemotePort,OwningProcess | Format-Table -AutoSize }"]).await
    } else {
        "not observed on this platform".into()
    };
    let udp = udp_probe().await;
    let v6 = match tokio::time::timeout(
        Duration::from_secs(9),
        crate::netcheck::public_ip_v6(Duration::from_secs(8)),
    )
    .await
    {
        Ok(Ok(ip)) => format!("reachable via {ip}; route attribution not measured"),
        _ => "failed/inconclusive; not proof of leak prevention".into(),
    };
    format!("Connection Diagnostics\n{source}\nXray-core: {core}\nSelected dial endpoint: {}\nObserved Xray TCP peers (all Xray processes; PID attribution required): {sockets}\nOrdinary IPv4 / ipify: {ip1}\nOrdinary / Cloudflare: {ip2}\nExplicit HTTP proxy / ipify: {proxy}\nIPv6: {v6}\nTCP: see HTTP probe outcomes above\nOrdinary UDP DNS probe: {udp} (routing attribution not measured)\nDNS leak test: not measured; successful resolution does not prove resolver path\nUnexpected direct connections: not measured; route table alone cannot attribute sockets\nTUN adapter counters: {counters:?} (None means unavailable)\nKill switch: route retention only; not a verified firewall guarantee after adapter/process loss\nLocal proxy configured HTTP/SOCKS ports: {}/{} (system mode only; SOCKS CONNECT, no UDP relay)\nTLS below is configured, not a packet capture. No billing classification can be inferred.\n\nRuntime settings (credentials and arbitrary header values redacted):\n{}\n\nInstalled routes:\n{routes}",
        selected.as_ref().map(|e|format!("{}:{}",e.address,e.port)).unwrap_or_else(||"none".into()),
        port,selected.as_ref().map(|p|if p.socks_port==0 {cfg.settings.socks_port}else{p.socks_port}).unwrap_or(cfg.settings.socks_port),
        serde_json::to_string_pretty(&redact(document)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_bypass_dns_tls_and_auth_without_exposing_secrets() {
        let a = json!({"outbounds":[{"protocol":"freedom"},{"protocol":"vmess","settings":{"vnext":[{"users":[{"id":"private-uuid","security":"auto"}]}]},"streamSettings":{"tlsSettings":{"serverName":"ea.com","minVersion":"1.2"},"wsSettings":{"headers":{"Authorization":"Bearer secret"},"path":"/ws?token=private"}}}],"dns":{"servers":["1.1.1.1"]}});
        let mut b = a.clone();
        b["dns"]["servers"][0] = json!("8.8.8.8");
        let r = compare_json(&a, &b.to_string()).unwrap();
        assert!(r.contains("dns/servers/0"));
        assert!(r.contains("minVersion"));
        assert!(!r.contains("private-uuid"));
        assert!(!r.contains("Bearer secret"));
        assert!(!r.contains("token=private"));
        assert!(r.contains("ea.com"));
    }
    #[test]
    fn refuses_ambiguous_or_wrong_exports() {
        let a = json!({"outbounds":[{"protocol":"vmess"}]});
        assert!(compare_json(
            &a,
            r#"{"outbounds":[{"protocol":"vmess"},{"protocol":"vmess"}]}"#
        )
        .is_err());
        assert!(compare_json(&a, "{}").is_err());
    }
    #[test]
    fn redacts_nested_secrets() {
        let r=redact(&json!({"TOKEN":"hidden1","headers":{"X-Custom":"hidden2"},"path":"/x?auth=hidden3","uuid":"hidden4","serverName":"ea.com"})).to_string();
        for secret in ["hidden1", "hidden2", "hidden3", "hidden4"] {
            assert!(!r.contains(secret));
        }
        assert!(r.contains("ea.com"));
    }
}

#[cfg(test)]
mod compatibility_report_tests {
    use super::*;
    #[test]
    fn compares_remote_backend_instead_of_loopback_socks_frontend() {
        let backend = json!({"outbounds":[{"protocol":"vmess","streamSettings":{"tlsSettings":{"serverName":"ea.com"}}}]});
        let frontend = json!({"outbounds":[crate::compat::bridge_outbound(12345)],"compatibilityBackend":backend.clone()});
        let report = compare_json(&frontend, &backend.to_string()).unwrap();
        assert!(report.contains("Comparing the NetMod remote backend"));
        assert!(report.contains("ea.com"));
        assert!(!report.contains("DIFFERENT"));
    }
}
