//! Xray-core integration: config generation and managed subprocess lifecycle.

use crate::logging::Logger;
use crate::model::{Endpoint, Settings};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Build the Xray configuration document for a resolved endpoint.
///
/// `socks_port` is the private loopback inbound that the bundled local proxy
/// dials; it is never exposed to the system proxy settings.
pub fn build_config(ep: &Endpoint, settings: &Settings, socks_port: u16) -> Value {
    build_config_udp(ep, settings, socks_port, false)
}

/// What the installed Xray-core understands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreCaps {
    /// The core knows the `xudpConcurrency` and `xudpProxyUDP443` fields of
    /// the `mux` object (Xray 1.8.6 and newer).
    pub xudp: bool,
}

/// The capabilities of the core that is in use. They are probed once, when the
/// core is found, and then reused for every configuration.
static CORE_CAPS: RwLock<CoreCaps> = RwLock::new(CoreCaps { xudp: false });

/// Has the core been probed already? The core cannot change while the app
/// runs, so one probe per process is enough.
static CAPS_PROBED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The capabilities of the core that is in use.
pub fn core_caps() -> CoreCaps {
    *CORE_CAPS.read().unwrap_or_else(|e| e.into_inner())
}

/// Remember what the core supports.
pub fn set_core_caps(caps: CoreCaps) {
    if let Ok(mut c) = CORE_CAPS.write() {
        *c = caps;
    }
}

/// Probe the core once and remember what it supports.
pub fn ensure_core_caps(exe: &Path, logger: &Logger) -> CoreCaps {
    if CAPS_PROBED.load(std::sync::atomic::Ordering::Relaxed) {
        return core_caps();
    }
    let caps = probe_core_caps(exe, logger);
    set_core_caps(caps);
    CAPS_PROBED.store(true, std::sync::atomic::Ordering::Relaxed);
    logger.info(format!(
        "Xray-core capabilities: XUDP {}",
        if caps.xudp {
            "supported"
        } else {
            "not supported"
        }
    ));
    caps
}

/// The `mux` object of an outbound.
///
/// `caps` says whether the installed core understands the XUDP fields: they
/// are only written when it does, so an older core never sees a field it does
/// not know. With MUX switched off the object stays compatible and simply
/// carries `"enabled": false`.
pub fn mux_block(ep: &Endpoint, caps: CoreCaps) -> Value {
    let mut mux = json!({
        "enabled": ep.mux_enabled,
        "concurrency": ep.mux_concurrency.max(1),
    });
    if ep.mux_enabled && ep.mux_xudp_enabled && caps.xudp {
        mux["xudpConcurrency"] = json!(ep.mux_xudp_concurrency.clamp(1, 1024));
        mux["xudpProxyUDP443"] = json!(normalize_udp443(&ep.mux_xudp_udp443));
    }
    mux
}

/// The value the core accepts for `xudpProxyUDP443`.
pub fn normalize_udp443(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" => "allow",
        "skip" => "skip",
        _ => "reject",
    }
}

/// The `vmess` outbound document for a resolved endpoint. Composed into
/// both the proxy config and the TUN-mode config.
/// Build the outbound document for a resolved endpoint (VMess, VLESS or
/// Trojan). The transport (stream settings) is shared by all of them.
pub fn build_outbound(ep: &Endpoint) -> Value {
    build_outbound_with_caps(ep, core_caps())
}

