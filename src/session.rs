//! Headless tunnel session: ensures the core binary, generates the Xray
//! config, starts the Xray subprocess, and puts the local HTTP/SOCKS proxy in
//! front of it. Reused by the GUI later.

use crate::corebin::{self, xray_path, XRAY_PACKAGE};
use crate::localproxy::LocalProxy;
use crate::logging::Logger;
use crate::model::{Endpoint, Settings};
use crate::stats::Traffic;
use crate::util;
use crate::xray::{self, XrayProcess};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

pub struct Session {
    core: XrayProcess,
    proxy: LocalProxy,
    pub traffic: Arc<Traffic>,
    pub http_port: u16,
    pub socks_port: u16,
    #[allow(dead_code)]
    pub endpoint: Option<Endpoint>,
}

impl Session {
    pub async fn start(
        ep: Endpoint,
        settings: &Settings,
        verify_hash: bool,
        logger: &Logger,
    ) -> Result<Self> {
        anyhow::ensure!(
            !settings.netmod_compatibility,
            "NetMod compatibility requires TUN mode"
        );
        ep.validate()
            .map_err(|e| anyhow::anyhow!("invalid endpoint: {e}"))?;
        logger.info(format!(
            "Endpoint: dial {}:{}, tls={}, sni='{}', network={}, ws_host='{}'",
            ep.address, ep.port, ep.tls, ep.sni, ep.network, ep.ws_host
        ));

        corebin::ensure_package(&XRAY_PACKAGE, verify_hash, logger).await?;

        // Find out what the core understands (XUDP) before the configuration
        // is written, so the configuration only uses fields it knows.
        xray::ensure_core_caps(&xray_path(), logger);

        let internal = util::free_port();
        #[cfg(target_os = "linux")]
        let cfg = if ep.network == "ws" && ep.tls && ep.pinned_cert_sha256.is_empty() {
            crate::compat::config(&ep, internal, "", &settings.log_level)?
        } else {
            xray::build_config(&ep, settings, internal)
        };
        #[cfg(not(target_os = "linux"))]
        let cfg = xray::build_config(&ep, settings, internal);
        let cfg_path = util::runtime_config_path();
        xray::write_config(&cfg, &cfg_path)?;
        logger.info(format!("Wrote runtime config {}", cfg_path.display()));

        // A broken configuration must not leave a half-open tunnel behind, so
        // the core validates it before it is started.
        xray::validate_config(&xray_path(), &cfg_path)
            .map_err(|e| anyhow::anyhow!("the Xray configuration was rejected: {e:#}"))?;

        logger.event("Starting Xray-core");
        let core = XrayProcess::start(&xray_path(), cfg_path, logger).await?;
        logger.event("Xray-core started successfully");
        wait_for_port(internal, Duration::from_secs(10))
            .await
            .context("Xray core did not start its local SOCKS inbound")?;

        let traffic = Arc::new(Traffic::default());
        let upstream: SocketAddr = format!("127.0.0.1:{internal}").parse().unwrap();
        // A per-server port overrides the global setting (0 = use global).
        let http_port = if ep.http_port != 0 {
            ep.http_port
        } else {
            settings.http_port
        };
        let socks_port = if ep.socks_port != 0 {
            ep.socks_port
        } else {
            settings.socks_port
        };
        let proxy = LocalProxy::start(
            http_port,
            socks_port,
            upstream,
            traffic.clone(),
            logger.clone(),
        )
        .await?;
        // The listeners are only announced when they really accept
        // connections.
        logger.event(format!("SOCKS proxy listening on port {socks_port}"));
        logger.event(format!("HTTP proxy listening on port {http_port}"));

        Ok(Self {
            core,
            proxy,
            traffic,
            http_port,
            socks_port,
            endpoint: Some(ep),
        })
    }

    /// Start a session whose outbound is `freedom` (direct). Used by the
    /// self-test to validate the local proxy + Xray plumbing without needing a
    /// VMess server.
    pub async fn start_direct(
        http_port: u16,
        socks_port: u16,
        verify_hash: bool,
        logger: &Logger,
    ) -> Result<Self> {
        corebin::ensure_package(&XRAY_PACKAGE, verify_hash, logger).await?;
        let internal = util::free_port();
        let cfg = json!({
            "log": { "loglevel": "warning" },
            "inbounds": [{
                "tag": "local-socks",
                "listen": "127.0.0.1",
                "port": internal,
                "protocol": "socks",
                "settings": { "auth": "noauth", "udp": false }
            }],
            "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
        });
        let cfg_path = util::runtime_config_path();
        xray::write_config(&cfg, &cfg_path)?;
        let core = XrayProcess::start(&xray_path(), cfg_path, logger).await?;
        wait_for_port(internal, Duration::from_secs(10)).await?;

        let traffic = Arc::new(Traffic::default());
        let upstream: SocketAddr = format!("127.0.0.1:{internal}").parse().unwrap();
        let proxy = LocalProxy::start(
            http_port,
            socks_port,
            upstream,
            traffic.clone(),
            logger.clone(),
        )
        .await?;
        Ok(Self {
            core,
            proxy,
            traffic,
            http_port,
            socks_port,
            endpoint: None,
        })
    }

    pub fn core_exit_code(&mut self) -> Option<i32> {
        self.core.try_exit_code()
    }

    pub async fn stop(mut self, logger: &Logger) {
        self.proxy.stop();
        self.core.stop(logger).await;
    }
}

async fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Ok(_)) = tokio::time::timeout(
            Duration::from_millis(400),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("port 127.0.0.1:{port} was never opened");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}
