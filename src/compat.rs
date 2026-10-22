//! Explicit NetMod-core compatibility experiment. The pinned modern core still
//! owns TUN; the installed NetMod 1.6.1 core owns the remote VMess connection.
use crate::{
    logging::Logger,
    model::Endpoint,
    util,
    xray::{self, XrayProcess},
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

pub fn executable() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe.parent().unwrap().join("compat/v2ray.exe");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    let root = std::env::var_os("ProgramFiles(x86)")
        .context("NetMod compatibility requires Windows and an installed NetMod")?;
    let path = PathBuf::from(root).join("NetMod/bin/v2ray.exe");
    anyhow::ensure!(
        path.is_file(),
        "NetMod's bundled core was not found; disable NetMod compatibility or install NetMod"
    );
    Ok(path)
}

pub fn validate_endpoint(ep: &Endpoint) -> Result<()> {
    anyhow::ensure!(
        ep.protocol == "vmess" && ep.network == "ws" && ep.tls,
        "NetMod compatibility currently supports VMess + WebSocket + TLS only"
    );
    anyhow::ensure!(ep.pinned_cert_sha256.trim().is_empty(),
        "NetMod 1.6.1 cannot preserve an explicit certificate pin; disable compatibility to use this profile");
    anyhow::ensure!(
        !ep.mux_xudp_enabled,
        "NetMod compatibility does not support the newer XUDP settings"
    );
    Ok(())
}

pub fn config(ep: &Endpoint, port: u16, uplink: &str, level: &str) -> Result<Value> {
    validate_endpoint(ep)?;
    let mut headers = serde_json::to_value(&ep.ws_headers)?;
    if !ep.ws_host.is_empty() {
        headers["Host"] = json!(ep.ws_host);
    }
    let mut tls = json!({"serverName":ep.sni,"allowInsecure":ep.allow_insecure,
        "allowInsecureCiphers": ep.allow_insecure});
    if !ep.alpn.is_empty() {
        tls["alpn"] = json!(ep.alpn);
    }
    if !ep.fingerprint.is_empty() {
        tls["fingerprint"] = json!(ep.fingerprint);
    }
    let mut outbound = json!({"tag":"proxy", "protocol":"vmess",
        "settings":{"vnext":[{"address":ep.address,"port":ep.port,
            "users":[{"id":ep.uuid,"alterId":ep.alter_id,"security":ep.security}]}]},
        "streamSettings":{"network":"ws","security":"tls","tlsSettings":tls,
            "wsSettings":{"path":ep.ws_path,"headers":headers}},
        "mux":{"enabled":ep.mux_enabled,"concurrency":ep.mux_concurrency}});
    if !uplink.is_empty() {
        uplink
            .parse::<std::net::Ipv4Addr>()
            .context("compatibility uplink must be a physical IPv4 address")?;
        outbound["sendThrough"] = json!(uplink);
    }
    let mut doc = json!({"log":{"loglevel":level},
        "policy":{"0":{"handshake":4,"connIdle":300,"uplinkOnly":0,"downlinkOnly":0,"bufferSize":0}},
        "inbounds":[{"tag":"compat-socks","listen":"127.0.0.1","port":port,"protocol":"socks",
            "settings":{"auth":"noauth","udp":true},"sniffing":{"enabled":false,"destOverride":[]}}],
        "outbounds":[outbound,{"protocol":"freedom","tag":"direct"},
            {"protocol":"blackhole","tag":"block","settings":{"response":{"type":"http"}}}],
        "routing":{"domainMatcher":"mph","domainStrategy":"IPIfNonMatch","rules":[]}});
    if !ep.dns.trim().is_empty() {
        doc["dns"] = json!({"servers":ep.dns.split(',').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>()});
    }
    Ok(doc)
}

pub fn bridge_outbound(port: u16) -> Value {
    json!({"tag":"netmod-local","protocol":"socks",
        "settings":{"servers":[{"address":"127.0.0.1","port":port}]}})
}