/// The outbound document with an explicit set of core capabilities.
pub fn build_outbound_with_caps(ep: &Endpoint, caps: CoreCaps) -> Value {
    let stream = stream_settings(ep);
    let mux = mux_block(ep, caps);
    match ep.protocol.as_str() {
        "trojan" => {
            let mut server = json!({
                "address": ep.address,
                "port": ep.port,
                "password": ep.uuid,
            });
            if !ep.flow.trim().is_empty() {
                server["flow"] = json!(ep.flow.trim());
            }
            json!({
                "tag": "proxy",
                "protocol": "trojan",
                "settings": { "servers": [server] },
                "streamSettings": stream,
                "mux": mux
            })
        }
        "vless" => {
            let mut user = json!({
                "id": ep.uuid,
                "encryption": "none",
                "level": 0
            });
            if !ep.flow.trim().is_empty() {
                user["flow"] = json!(ep.flow.trim());
            }
            json!({
                "tag": "proxy",
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": ep.address,
                        "port": ep.port,
                        "users": [user]
                    }]
                },
                "streamSettings": stream,
                "mux": mux
            })
        }
        _ => {
            let mut user = json!({
                "id": ep.uuid,
                "alterId": ep.alter_id,
                "security": ep.security,
                "level": 0
            });
            if !ep.flow.trim().is_empty() {
                user["flow"] = json!(ep.flow.trim());
            }
            json!({
                "tag": "proxy",
                "protocol": "vmess",
                "settings": {
                    "vnext": [{
                        "address": ep.address,
                        "port": ep.port,
                        "users": [user]
                    }]
                },
                "streamSettings": stream,
                "mux": mux
            })
        }
    }
}

/// The transport part of an outbound (network, TLS and the transport options).
fn stream_settings(ep: &Endpoint) -> Value {
    let mut stream = json!({ "network": ep.network });

    if ep.tls {
        let mut tls = json!({
            "serverName": ep.sni,
        });
        if !ep.alpn.is_empty() {
            tls["alpn"] = json!(ep.alpn);
        }
        if !ep.fingerprint.trim().is_empty() {
            tls["fingerprint"] = json!(ep.fingerprint);
        }
        // Xray 26 removed `allowInsecure`; the supported way to trust a
        // certificate that does not match the SNI is to pin it.
        if !ep.pinned_cert_sha256.trim().is_empty() {
            tls["pinnedPeerCertSha256"] = json!(ep.pinned_cert_sha256.trim());
        }
        stream["security"] = json!("tls");
        stream["tlsSettings"] = tls;
    } else {
        stream["security"] = json!("none");
    }

    match ep.network.as_str() {
        "ws" => {
            // Xray 26 deprecated `headers.Host` in favour of an independent
            // `host` field; the old form still works but warns.
            let mut ws = json!({ "path": ep.ws_path });
            if !ep.ws_host.trim().is_empty() {
                ws["host"] = json!(ep.ws_host);
            }
            if !ep.ws_headers.is_empty() {
                let mut headers = json!({});
                for (k, v) in &ep.ws_headers {
                    headers[k.as_str()] = json!(v);
                }
                ws["headers"] = headers;
            }
            stream["wsSettings"] = ws;
        }
        "h2" => {
            stream["httpSettings"] = json!({
                "host": [ep.h2_host],
                "path": ep.h2_path
            });
        }
        "grpc" => {
            let mut grpc = json!({ "serviceName": ep.grpc_service });
            if ep.grpc_multi {
                grpc["multiMode"] = json!(true);
            }
            stream["grpcSettings"] = grpc;
        }
        "httpupgrade" => {
            stream["httpupgradeSettings"] = json!({
                "path": ep.ws_path,
                "host": ep.ws_host
            });
        }
        "xhttp" => {
            let mut xhttp = json!({
                "path": ep.ws_path,
                "host": ep.ws_host
            });
            if !ep.xhttp_mode.trim().is_empty() {
                xhttp["mode"] = json!(ep.xhttp_mode.trim());
            }
            stream["xhttpSettings"] = xhttp;
        }
        "quic" => {
            let mut q = json!({});
            if !ep.quic_security.trim().is_empty() {
                q["security"] = json!(ep.quic_security.trim());
            }
            if !ep.quic_header.trim().is_empty() {
                q["header"] = json!({ "type": ep.quic_header.trim() });
            }
            stream["quicSettings"] = q;
        }
        "kcp" => {
            let mut k = json!({});
            if !ep.kcp_seed.trim().is_empty() {
                k["seed"] = json!(ep.kcp_seed.trim());
            }
            if !ep.kcp_header.trim().is_empty() {
                k["header"] = json!({ "type": ep.kcp_header.trim() });
            }
            stream["kcpSettings"] = k;
        }
        "tcp" => {
            // TCP with an optional HTTP obfuscation header.
            let header_type = if ep.tcp_header.trim().is_empty() {
                "none"
            } else {
                ep.tcp_header.trim()
            };
            if header_type == "http" {
                stream["tcpSettings"] = json!({
                    "header": {
                        "type": "http",
                        "request": {
                            "path": [ep.ws_path],
                            "headers": { "Host": [ep.ws_host] }
                        }
                    }
                });
            } else {
                stream["tcpSettings"] = json!({ "header": { "type": "none" } });
            }
        }
        _ => {}
    }
    stream
}

