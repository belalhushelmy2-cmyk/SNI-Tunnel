//! Import helpers: `vmess://` links, subscription blobs, clipboard text and
//! JSON files. Nothing here talks to the network.

use crate::model::{normalize_transport, ServerProfile};
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};

type B64Decoder = dyn Fn(&str) -> Option<Vec<u8>>;

fn b64_decode_variants(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    let engines: [&B64Decoder; 4] = [
        &|x| base64::engine::general_purpose::STANDARD.decode(x).ok(),
        &|x| {
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(x)
                .ok()
        },
        &|x| base64::engine::general_purpose::URL_SAFE.decode(x).ok(),
        &|x| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(x)
                .ok()
        },
    ];
    for e in engines {
        if let Some(v) = e(s) {
            return Some(v);
        }
    }
    None
}

fn get_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn profile_from_vmess_json(v: &Value) -> Result<ServerProfile> {
    let address = get_str(v, "add");
    let uuid = get_str(v, "id");
    if address.is_empty() {
        bail!("vmess entry has no server address (add)");
    }
    if uuid.is_empty() {
        bail!("vmess entry has no UUID (id)");
    }
    let raw_port = get_str(v, "port");
    let port: u16 = if raw_port.trim().is_empty() {
        443
    } else {
        raw_port
            .trim()
            .parse()
            .context("VMess port must be between 1 and 65535")?
    };
    anyhow::ensure!(port > 0, "VMess port must be between 1 and 65535");
    let net = normalize_transport(&get_str(v, "net"));
    let tls_field = get_str(v, "tls").to_ascii_lowercase();
    let tls = tls_field == "tls" || tls_field == "true" || tls_field == "reality";
    let path = get_str(v, "path");
    let host = get_str(v, "host");
    let sni = get_str(v, "sni");
    let aid: u32 = get_str(v, "aid").trim().parse().unwrap_or(0);
    let alpn_text = match v.get("alpn").and_then(Value::as_array) {
        Some(a) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(","),
        None => get_str(v, "alpn"),
    };
    let alpn = alpn_text
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut p = ServerProfile {
        name: {
            let ps = get_str(v, "ps");
            if ps.is_empty() {
                format!("{address}:{port}")
            } else {
                ps
            }
        },
        address,
        port,
        uuid,
        security: {
            let s = get_str(v, "scy");
            if s.is_empty() {
                "auto".into()
            } else {
                s
            }
        },
        network: net.clone(),
        tls,
        sni,
        alpn,
        allow_insecure: get_str(v, "allowInsecure") == "true",
        fingerprint: get_str(v, "fp"),
        flow: get_str(v, "flow"),
        mux_enabled: v
            .get("mux_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mux_concurrency: get_str(v, "mux_concurrency").parse().unwrap_or(8),
        mux_xudp_enabled: v
            .get("mux_xudp_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mux_xudp_concurrency: get_str(v, "mux_xudp_concurrency").parse().unwrap_or(16),
        mux_xudp_udp443: {
            let policy = get_str(v, "mux_xudp_udp443");
            if policy.is_empty() {
                "reject".into()
            } else {
                policy
            }
        },
        alter_id: aid,
        tcp_header: {
            let t = get_str(v, "type");
            if t.is_empty() {
                "none".into()
            } else {
                t
            }
        },
        xhttp_mode: get_str(v, "mode"),
        dns: get_str(v, "dns"),
        pinned_cert_sha256: get_str(v, "pinned_cert_sha256"),
        remark: get_str(v, "remark"),
        ws_headers: v
            .get("ws_headers")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default(),
        timeout_secs: get_str(v, "timeout_secs").parse().unwrap_or_default(),
        http_port: get_str(v, "http_port").parse().unwrap_or_default(),
        socks_port: get_str(v, "socks_port").parse().unwrap_or_default(),
        grpc_multi: get_str(v, "grpc_multi") == "true",
        kcp_seed: get_str(v, "kcp_seed"),
        kcp_header: get_str(v, "kcp_header"),
        quic_security: get_str(v, "quic_security"),
        quic_header: get_str(v, "quic_header"),
        ..Default::default()
    };

    match net.as_str() {
        "tcp" | "ws" | "httpupgrade" | "xhttp" => {
            p.ws_host = host;
            p.ws_path = if path.is_empty() { "/".into() } else { path };
        }
        "h2" => {
            p.h2_host = host;
            p.h2_path = if path.is_empty() { "/".into() } else { path };
        }
        "grpc" => {
            p.grpc_service = if path.is_empty() {
                get_str(v, "serviceName")
            } else {
                path
            };
        }
        _ => {}
    }
    Ok(p)
}

/// Parse a single `vmess://` link. Two shapes are supported:
///  * the classic v2rayN form — `vmess://base64(json)`
///  * the newer v2rayN form — `vmess://base64(auto:uuid@host:port)?params#name`
pub fn parse_vmess_link(link: &str) -> Result<ServerProfile> {
    let link = link.trim();
    let (scheme, payload) = match link.split_once("://") {
        Some((s, p)) => (s.to_ascii_lowercase(), p),
        None => return Err(anyhow!("not a vmess:// link")),
    };
    match scheme.as_str() {
        "vmess" => parse_vmess_payload(payload),
        "vless" | "trojan" => {
            let (body, query, remark) = split_link(payload);
            let mut p = profile_from_userinfo(&body, &query, &remark)?;
            p.protocol = scheme;
            Ok(p)
        }
        other => Err(anyhow!("unsupported link scheme: {other}")),
    }
}

fn parse_vmess_payload(payload: &str) -> Result<ServerProfile> {
    // Split off the fragment (remarks) and the query string.
    let (body, query, remark) = split_link(payload);
    let body = body.split('?').next().unwrap_or(&body).to_string();

    if let Ok(v) = serde_json::from_str::<Value>(&String::from_utf8_lossy(
        &b64_decode_variants(&body).unwrap_or_default(),
    )) {
        if v.is_object() {
            let mut p = profile_from_vmess_json(&v)?;
            if !remark.is_empty() {
                p.remark = remark.clone();
            }
            return Ok(p);
        }
    }

    // Newer form: base64("auto:uuid@host:port") + query parameters.
    if let Some(raw) = b64_decode_variants(&body) {
        if let Ok(text) = String::from_utf8(raw) {
            if text.contains('@') {
                return profile_from_userinfo(&text, &query, &remark);
            }
        }
    }
    // Plain form: "uuid@host:port" directly.
    if body.contains('@') {
        return profile_from_userinfo(&body, &query, &remark);
    }

    bail!("vmess payload is not valid base64 or uuid@host form")
}

/// Split `payload#remarks?query` into (body, query, remarks).
fn split_link(payload: &str) -> (String, String, String) {
    let (before_hash, remark) = match payload.split_once('#') {
        Some((b, r)) => (b.to_string(), r.to_string()),
        None => (payload.to_string(), String::new()),
    };
    let (body, query) = match before_hash.split_once('?') {
        Some((b, q)) => (b.to_string(), q.to_string()),
        None => (before_hash, String::new()),
    };
    (body, query, remark)
}

/// Parse a `user:uuid@host:port` style VMess link.
fn profile_from_userinfo(text: &str, query: &str, remark: &str) -> Result<ServerProfile> {
    let (userinfo, hostport) = text
        .rsplit_once('@')
        .ok_or_else(|| anyhow!("vmess link has no '@'"))?;
    let uuid = match userinfo.split_once(':') {
        Some((_, u)) => u.to_string(),
        None => userinfo.to_string(),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(443)),
        None => (hostport.to_string(), 443),
    };

    let mut p = ServerProfile {
        address: host,
        port,
        uuid,
        name: if remark.is_empty() {
            format!("{}:{}", hostport, port)
        } else {
            remark.to_string()
        },
        ..Default::default()
    };

    for (k, v) in parse_query(query) {
        match k.as_str() {
            "encryption" | "scy" => p.security = v,
            "security" => {
                p.tls = v.eq_ignore_ascii_case("tls") || v.eq_ignore_ascii_case("reality")
            }
            "type" => p.network = normalize_transport(&v),
            "sni" | "peer" => p.sni = v,
            "host" => p.ws_host = v,
            "path" => p.ws_path = v,
            "alpn" => {
                p.alpn = v
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "fp" => p.fingerprint = v,
            "allowInsecure" | "allow_insecure" => p.allow_insecure = v == "true",
            "flow" => p.flow = v,
            "headerType" => p.tcp_header = v,
            "mode" => p.xhttp_mode = v,
            "serviceName" | "servicename" => p.grpc_service = v,
            "remarks" | "remark" if !remark.is_empty() => {}
            _ => {}
        }
    }
    if p.ws_path.trim().is_empty() {
        p.ws_path = "/".into();
    }
    if p.network.as_str() == "grpc" {
        p.grpc_service = p.grpc_service.clone();
    }
    Ok(p)
}

/// Parse `a=b&c=d` into pairs (percent-decoded).
fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((percent_decode(k), percent_decode(v)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Export a profile as a shareable link.
///
/// VMess uses the classic v2rayN `vmess://base64(json)` form, VLESS and
/// Trojan the `scheme://id@host:port?params#name` form. The UUID (or the
/// Trojan password) and every transport parameter are preserved.
pub fn to_vmess_link(p: &ServerProfile) -> String {
    if p.protocol == "vless" || p.protocol == "trojan" {
        return to_userinfo_link(p);
    }
    let (host, path) = match p.network.as_str() {
        "h2" => (p.h2_host.clone(), p.h2_path.clone()),
        "grpc" => (String::new(), p.grpc_service.clone()),
        _ => (p.ws_host.clone(), p.ws_path.clone()),
    };
    let v = json!({
        "v": "2",
        "ps": p.name,
        "add": p.address,
        "port": p.port.to_string(),
        "id": p.uuid,
        "aid": p.alter_id.to_string(),
        "scy": p.security,
        "net": p.network,
        "type": if p.tcp_header.is_empty() { "none".to_string() } else { p.tcp_header.clone() },
        "host": host,
        "path": path,
        "tls": if p.tls { "tls" } else { "" },
        "sni": p.sni,
        "alpn": p.alpn.join(","),
        "fp": p.fingerprint,
        "flow": p.flow,
        "dns": p.dns, "ws_headers": p.ws_headers, "remark":p.remark,
        "pinned_cert_sha256":p.pinned_cert_sha256, "timeout_secs":p.timeout_secs,
        "http_port":p.http_port,"socks_port":p.socks_port,"grpc_multi":p.grpc_multi,
        "kcp_seed":p.kcp_seed,"kcp_header":p.kcp_header,"quic_security":p.quic_security,"quic_header":p.quic_header,
        "mux_enabled": p.mux_enabled,
        "mux_concurrency": p.mux_concurrency,
        "mux_xudp_enabled": p.mux_xudp_enabled,
        "mux_xudp_concurrency": p.mux_xudp_concurrency,
        "mux_xudp_udp443": p.mux_xudp_udp443,
        "allowInsecure": if p.allow_insecure { "true" } else { "false" }
    });
    format!(
        "vmess://{}",
        base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_string(&v).unwrap_or_default())
    )
}

/// Export a VLESS/Trojan profile as `scheme://id@host:port?params#name`.
pub fn to_userinfo_link(p: &ServerProfile) -> String {
    let mut params: Vec<String> = Vec::new();
    params.push(format!("security={}", if p.tls { "tls" } else { "none" }));
    params.push(format!("type={}", p.network));
    if !p.sni.trim().is_empty() {
        params.push(format!("sni={}", p.sni.trim()));
    }
    if !p.ws_host.trim().is_empty() {
        params.push(format!("host={}", p.ws_host.trim()));
    }
    if !p.ws_path.trim().is_empty() {
        params.push(format!("path={}", p.ws_path.trim()));
    }
    if !p.alpn.is_empty() {
        params.push(format!("alpn={}", p.alpn.join(",")));
    }
    if !p.fingerprint.trim().is_empty() {
        params.push(format!("fp={}", p.fingerprint.trim()));
    }
    if !p.flow.trim().is_empty() {
        params.push(format!("flow={}", p.flow.trim()));
    }
    if p.allow_insecure {
        params.push("allowInsecure=1".into());
    }
    if p.protocol == "vless" {
        params.push("encryption=none".into());
    }
    let remark = if p.name.trim().is_empty() {
        String::new()
    } else {
        format!("#{}", p.name.trim())
    };
    format!(
        "{}://{}@{}:{}?{}{}",
        p.protocol,
        p.uuid,
        p.address,
        p.port,
        params.join("&"),
        remark
    )
}

/// Parse a JSON configuration: either a single profile object, an array of
/// profiles, or a list of vmess-style objects.
pub fn parse_json_config(text: &str) -> Result<Vec<ServerProfile>> {
    let v: Value = serde_json::from_str(text).context("invalid JSON")?;
    parse_json_value(&v)
}

fn parse_json_value(v: &Value) -> Result<Vec<ServerProfile>> {
    match v {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                out.extend(parse_json_value(item)?);
            }
            if out.is_empty() {
                bail!("no server entries found in JSON array");
            }
            Ok(out)
        }
        Value::Object(_) => {
            // Our own export format uses `address` + `uuid`.
            if v.get("address").is_some() || v.get("uuid").is_some() {
                let p: ServerProfile = serde_json::from_value(v.clone())
                    .context("JSON is neither a vmess entry nor a SNI Tunnel server profile")?;
                let mut p = p;
                if p.id.is_empty() {
                    p.id = uuid::Uuid::new_v4().to_string();
                }
                if p.name.is_empty() {
                    p.name = format!("{}:{}", p.address, p.port);
                }
                Ok(vec![p])
            } else if v.get("add").is_some() || v.get("id").is_some() {
                Ok(vec![profile_from_vmess_json(v)?])
            } else {
                bail!("unrecognized JSON profile shape")
            }
        }
        _ => bail!("JSON must be an object or an array of objects"),
    }
}

/// Parse arbitrary clipboard/import text: a JSON document, one or more
/// `vmess://` links, or a base64 subscription blob containing links.
pub fn parse_any(text: &str) -> Result<Vec<ServerProfile>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("nothing to import");
    }

    // 1. Plain JSON.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return parse_json_config(trimmed);
    }

    // Parse every link, including multiple links pasted on one line. A bad
    // entry aborts the batch instead of silently reporting partial success.
    let lower = trimmed.to_ascii_lowercase();
    let mut starts: Vec<usize> = ["vmess://", "vless://", "trojan://"]
        .into_iter()
        .flat_map(|scheme| {
            lower
                .match_indices(scheme)
                .map(|(offset, _)| offset)
                .collect::<Vec<_>>()
        })
        .collect();
    starts.sort_unstable();
    if !starts.is_empty() {
        let mut profiles = Vec::new();
        for (number, start) in starts.iter().copied().enumerate() {
            let end = starts.get(number + 1).copied().unwrap_or(trimmed.len());
            let link = trimmed[start..end].lines().next().unwrap_or("").trim();
            profiles.push(parse_vmess_link(link).map_err(|_| anyhow!(
                "Server link {} is invalid; no profiles were imported. Check the complete link and try again.", number + 1
            ))?);
        }
        return Ok(profiles);
    }

    // 3. Base64 subscription blob.
    let compact: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if let Some(raw) = b64_decode_variants(&compact) {
        if let Ok(decoded) = String::from_utf8(raw) {
            let lower = decoded.to_ascii_lowercase();
            if lower.contains("vmess://")
                || lower.contains("vless://")
                || lower.contains("trojan://")
            {
                return parse_any(&decoded);
            }
        }
    }

    bail!("unrecognized import format (expected a vmess:// link or JSON)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn batch_import_preserves_every_link_and_rejects_partial_success() {
        let good = mk_link(
            r#"{"add":"192.0.2.10","id":"b831381d-6324-4d53-ad4f-8cda48b30811","port":443}"#,
        );
        assert_eq!(parse_any(&format!("{good}\n{good}")).unwrap().len(), 2);
        assert_eq!(parse_any(&format!("{good} {good}")).unwrap().len(), 2);
        assert!(parse_any(&format!("{good}\nvmess://broken")).is_err());
        let upper = good.replacen("vmess://", "VMESS://", 1);
        assert_eq!(parse_any(&upper).unwrap().len(), 1);
        let encoded = STANDARD.encode(format!("{good}\n{good}"));
        let wrapped = encoded
            .as_bytes()
            .chunks(32)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(parse_any(&wrapped).unwrap().len(), 2);
    }

    fn mk_link(json: &str) -> String {
        format!("vmess://{}", STANDARD.encode(json))
    }

    #[test]
    fn tcp_headers_and_array_alpn_survive_import() {
        let p=profile_from_vmess_json(&json!({"add":"server.example","id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"tcp","tls":"tls","sni":"ea.com","type":"http","host":"host.example","path":"/route","alpn":["http/1.1"],"fp":"chrome"})).unwrap();
        let ep = crate::model::resolve_endpoint(&p, None);
        let ob = crate::xray::build_outbound(&ep);
        assert_eq!(ob["streamSettings"]["tlsSettings"]["serverName"], "ea.com");
        assert_eq!(
            ob["streamSettings"]["tlsSettings"]["alpn"],
            json!(["http/1.1"])
        );
        assert_eq!(
            ob["streamSettings"]["tcpSettings"]["header"]["request"]["headers"]["Host"],
            json!(["host.example"])
        );
        assert_eq!(ep.address, "server.example");
    }

    #[test]
    fn parses_ws_tls_link() {
        let json = r#"{"v":"2","ps":"Tokyo","add":"1.2.3.4","port":"443",
            "id":"b831381d-6324-4d53-ad4f-8cda48b30811","aid":"0","scy":"auto",
            "net":"ws","type":"none","host":"cdn.example.com","path":"/ws",
            "tls":"tls","sni":"cdn.example.com","alpn":"h2,http/1.1","fp":"chrome"}"#;
        let p = parse_vmess_link(&mk_link(json)).unwrap();
        assert_eq!(p.name, "Tokyo");
        assert_eq!(p.address, "1.2.3.4");
        assert_eq!(p.port, 443);
        assert_eq!(p.network, "ws");
        assert_eq!(p.ws_host, "cdn.example.com");
        assert_eq!(p.ws_path, "/ws");
        assert!(p.tls);
        assert_eq!(p.sni, "cdn.example.com");
        assert_eq!(p.alpn, vec!["h2", "http/1.1"]);
        assert_eq!(p.fingerprint, "chrome");
    }

    #[test]
    fn parses_tcp_no_tls() {
        let json = r#"{"v":"2","ps":"direct","add":"example.com","port":8080,
            "id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"tcp"}"#;
        let p = parse_vmess_link(&mk_link(json)).unwrap();
        assert_eq!(p.port, 8080);
        assert!(!p.tls);
        assert_eq!(p.network, "tcp");
    }

    #[test]
    fn parses_json_array_and_normalizes_http2() {
        let text = r#"[{"add":"a.example","port":"443","id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"http","tls":"tls"}]"#;
        let list = parse_json_config(text).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].network, "h2");
    }

    #[test]
    fn parses_our_own_export_format() {
        let text = r#"{"name":"x","address":"h","port":443,"uuid":"b831381d-6324-4d53-ad4f-8cda48b30811","network":"grpc","grpc_service":"svc"}"#;
        let list = parse_json_config(text).unwrap();
        assert_eq!(list[0].network, "grpc");
        assert_eq!(list[0].grpc_service, "svc");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_any("hello world").is_err());
        assert!(parse_vmess_link("vmess://!!!notbase64!!!").is_err());
    }

    #[test]
    fn parses_multi_line_subscription() {
        let a = mk_link(r#"{"add":"a","port":1,"id":"b831381d-6324-4d53-ad4f-8cda48b30811"}"#);
        let b = mk_link(r#"{"add":"b","port":2,"id":"b831381d-6324-4d53-ad4f-8cda48b30811"}"#);
        let blob = STANDARD.encode(format!("{a}\n{b}"));
        let list = parse_any(&blob).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn export_import_round_trip_keeps_everything() {
        let p = ServerProfile {
            name: "Tokyo".into(),
            address: "1.2.3.4".into(),
            port: 8443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            sni: "ea.com".into(),
            ws_host: "front.example".into(),
            ws_path: "/ray".into(),
            alpn: vec!["h2".into(), "http/1.1".into()],
            fingerprint: "chrome".into(),
            alter_id: 0,
            security: "auto".into(),
            ..Default::default()
        };
        let link = to_vmess_link(&p);
        assert!(link.starts_with("vmess://"));
        let back = parse_vmess_link(&link).unwrap();
        assert_eq!(back.address, p.address);
        assert_eq!(back.port, p.port);
        assert_eq!(back.uuid, p.uuid);
        assert_eq!(back.sni, "ea.com");
        assert_eq!(back.ws_host, "front.example");
        assert_eq!(back.ws_path, "/ray");
        assert_eq!(back.network, "ws");
        assert_eq!(back.alpn, p.alpn);
        assert_eq!(back.fingerprint, "chrome");
        assert!(back.tls);
    }

    #[test]
    fn parses_new_style_userinfo_link() {
        let userinfo = STANDARD.encode("auto:b831381d-6324-4d53-ad4f-8cda48b30811@1.2.3.4:443");
        let link = format!(
            "vmess://{userinfo}?encryption=auto&security=tls&type=ws&host=front.example&path=%2Fws&sni=ea.com#Tokyo"
        );
        let p = parse_vmess_link(&link).unwrap();
        assert_eq!(p.address, "1.2.3.4");
        assert_eq!(p.port, 443);
        assert_eq!(p.uuid, "b831381d-6324-4d53-ad4f-8cda48b30811");
        assert!(p.tls);
        assert_eq!(p.network, "ws");
        assert_eq!(p.sni, "ea.com");
        assert_eq!(p.ws_host, "front.example");
        assert_eq!(p.ws_path, "/ws");
    }

    #[test]
    fn imported_sni_survives_and_stays_independent() {
        // The SNI of an imported server must be detected, and must not leak
        // into the address or the WebSocket Host.
        let json = r#"{"v":"2","ps":"x","add":"10.0.0.9","port":"443",
            "id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"ws",
            "host":"front.example","path":"/ray","tls":"tls","sni":"ea.com"}"#;
        let p = parse_vmess_link(&mk_link(json)).unwrap();
        assert_eq!(p.sni, "ea.com");
        assert_eq!(p.address, "10.0.0.9");
        assert_eq!(p.ws_host, "front.example");
        assert_eq!(p.ws_path, "/ray");
    }

    #[test]
    fn parses_transport_variants() {
        let json = r#"{"add":"h","port":443,"id":"b831381d-6324-4d53-ad4f-8cda48b30811",
            "net":"httpupgrade","host":"h.example","path":"/up"}"#;
        let p = parse_vmess_link(&mk_link(json)).unwrap();
        assert_eq!(p.network, "httpupgrade");
        assert_eq!(p.ws_host, "h.example");
    }

    #[test]
    fn parses_vless_and_trojan_links() {
        let vless = "vless://b831381d-6324-4d53-ad4f-8cda48b30811@1.2.3.4:443?security=tls&type=ws&host=h.example&path=%2Fp&sni=ea.com#VLESS%20node";
        let p = parse_vmess_link(vless).unwrap();
        assert_eq!(p.protocol, "vless");
        assert_eq!(p.address, "1.2.3.4");
        assert_eq!(p.port, 443);
        assert_eq!(p.uuid, "b831381d-6324-4d53-ad4f-8cda48b30811");
        assert_eq!(p.network, "ws");
        assert_eq!(p.sni, "ea.com");
        assert_eq!(p.ws_host, "h.example");
        assert_eq!(p.ws_path, "/p");
        assert!(p.tls);

        let trojan = "trojan://s3cret@5.6.7.8:8443?security=tls&sni=ea.com#Trojan";
        let p = parse_vmess_link(trojan).unwrap();
        assert_eq!(p.protocol, "trojan");
        assert_eq!(p.uuid, "s3cret");
        assert_eq!(p.address, "5.6.7.8");
        assert_eq!(p.port, 8443);
        assert_eq!(p.sni, "ea.com");
    }

    #[test]
    fn vless_and_trojan_round_trip() {
        let base = ServerProfile {
            name: "node".into(),
            address: "1.2.3.4".into(),
            port: 443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            protocol: "vless".into(),
            network: "ws".into(),
            tls: true,
            sni: "ea.com".into(),
            ws_host: "h.example".into(),
            ws_path: "/p".into(),
            ..Default::default()
        };
        let link = to_vmess_link(&base);
        assert!(link.starts_with("vless://"));
        let back = parse_vmess_link(&link).unwrap();
        assert_eq!(back.protocol, "vless");
        assert_eq!(back.address, base.address);
        assert_eq!(back.port, base.port);
        assert_eq!(back.uuid, base.uuid);
        assert_eq!(back.sni, base.sni);
        assert_eq!(back.ws_host, base.ws_host);
        assert_eq!(back.ws_path, base.ws_path);

        let mut trojan = base.clone();
        trojan.protocol = "trojan".into();
        trojan.uuid = "password".into();
        let link = to_vmess_link(&trojan);
        assert!(link.starts_with("trojan://"));
        let back = parse_vmess_link(&link).unwrap();
        assert_eq!(back.protocol, "trojan");
        assert_eq!(back.uuid, "password");
        assert_eq!(back.sni, "ea.com");
    }

    #[test]
    fn parse_any_accepts_a_mixed_subscription() {
        let text = format!(
            "{}\n{}",
            "vless://b831381d-6324-4d53-ad4f-8cda48b30811@a.example:443?type=ws#a",
            "trojan://pw@b.example:443#b"
        );
        let list = parse_any(&text).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].protocol, "vless");
        assert_eq!(list[1].protocol, "trojan");
    }
}