pub struct CompatibilityCore {
    core: XrayProcess,
    pub port: u16,
    path: PathBuf,
}
impl CompatibilityCore {
    pub async fn start(ep: &Endpoint, uplink: &str, level: &str, logger: &Logger) -> Result<Self> {
        let exe = executable()?;
        let mut command = tokio::process::Command::new(&exe);
        command.arg("version").kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let output = tokio::time::timeout(Duration::from_secs(5), command.output()).await??;
        anyhow::ensure!(output.status.success() && String::from_utf8_lossy(&output.stdout).starts_with("Xray 1.6.1 "),
            "This compatibility mode was validated with NetMod's Xray 1.6.1; installed core differs");
        let port = util::free_port();
        let document = config(ep, port, uplink, level)?;
        let path = util::app_dir().join(format!("netmod-session-{}.json", uuid::Uuid::new_v4()));
        xray::write_config(&document, &path)?;
        let result = async {
            xray::validate_config(&exe, &path)?;
            let mut core = XrayProcess::start(&exe, path.clone(), logger).await?;
            let ready = tokio::time::timeout(Duration::from_secs(8), async {
                loop {
                    if core.try_exit_code().is_some() { bail!("NetMod compatibility core exited before becoming ready"); }
                    if tokio::net::TcpStream::connect(("127.0.0.1",port)).await.is_ok() { return Ok(()); }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }).await.context("NetMod compatibility core did not open its listener")?;
            ready?;
            logger.event("NetMod compatibility active: installed Xray 1.6.1 handles the server connection");
            if ep.allow_insecure { logger.warn("NetMod compatibility: Allow insecure is enabled; server identity is not verified"); }
            Ok(Self { core, port, path: path.clone() })
        }.await;
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result
    }
    pub fn exit_code(&mut self) -> Option<i32> {
        self.core.try_exit_code()
    }
    pub async fn stop(&mut self, logger: &Logger) {
        self.core.stop(logger).await;
    }
}
impl Drop for CompatibilityCore {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn endpoint() -> Endpoint {
        Endpoint {
            protocol: "vmess".into(),
            network: "ws".into(),
            tls: true,
            sni: "sni.example".into(),
            ws_host: "ws.example".into(),
            address: "server.example".into(),
            port: 443,
            uuid: "test-credential".into(),
            allow_insecure: true,
            ..Default::default()
        }
    }
    #[test]
    fn legacy_transport_preserves_independent_fields_and_binds_uplink() {
        let v = config(&endpoint(), 12345, "192.0.2.4", "warning").unwrap();
        let o = &v["outbounds"][0];
        assert_eq!(o["settings"]["vnext"][0]["address"], "server.example");
        assert_eq!(
            o["settings"]["vnext"][0]["users"][0]["id"],
            "test-credential"
        );
        assert_eq!(
            o["streamSettings"]["tlsSettings"]["serverName"],
            "sni.example"
        );
        assert_eq!(
            o["streamSettings"]["wsSettings"]["headers"]["Host"],
            "ws.example"
        );
        assert!(o["streamSettings"]["wsSettings"]["host"].is_null());
        assert_eq!(o["sendThrough"], "192.0.2.4");
        assert_eq!(v["inbounds"][0]["settings"]["udp"], true);
    }
    #[test]
    fn refuses_to_silently_drop_pins_or_unsupported_transport() {
        let mut ep = endpoint();
        ep.pinned_cert_sha256 = "ab".repeat(32);
        assert!(validate_endpoint(&ep).is_err());
        ep.pinned_cert_sha256.clear();
        ep.network = "grpc".into();
        assert!(validate_endpoint(&ep).is_err());
    }
    #[test]
    fn bridge_is_loopback_only_and_has_no_remote_tls() {
        let v = bridge_outbound(12345);
        assert_eq!(v["settings"]["servers"][0]["address"], "127.0.0.1");
        assert!(v["streamSettings"].is_null());
    }
}