/// The VMess outbound (kept for the callers that only support VMess).
pub fn vmess_outbound(ep: &Endpoint) -> Value {
    build_outbound(ep)
}

/// Like [`build_config`] but lets the caller decide whether the private SOCKS
/// inbound accepts UDP. TUN mode needs UDP (DNS, QUIC, ...).
pub fn build_config_udp(ep: &Endpoint, settings: &Settings, socks_port: u16, udp: bool) -> Value {
    let outbound = vmess_outbound(ep);

    let mut config = json!({
        "log": { "loglevel": core_log_level(settings) },
        "inbounds": [{
            "tag": "local-socks",
            "listen": "127.0.0.1",
            "port": socks_port,
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": udp },
            "sniffing": { "enabled": false }
        }],
        "outbounds": [
            outbound,
            { "tag": "direct", "protocol": "freedom" },
            { "tag": "block", "protocol": "blackhole" }
        ]
    });

    if !settings.custom_dns.trim().is_empty() {
        let dns = settings
            .custom_dns
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if !dns.is_empty() {
            config["dns"] = json!({
                "servers": dns,
                "queryStrategy": "UseIP"
            });
        }
    }

    // A per-server DNS override wins over the global setting.
    if !ep.dns.trim().is_empty() {
        let dns = ep
            .dns
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if !dns.is_empty() {
            config["dns"] = json!({
                "servers": dns,
                "queryStrategy": "UseIP"
            });
        }
    }

    config
}

/// The log level of the core.
///
/// In the advanced mode the core writes its debug output, in the simple mode
/// only the problems reach the log.
pub fn core_log_level(settings: &Settings) -> String {
    match crate::logging::LogMode::parse(&settings.log_mode) {
        Some(crate::logging::LogMode::Advanced) => "debug".into(),
        _ => normalize_level(&settings.log_level),
    }
}

pub(crate) fn normalize_level(level: &str) -> String {
    match level.to_ascii_lowercase().as_str() {
        "debug" | "info" | "warning" | "error" | "none" => level.to_ascii_lowercase(),
        _ => "warning".into(),
    }
}

pub fn write_config(config: &Value, path: &Path) -> Result<()> {
    let text = serde_json::to_string_pretty(config).context("serialize xray config")?;
    std::fs::write(path, text).context("write xray config")?;
    Ok(())
}

/// Validate a configuration file *without* starting the core.
///
/// `xray run -test` parses and builds the configuration but does not open
/// any listener, so a broken config is reported before we touch the network
/// (no adapter, no routes).
pub fn validate_config(exe: &Path, config_path: &Path) -> Result<()> {
    let out = std::process::Command::new(exe)
        .arg("run")
        .arg("-test")
        .arg("-c")
        .arg(config_path)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .output()
        .context("run the xray config check")?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let text = text.trim().to_string();
    if out.status.success() {
        return Ok(());
    }
    // Xray prints "Failed to start: main: failed to load config files: ...".
    let detail = text
        .lines()
        .rev()
        .find(|l| l.contains("error") || l.contains("Error") || l.contains("failed"))
        .unwrap_or(text.as_str())
        .trim()
        .to_string();
    bail!("Xray rejected the configuration: {detail}");
}

/// Probe what the installed core supports.
///
/// The XUDP fields are only written when the core really knows them. A core
/// that does not know a field ignores it silently, so the probe uses the
/// *error* of the core: a configuration with an invalid value for
/// `xudpProxyUDP443` and a configuration with a string in `xudpConcurrency`
/// have to be rejected. A core that ignores the fields accepts both.
pub fn probe_core_caps(exe: &Path, logger: &Logger) -> CoreCaps {
    let probe_path = crate::util::runtime_config_path().with_extension("muxprobe.json");
    let probe = |mux: Value| -> Result<()> {
        let cfg = json!({
            "log": { "loglevel": "warning" },
            "inbounds": [],
            "outbounds": [{
                "tag": "probe",
                "protocol": "vmess",
                "settings": { "vnext": [{
                    "address": "127.0.0.1",
                    "port": 1,
                    "users": [{ "id": "00000000-0000-0000-0000-000000000000" }]
                }] },
                "mux": mux
            }]
        });
        write_config(&cfg, &probe_path)?;
        validate_config(exe, &probe_path)
    };

    // A configuration that uses the fields with valid values has to work.
    let valid = json!({
        "enabled": true,
        "concurrency": 8,
        "xudpConcurrency": 16,
        "xudpProxyUDP443": "reject"
    });
    if let Err(e) = probe(valid) {
        logger.warn(format!(
            "the core does not accept the XUDP fields: {e:#} - XUDP stays switched off"
        ));
        return CoreCaps { xudp: false };
    }
    // An invalid value has to be reported - that proves the field is known.
    let bad_policy = probe(json!({
        "enabled": true,
        "concurrency": 8,
        "xudpProxyUDP443": "sni-invalid-value"
    }));
    // A wrong type has to be reported as well - that proves the other field.
    let bad_type = probe(json!({
        "enabled": true,
        "concurrency": 8,
        "xudpConcurrency": "not-a-number"
    }));
    let _ = std::fs::remove_file(&probe_path);
    CoreCaps {
        xudp: bad_policy.is_err() && bad_type.is_err(),
    }
}

/// Owns the running `xray.exe` process.
pub struct XrayProcess {
    child: Child,
    #[allow(dead_code)]
    pub config_path: PathBuf,
    /// The last error line the core printed (used for accurate diagnostics).
    last_error: Arc<Mutex<String>>,
}

/// Why the core refused to start.
#[derive(Debug, Clone)]
pub enum CoreFailure {
    /// The configuration was rejected by Xray.
    ConfigRejected(String),
    /// The wintun driver could not be loaded.
    WintunMissing,
    /// Anything else, with the core's own message.
    Other(String),
}

impl CoreFailure {
    /// A message suitable for the status line and the log.
    pub fn message(&self) -> String {
        match self {
            CoreFailure::ConfigRejected(m) => format!("Xray rejected the configuration: {m}"),
            CoreFailure::WintunMissing => {
                "the wintun driver (wintun.dll) is missing or has the wrong architecture \
                 - it must sit next to xray.exe"
                    .to_string()
            }
            CoreFailure::Other(m) => m.clone(),
        }
    }
}

/// Classify the core's output into a specific failure.
pub fn classify_core_failure(text: &str) -> CoreFailure {
    let lower = text.to_ascii_lowercase();
    if lower.contains("wintun")
        && (lower.contains("load") || lower.contains("open") || lower.contains("dll"))
    {
        return CoreFailure::WintunMissing;
    }
    if lower.contains("failed to load config")
        || lower.contains("failed to build")
        || lower.contains("rejected")
    {
        let detail = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or(text)
            .trim()
            .to_string();
        return CoreFailure::ConfigRejected(detail);
    }
    CoreFailure::Other(text.trim().to_string())
}

impl XrayProcess {
    pub async fn start(exe: &Path, config_path: PathBuf, logger: &Logger) -> Result<Self> {
        Self::start_with_counter(exe, config_path, logger, None).await
    }

    /// Start the core and let `conns` count the connections it carries.
    pub async fn start_with_counter(
        exe: &Path,
        config_path: PathBuf,
        logger: &Logger,
        conns: Option<Arc<crate::stats::ConnCounter>>,
    ) -> Result<Self> {
        if !exe.is_file() {
            bail!("Xray executable not found at {}", exe.display());
        }
        let mut cmd = Command::new(exe);
        cmd.arg("run")
            .arg("-c")
            .arg(&config_path)
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().context("spawn xray process")?;
        #[cfg(windows)]
        crate::child_job::attach(child.id().context("Xray exited before supervision")?)?;

        let last_error: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stdout) = child.stdout.take() {
            spawn_pipe_reader(
                stdout,
                "core",
                logger.clone(),
                Some(last_error.clone()),
                conns.clone(),
            );
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_pipe_reader(
                stderr,
                "core",
                logger.clone(),
                Some(last_error.clone()),
                conns.clone(),
            );
        }

        logger.info("Xray core started");
        Ok(Self {
            child,
            config_path,
            last_error,
        })
    }

    /// The last error the core printed, if any.
    pub fn last_error(&self) -> String {
        self.last_error.lock().unwrap().clone()
    }

    /// Classify why the core is not running.
    pub fn failure(&self) -> Option<CoreFailure> {
        let text = self.last_error();
        if text.is_empty() {
            return None;
        }
        Some(classify_core_failure(&text))
    }

    /// Returns `Some(exit_code)` if the process has exited.
    pub fn try_exit_code(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }

    pub async fn stop(&mut self, logger: &Logger) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.child.start_kill();
                let _ = tokio::time::timeout(std::time::Duration::from_secs(3), self.child.wait())
                    .await;
                logger.info("Xray core stopped");
            }
        }
    }
}

fn core_line_level(line: &str) -> &'static str {
    let lower = line.to_ascii_lowercase();
    if lower.contains("[error]") {
        "ERROR"
    } else if lower.contains("[warning]") || lower.contains("warning") {
        "WARN"
    } else if lower.contains("[info]") {
        "INFO"
    } else if lower.contains("[debug]") {
        "DEBUG"
    } else if lower.contains("failed") || lower.contains("rejected") {
        "ERROR"
    } else {
        "INFO"
    }
}

pub(crate) fn spawn_pipe_reader<R>(
    reader: R,
    tag: &'static str,
    logger: Logger,
    sink: Option<Arc<Mutex<String>>>,
    conns: Option<Arc<crate::stats::ConnCounter>>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(c) = &conns {
                c.observe(line);
            }
            let level = core_line_level(line);
            if level == "ERROR" || line.to_ascii_lowercase().contains("failed") {
                if let Some(s) = &sink {
                    *s.lock().unwrap() = line.to_string();
                }
            }
            logger.log(level, format!("[{tag}] {line}"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{resolve_endpoint, ServerProfile, SniProfile};

    #[test]
    fn core_severity_preserves_warning_and_info() {
        assert_eq!(core_line_level("[Warning] core: Xray started"), "WARN");
        assert_eq!(core_line_level("[Warning] failed to dial"), "WARN");
        assert_eq!(core_line_level("[Error] core: stopped"), "ERROR");
        assert_eq!(core_line_level("[Info] connected"), "INFO");
        assert_eq!(core_line_level("[Debug] transport"), "DEBUG");
    }

    fn ep_ws() -> Endpoint {
        let s = ServerProfile {
            name: "s".into(),
            address: "1.2.3.4".into(),
            port: 443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            sni: "front.example".into(),
            ws_host: "front.example".into(),
            ws_path: "/x".into(),
            alpn: vec!["h2".into(), "http/1.1".into()],
            ..Default::default()
        };
        resolve_endpoint(&s, None)
    }

    #[test]
    fn vmess_outbound_is_well_formed() {
        let ep = ep_ws();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(cfg["outbounds"][0]["protocol"], "vmess");
        let vnext = &cfg["outbounds"][0]["settings"]["vnext"][0];
        assert_eq!(vnext["address"], "1.2.3.4");
        assert_eq!(vnext["port"], 443);
        assert_eq!(vnext["users"][0]["id"], ep.uuid);
        let stream = &cfg["outbounds"][0]["streamSettings"];
        assert_eq!(stream["network"], "ws");
        assert_eq!(stream["security"], "tls");
        assert_eq!(stream["tlsSettings"]["serverName"], "front.example");
        assert_eq!(stream["wsSettings"]["host"], "front.example");
        assert_eq!(stream["wsSettings"]["path"], "/x");
        assert_eq!(cfg["inbounds"][0]["protocol"], "socks");
        assert_eq!(cfg["inbounds"][0]["port"], 1081);
    }

    #[test]
    fn sni_and_address_are_independent() {
        let s = ServerProfile {
            address: "1.2.3.4".into(),
            port: 443,
            uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
            network: "ws".into(),
            tls: true,
            ..Default::default()
        };
        let p = SniProfile {
            tls: true,
            sni: "cdn.other.example".into(),
            ws_host: "cdn.other.example".into(),
            ..Default::default()
        };
        let ep = resolve_endpoint(&s, Some(&p));
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let vnext = &cfg["outbounds"][0]["settings"]["vnext"][0];
        // Dial address stays the real server; TLS name/Host point elsewhere.
        assert_eq!(vnext["address"], "1.2.3.4");
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["tlsSettings"]["serverName"],
            "cdn.other.example"
        );
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["wsSettings"]["host"],
            "cdn.other.example"
        );
    }

    #[test]
    fn tls_off_emits_none_security() {
        let mut ep = ep_ws();
        ep.tls = false;
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(cfg["outbounds"][0]["streamSettings"]["security"], "none");
        assert!(cfg["outbounds"][0]["streamSettings"]["tlsSettings"].is_null());
    }

    fn ep_with(network: &str) -> Endpoint {
        let mut ep = ep_ws();
        ep.network = network.to_string();
        ep
    }

    #[test]
    fn generates_transport_specific_settings() {
        // WebSocket with extra headers
        let mut ep = ep_with("ws");
        ep.ws_headers.insert("X-Token".into(), "abc".into());
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let ws = &cfg["outbounds"][0]["streamSettings"]["wsSettings"];
        assert_eq!(ws["path"], "/x");
        assert_eq!(ws["host"], "front.example");
        assert_eq!(ws["headers"]["X-Token"], "abc");
        // The removed/deprecated forms must not appear any more.
        assert!(ws["headers"].get("Host").is_none());
        assert!(cfg["outbounds"][0]["streamSettings"]["tlsSettings"]
            .get("allowInsecure")
            .is_none());

        // HTTP/2
        let mut ep = ep_with("h2");
        ep.h2_host = "h2.example".into();
        ep.h2_path = "/h2".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["httpSettings"]["host"][0],
            "h2.example"
        );

        // gRPC
        let mut ep = ep_with("grpc");
        ep.grpc_service = "GunService".into();
        ep.grpc_multi = true;
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let grpc = &cfg["outbounds"][0]["streamSettings"]["grpcSettings"];
        assert_eq!(grpc["serviceName"], "GunService");
        assert_eq!(grpc["multiMode"], true);

        // HTTPUpgrade
        let mut ep = ep_with("httpupgrade");
        ep.ws_path = "/up".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["httpupgradeSettings"]["path"],
            "/up"
        );

        // XHTTP
        let mut ep = ep_with("xhttp");
        ep.xhttp_mode = "stream-one".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["xhttpSettings"]["mode"],
            "stream-one"
        );

        // KCP
        let mut ep = ep_with("kcp");
        ep.kcp_seed = "seed".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["kcpSettings"]["seed"],
            "seed"
        );

        // TCP with an HTTP header
        let mut ep = ep_with("tcp");
        ep.tcp_header = "http".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(
            cfg["outbounds"][0]["streamSettings"]["tcpSettings"]["header"]["type"],
            "http"
        );
    }

    #[test]
    fn mux_and_dns_are_emitted() {
        let mut ep = ep_ws();
        ep.mux_enabled = true;
        ep.mux_concurrency = 4;
        ep.dns = "1.1.1.1, 8.8.8.8".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(cfg["outbounds"][0]["mux"]["enabled"], true);
        assert_eq!(cfg["outbounds"][0]["mux"]["concurrency"], 4);
        assert_eq!(cfg["dns"]["servers"][0], "1.1.1.1");
        assert_eq!(cfg["dns"]["servers"][1], "8.8.8.8");
    }

    #[test]
    fn mux_off_stays_compatible() {
        let ep = ep_ws();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        assert_eq!(cfg["outbounds"][0]["mux"]["enabled"], false);
        // The XUDP fields are never written when MUX is off.
        assert!(cfg["outbounds"][0]["mux"].get("xudpConcurrency").is_none());
        assert!(cfg["outbounds"][0]["mux"].get("xudpProxyUDP443").is_none());
    }

    #[test]
    fn xudp_is_only_written_when_the_core_knows_it() {
        let mut ep = ep_ws();
        ep.mux_enabled = true;
        ep.mux_xudp_enabled = true;
        ep.mux_xudp_concurrency = 16;
        ep.mux_xudp_udp443 = "allow".into();

        // A core that does not know the fields must not see them.
        let cfg = build_outbound_with_caps(&ep, CoreCaps { xudp: false });
        assert!(cfg["mux"].get("xudpConcurrency").is_none());
        assert!(cfg["mux"].get("xudpProxyUDP443").is_none());
        assert_eq!(cfg["mux"]["enabled"], true);

        // A core that knows them gets them.
        let cfg = build_outbound_with_caps(&ep, CoreCaps { xudp: true });
        assert_eq!(cfg["mux"]["xudpConcurrency"], 16);
        assert_eq!(cfg["mux"]["xudpProxyUDP443"], "allow");
    }

    #[test]
    fn xudp_is_ignored_when_mux_is_off() {
        let mut ep = ep_ws();
        ep.mux_enabled = false;
        ep.mux_xudp_enabled = true;
        let cfg = build_outbound_with_caps(&ep, CoreCaps { xudp: true });
        assert_eq!(cfg["mux"]["enabled"], false);
        assert!(cfg["mux"].get("xudpConcurrency").is_none());
    }

    #[test]
    fn the_udp443_policy_is_normalized() {
        assert_eq!(normalize_udp443("ALLOW"), "allow");
        assert_eq!(normalize_udp443(" skip "), "skip");
        assert_eq!(normalize_udp443("nonsense"), "reject");
        assert_eq!(normalize_udp443(""), "reject");
    }

    #[test]
    fn generates_vless_and_trojan_outbounds() {
        let mut ep = ep_ws();
        ep.protocol = "vless".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["protocol"], "vless");
        assert_eq!(ob["settings"]["vnext"][0]["users"][0]["id"], ep.uuid);
        assert_eq!(ob["settings"]["vnext"][0]["users"][0]["encryption"], "none");

        let mut ep = ep_ws();
        ep.protocol = "trojan".into();
        ep.uuid = "s3cret".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["protocol"], "trojan");
        assert_eq!(ob["settings"]["servers"][0]["password"], "s3cret");
        assert_eq!(ob["settings"]["servers"][0]["address"], ep.address);
        assert_eq!(ob["settings"]["servers"][0]["port"], ep.port);
    }

    #[test]
    fn removed_allow_insecure_is_never_emitted() {
        // Xray 26 removed `allowInsecure`; emitting it makes the core refuse
        // to start (exit code 23).
        let mut ep = ep_ws();
        ep.allow_insecure = true;
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let tls = &cfg["outbounds"][0]["streamSettings"]["tlsSettings"];
        assert!(
            tls.get("allowInsecure").is_none(),
            "allowInsecure must not be in the config any more"
        );
    }

    #[test]
    fn a_pinned_certificate_replaces_allow_insecure() {
        let mut ep = ep_ws();
        ep.allow_insecure = true;
        ep.pinned_cert_sha256 =
            "e8e2d387fdbffeb38e9c9065cf30a97ee23c0e3d32ee6f78ffae40966befccc9".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let tls = &cfg["outbounds"][0]["streamSettings"]["tlsSettings"];
        assert_eq!(
            tls["pinnedPeerCertSha256"],
            "e8e2d387fdbffeb38e9c9065cf30a97ee23c0e3d32ee6f78ffae40966befccc9"
        );
        assert!(tls.get("allowInsecure").is_none());
    }

    #[test]
    fn sni_and_host_stay_independent_in_the_generated_config() {
        let mut ep = ep_ws();
        ep.address = "10.0.0.9".into();
        ep.sni = "ea.com".into();
        ep.ws_host = "front.example".into();
        ep.ws_path = "/ray".into();
        let cfg = build_config(&ep, &Settings::default(), 1081);
        let vnext = &cfg["outbounds"][0]["settings"]["vnext"][0];
        let stream = &cfg["outbounds"][0]["streamSettings"];
        assert_eq!(vnext["address"], "10.0.0.9");
        assert_eq!(stream["tlsSettings"]["serverName"], "ea.com");
        assert_eq!(stream["wsSettings"]["host"], "front.example");
        assert_eq!(stream["wsSettings"]["path"], "/ray");
    }
}
