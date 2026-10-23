//! Tunnel engine: a single actor that owns the live session, mutates the
//! Windows system proxy, reports status/stats, detects core death and can
//! auto-reconnect. The GUI will drive this through the same channel API.

use crate::logging::Logger;
use crate::model::{resolve_endpoint, AppConfig, Endpoint, Settings};
use crate::netcheck;
use crate::session::Session;
use crate::stats::Snapshot as TrafficSnapshot;
use crate::sysproxy;
use crate::tun;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

pub const TEST_URL: &str = "https://api.ipify.org";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Disconnected,
    Preparing,
    Authenticating,
    Configuring,
    Reconnecting,
    Degraded,
    /// 1. The core process is running.
    CoreReady,
    /// 2. The local proxy listeners accept connections.
    ListenersReady,
    Verifying,
    /// 3. The VMess tunnel really carries traffic.
    Forwarding,
    /// 4. The TUN adapter is initialised.
    TunReady,
    /// 5. The routes that send the traffic into the tunnel are installed.
    RoutesActive,
    /// 6. The DNS of the machine goes through the tunnel.
    DnsReady,
    /// 7. Ordinary system traffic leaves through the tunnel.
    FullForwarding,
    Disconnecting,
    Error,
}

impl Phase {
    /// Is a VMess session currently held open (core running)?
    #[allow(dead_code)]
    pub fn session_up(self) -> bool {
        matches!(
            self,
            Phase::CoreReady
                | Phase::ListenersReady
                | Phase::Verifying
                | Phase::Forwarding
                | Phase::TunReady
                | Phase::RoutesActive
                | Phase::DnsReady
                | Phase::FullForwarding
        )
    }

    /// Is the tunnel carrying traffic (not just the core running)?
    pub fn forwarding(self) -> bool {
        matches!(
            self,
            Phase::Forwarding | Phase::DnsReady | Phase::FullForwarding
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Phase::Disconnected => "DISCONNECTED",
            Phase::Preparing => "CONNECTING",
            Phase::Authenticating => "AUTHENTICATING",
            Phase::Configuring => "CONFIGURING TUNNEL",
            Phase::Reconnecting => "RECONNECTING",
            Phase::Degraded => "DEGRADED",
            Phase::CoreReady => "XRAY STARTED",
            Phase::ListenersReady => "LISTENERS READY",
            Phase::Verifying => "VERIFYING",
            Phase::Forwarding => "CONNECTED",
            Phase::TunReady => "TUN ADAPTER READY",
            Phase::RoutesActive => "VPN ROUTES ACTIVE",
            Phase::DnsReady => "DNS VERIFIED",
            Phase::FullForwarding => "CONNECTED",
            Phase::Disconnecting => "DISCONNECTING",
            Phase::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: Phase,
    pub detail: String,
    pub external_ip: Option<String>,
    pub server_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct StatsEvent {
    pub up: u64,
    pub down: u64,
    pub up_speed: u64,
    pub down_speed: u64,
    pub active: i64,
    #[allow(dead_code)]
    pub total: u64,
    pub uptime_secs: u64,
}

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Status(Status),
    Stats(StatsEvent),
    Protection(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Connect,
    Disconnect,
    /// Tear the tunnel down and bring it up again with the current config.
    Reconnect,
    TestConnection,
    /// Check that ordinary system traffic really uses the tunnel.
    VerifyFullTunnel,
    RefreshIp,
    Shutdown,
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::UnboundedSender<Command>,
    pub events: broadcast::Sender<EngineEvent>,
    shutdown_done: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
}

impl EngineHandle {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    /// Stop the engine and wait for connection cleanup to finish.
    pub fn shutdown_and_wait(&self, timeout: Duration) {
        self.send(Command::Shutdown);
        if let Some(done) = self.shutdown_done.lock().unwrap().take() {
            let _ = done.recv_timeout(timeout);
        }
    }

    pub async fn shutdown_and_wait_async(&self, timeout: Duration) {
        self.send(Command::Shutdown);
        if let Some(done) = self.shutdown_done.lock().unwrap().take() {
            let _ = tokio::task::spawn_blocking(move || done.recv_timeout(timeout)).await;
        }
    }
}

pub fn spawn(cfg: Arc<Mutex<AppConfig>>, logger: Logger) -> EngineHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let (events, _) = broadcast::channel(256);
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let handle = EngineHandle {
        tx,
        events: events.clone(),
        shutdown_done: Arc::new(Mutex::new(Some(shutdown_rx))),
    };

    // Crash recovery: never leave the user's machine proxied after a crash.
    match sysproxy::recover_if_needed() {
        Ok(true) => logger.warn("recovered and restored system proxy settings from a previous run"),
        Ok(false) => {}
        Err(e) => logger.warn(format!("system proxy recovery failed: {e:#}")),
    }

    let mut core = Core {
        cfg,
        logger,
        rx,
        events,
        live: None,
        proxy_snap: None,
        connected_at: None,
        last: TrafficSnapshot::default(),
        last_tick: Instant::now(),
        retries: 0,
        network_waiting: false,
        reconnect_at: None,
        recovery_endpoint: None,
        recovery_server_id: None,
        #[cfg(target_os = "linux")]
        linux_tunnel: None,
        #[cfg(target_os = "linux")]
        linux_recovery_needed: false,
        full_tunnel: false,
        trusted_exit: None,
        last_health: Instant::now(),
        health_failures: 0,
        health_task: None,
        tun_counter_baseline: None,
    };

    tokio::spawn(async move {
        core.emit(Phase::Disconnected, "idle".into(), None);
        #[cfg(windows)]
        let _network_watch = match crate::network_events::Watch::new() {
            Ok(watch) => Some(watch),
            Err(error) => {
                core.logger
                    .warn(format!("Native network events unavailable: {error:#}"));
                None
            }
        };
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                cmd = core.rx.recv() => match cmd {
                    Some(Command::Shutdown) | None => {
                        core.teardown().await;
                        let _ = shutdown_tx.send(());
                        break;
                    }
                    Some(c) => core.handle(c).await,
                },
                _ = tick.tick() => core.on_tick().await,
            }
        }
    });

    handle
}

/// The live transport the engine is holding open.
#[allow(clippy::large_enum_variant)]
enum Live {
    /// System-proxy mode: our local HTTP/SOCKS proxy in front of Xray.
    Proxy(Box<Session>),
    /// TUN mode: Xray's native wintun adapter.
    Tun(tun::TunSession),
}

impl Live {
    fn core_exit_code(&mut self) -> Option<i32> {
        match self {
            Live::Proxy(s) => s.core_exit_code(),
            Live::Tun(t) => t.core_exit_code(),
        }
    }
}

struct Core {
    cfg: Arc<Mutex<AppConfig>>,
    logger: Logger,
    rx: mpsc::UnboundedReceiver<Command>,
    events: broadcast::Sender<EngineEvent>,
    live: Option<Live>,
    proxy_snap: Option<sysproxy::Snapshot>,
    connected_at: Option<Instant>,
    last: TrafficSnapshot,
    last_tick: Instant,
    retries: u32,
    network_waiting: bool,
    reconnect_at: Option<Instant>,
    recovery_endpoint: Option<Endpoint>,
    recovery_server_id: Option<String>,
    #[cfg(target_os = "linux")]
    linux_tunnel: Option<crate::linux_tunnel::LinuxTunnel>,
    #[cfg(target_os = "linux")]
    linux_recovery_needed: bool,
    /// The public IP seen before the tunnel was started.
    /// Is ordinary system traffic confirmed to use the tunnel?
    full_tunnel: bool,
    trusted_exit: Option<String>,
    last_health: Instant,
    health_failures: u8,
    health_task: Option<tokio::task::JoinHandle<anyhow::Result<Option<String>>>>,
    /// Absolute adapter counters when the current tunnel session started.
    tun_counter_baseline: Option<(u64, u64)>,
}

impl Core {
    fn emit(&self, phase: Phase, detail: String, ip: Option<String>) {
        // The event goes to the log (it is shown in the simple mode), the
        // phase is kept in the detailed log.
        self.logger.event(detail.clone());
        self.logger.info(format!("[{}] {detail}", phase.label()));
        let _ = self.events.send(EngineEvent::Status(Status {
            phase,
            detail,
            external_ip: ip,
            server_id: self.recovery_server_id.clone(),
        }));
    }

    async fn handle(&mut self, c: Command) {
        #[cfg(target_os = "linux")]
        if matches!(
            c,
            Command::TestConnection | Command::VerifyFullTunnel | Command::RefreshIp
        ) {
            if let Some(tunnel) = self.linux_tunnel.as_mut() {
                if let Err(e) = tunnel.verify().await {
                    self.emit(
                        Phase::Error,
                        format!("Helper verification failed: {e}"),
                        None,
                    );
                }
                return;
            }
        }
        match c {
            Command::Connect => {
                self.trusted_exit = None;
                self.reconnect_at = None;
                self.recovery_endpoint = None;
                self.recovery_server_id = None;
                self.retries = 0;
                self.connect().await;
            }
            Command::Disconnect => {
                #[cfg(target_os = "linux")]
                if self.linux_tunnel.is_none() && self.cfg.lock().unwrap().settings.mode == "tun" {
                    self.linux_recovery_needed = true;
                }
                self.trusted_exit = None;
                self.reconnect_at = None;
                self.recovery_endpoint = None;
                self.recovery_server_id = None;
                self.emit(Phase::Disconnecting, "shutting down".into(), None);
                self.teardown().await;
                self.emit(Phase::Disconnected, "disconnected".into(), None);
            }
            Command::Reconnect => {
                self.trusted_exit = None;
                self.reconnect_at = None;
                self.recovery_endpoint = None;
                self.recovery_server_id = None;
                self.retries = 0;
                self.emit(Phase::Disconnecting, "reconnecting".into(), None);
                self.teardown().await;
                self.connect().await;
            }
            Command::TestConnection => self.test_connection().await,
            Command::VerifyFullTunnel => self.check_health(true).await,
            Command::RefreshIp => self.refresh_ip().await,
            Command::Shutdown => {}
        }
    }

    async fn connect(&mut self) {
        #[cfg(target_os = "linux")]
        if self.linux_tunnel.is_some() {
            return;
        }
        if self.live.is_some() {
            return;
        }
        if let Err(error) = crate::util::ensure_no_competing_client() {
            self.emit(Phase::Error, error.to_string(), None);
            return;
        }
        // Reset only connection-scoped state. Saved servers and preferences
        // live in AppConfig and are deliberately left alone.
        self.last = TrafficSnapshot::default();
        self.last_tick = Instant::now();
        self.tun_counter_baseline = None;
        self.full_tunnel = false;
        self.health_failures = 0;
        let _ = self.events.send(EngineEvent::Stats(StatsEvent::default()));
        self.emit(
            Phase::Preparing,
            "Resolving server configuration".into(),
            None,
        );

        if self.recovery_server_id.is_none() {
            self.recovery_server_id = self.cfg.lock().unwrap().selected_server.clone();
        }
        let (mut ep, settings, mode, verify_hash) = {
            let cfg = self.cfg.lock().unwrap();
            let server = match cfg.selected_server_profile() {
                Some(s) => s.clone(),
                None => {
                    self.emit(Phase::Error, "no server selected".into(), None);
                    return;
                }
            };
            let sni = cfg.selected_sni().cloned();
            (
                self.recovery_endpoint
                    .clone()
                    .unwrap_or_else(|| resolve_endpoint(&server, sni.as_ref())),
                cfg.settings.clone(),
                cfg.settings.mode.clone(),
                cfg.settings.verify_core_hash,
            )
        };

        if ep.dns.trim().is_empty() {
            ep.dns = settings.custom_dns.clone();
        }
        self.recovery_endpoint = Some(ep.clone());
        if let Err(e) = ep.validate() {
            self.emit(Phase::Error, format!("invalid endpoint: {e}"), None);
            return;
        }

        #[cfg(target_os = "linux")]
        if mode == "tun" {
            self.emit(
                Phase::Authenticating,
                "Authorize the network helper; the interface stays unprivileged".into(),
                None,
            );
            match crate::linux_tunnel::LinuxTunnel::start(&ep, &settings, &self.logger).await {
                Ok(tunnel) => {
                    self.linux_tunnel = Some(tunnel);
                }
                Err(e) => self.emit(Phase::Error, format!("Linux TUN: {e:#}"), None),
            }
            return;
        }
        // Xray 26 removed `allowInsecure`: pin the server certificate instead.
        let certificate_result = if cfg!(target_os = "linux") {
            crate::compat::validate_endpoint(&ep)
        } else if settings.netmod_compatibility {
            if mode != "tun" {
                Err(anyhow::anyhow!("NetMod compatibility requires TUN mode"))
            } else {
                crate::compat::validate_endpoint(&ep)
            }
        } else {
            netcheck::ensure_cert_pin(&mut ep).await
        };
        if let Err(e) = certificate_result {
            self.emit(Phase::Error, format!("{e:#}"), None);
            self.schedule_reconnect(&settings);
            return;
        }
        if !ep.pinned_cert_sha256.trim().is_empty() {
            self.logger.info(format!(
                "pinning the server certificate (sha256 {})",
                &ep.pinned_cert_sha256[..16.min(ep.pinned_cert_sha256.len())]
            ));
        }

        if mode == "tun" {
            self.connect_tun(ep, settings, verify_hash).await;
            return;
        }

        let session = match Session::start(ep, &settings, verify_hash, &self.logger).await {
            Ok(s) => s,
            Err(e) => {
                let detail = format!("could not start tunnel: {e:#}");
                let permanent = detail.contains("configuration was rejected")
                    || detail.contains("invalid endpoint")
                    || detail.contains("does not support");
                self.emit(Phase::Error, detail, None);
                if !permanent {
                    self.schedule_reconnect(&settings);
                }
                return;
            }
        };
        let http_port = session.http_port;
        let socks_port = session.socks_port;
        self.live = Some(Live::Proxy(Box::new(session)));
        self.connected_at = Some(Instant::now());
        self.last_health = Instant::now();

        // System proxy is only applied after the core is confirmed listening,
        // so we never route the user's traffic into a dead tunnel.
        if mode == "system" {
            match sysproxy::enable(http_port, socks_port) {
                Ok(snap) => {
                    self.proxy_snap = Some(snap);
                    self.logger.info(format!(
                        "System proxy enabled -> HTTP 127.0.0.1:{http_port}, SOCKS 127.0.0.1:{socks_port}"
                    ));
                }
                Err(e) => {
                    let detail = format!("Desktop system proxy could not be configured: {e:#}. Choose manual local proxy or TUN mode.");
                    self.teardown().await;
                    self.emit(Phase::Error, detail, None);
                    return;
                }
            }
        } else {
            self.logger
                .warn("mode is not 'system'; system proxy left unchanged");
        }

        self.emit(
            Phase::CoreReady,
            "core running, local proxy listening (traffic not yet verified)".into(),
            None,
        );
        self.verify().await;
    }

    /// Bring up the wintun TUN tunnel. The system proxy is deliberately left
    /// alone: the TUN captures traffic at the device level.
    async fn connect_tun(&mut self, mut ep: Endpoint, settings: Settings, verify_hash: bool) {
        if !tun::is_elevated() {
            self.emit(
                Phase::Error,
                "TUN mode needs Administrator privileges - restart the app as Administrator".into(),
                None,
            );
            return;
        }

        // Resolve the server now (before routing changes) so the server itself
        // can be kept outside the tunnel.
        let mut exclusions = Vec::new();
        match netcheck::resolve(&ep.address, ep.port).await {
            Ok(r) => {
                exclusions = r.addrs.iter().map(|a| a.to_string()).collect();
                self.logger.info(format!(
                    "TUN mode: server {} resolved to {:?} (kept outside the tunnel)",
                    r.host, r.addrs
                ));
            }
            Err(e) => self.logger.warn(format!(
                "TUN mode: could not resolve the server address: {e:#}"
            )),
        }

        if let Some(address) = exclusions
            .iter()
            .find(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
        {
            ep.address = address.clone();
            self.recovery_endpoint = Some(ep.clone());
        }
        let endpoint_port = ep.port;
        let mode = tun::RouteMode::DefaultWithExclusions(exclusions);
        match tun::TunSession::start(ep, &settings, mode, verify_hash, &self.logger).await {
            Ok(mut s) => {
                #[cfg(windows)]
                if settings.kill_switch {
                    if let Err(error) =
                        crate::kill_switch::install(&s.device, &s.server_addrs, endpoint_port)
                    {
                        s.stop(&self.logger, false).await;
                        self.emit(
                            Phase::Error,
                            format!("Cannot enforce kill switch: {error:#}"),
                            None,
                        );
                        return;
                    }
                }
                let _ = self.events.send(EngineEvent::Protection(
                    if settings.kill_switch {
                        "Active · WFP output filters"
                    } else {
                        "Disabled"
                    }
                    .into(),
                ));
                self.last = TrafficSnapshot::default();
                self.last_tick = Instant::now();
                self.tun_counter_baseline = tun::adapter_traffic(&s.device);
                self.live = Some(Live::Tun(s));
                self.connected_at = Some(Instant::now());
                self.last_health = Instant::now();
            }
            Err(e) => {
                // Preserve the requested device-wide mode. A local proxy is
                // not an equivalent fallback: many applications bypass it.
                let mut detail = describe_tun_error(&e);
                let conflicts = tun::conflicting_vpn_clients(&self.logger).await;
                if !conflicts.is_empty() {
                    detail = format!(
                        "{detail} — {}",
                        tun::TunFailure::VpnConflict(conflicts.join(", ")).message()
                    );
                }
                self.logger.error(format!("TUN setup failed: {detail}"));
                self.emit(Phase::Error, detail, None);
                self.logger.warn("TUN setup failed; system proxy fallback is disabled. Device-wide protection is unavailable.");
                self.schedule_reconnect(&settings);
                return;
            }
        }

        self.emit(
            Phase::CoreReady,
            "TUN adapter up, routes installed (traffic not yet verified)".into(),
            None,
        );
        self.verify().await;
    }

    async fn verify(&mut self) {
        let (is_tun, http_port) = match self.live.as_ref() {
            Some(Live::Proxy(s)) => (false, Some(s.http_port)),
            Some(Live::Tun(_)) => (true, None),
            None => return,
        };

        if is_tun {
            self.check_health(true).await;
            return;
        }

        let http_port = http_port.unwrap_or_default();
        self.emit(
            Phase::Verifying,
            "Verifying VMess connectivity".into(),
            None,
        );

        match netcheck::proxied_get(http_port, TEST_URL, Duration::from_secs(25)).await {
            Ok(r) if r.status == 200 => {
                let ip = if r.body.is_empty() {
                    None
                } else {
                    Some(r.body.clone())
                };
                self.emit(
                    Phase::Forwarding,
                    format!("VMess connectivity verified ({} ms)", r.ms),
                    ip,
                );
            }
            Ok(r) => {
                let detail = format!(
                    "proxy answered HTTP {} - traffic may not be tunnelled",
                    r.status
                );
                self.emit(Phase::Error, detail, None);
            }
            Err(e) => {
                // Run the full diagnostic ladder so the log shows *where* the
                // tunnel breaks instead of one generic message.
                self.logger.info(format!("forwarding failed: {e:#}"));
                let ep = self.current_endpoint();
                let diag =
                    netcheck::diagnose(&ep, Some(http_port), TEST_URL, Duration::from_secs(15))
                        .await;
                self.logger
                    .info(format!("diagnostic report:\n{}", diag.report()));
                let detail = format!(
                    "{} (core is running, but traffic is not being forwarded)",
                    diag.summary()
                );
                self.emit(Phase::Error, detail, None);
                let auto = self.cfg.lock().unwrap().settings.auto_reconnect;
                if auto {
                    self.schedule_reconnect_inner();
                }
            }
        }
    }

    /// A detailed report about the current full-tunnel state.
    /// The endpoint the engine is currently configured for.
    fn current_endpoint(&self) -> Endpoint {
        let cfg = self.cfg.lock().unwrap();
        match cfg.selected_server_profile() {
            Some(s) => resolve_endpoint(s, cfg.selected_sni()),
            None => Endpoint::default(),
        }
    }

    fn schedule_reconnect(&mut self, settings: &crate::model::Settings) {
        if settings.auto_reconnect {
            self.schedule_reconnect_inner();
        }
    }

    fn schedule_reconnect_inner(&mut self) {
        let budget = self.cfg.lock().unwrap().settings.health.attempts();
        if self.retries >= budget {
            self.logger.error("Recovery attempts exhausted");
            self.reconnect_at = None;
            self.emit(
                Phase::Error,
                "Recovery failed; use Reconnect to retry".into(),
                None,
            );
            return;
        }
        self.retries += 1;
        let delay = (1u64 << (self.retries - 1)).min(15);
        let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]);
        self.reconnect_at = Some(Instant::now() + Duration::from_millis(delay * 1000 + jitter));
        self.emit(
            Phase::Reconnecting,
            format!("Recovery attempt {}/{budget} in {delay}s", self.retries),
            None,
        );
    }

    async fn test_connection(&mut self) {
        let (is_tun, http_port) = match self.live.as_ref() {
            Some(Live::Proxy(s)) => (false, Some(s.http_port)),
            Some(Live::Tun(_)) => (true, None),
            None => {
                self.logger
                    .info("test: no active tunnel; running server reachability checks");
                let ep = {
                    let cfg = self.cfg.lock().unwrap();
                    match cfg.selected_server_profile() {
                        Some(s) => resolve_endpoint(s, cfg.selected_sni()),
                        None => {
                            self.logger.error("test: no server selected");
                            return;
                        }
                    }
                };
                match netcheck::resolve(&ep.address, ep.port).await {
                    Ok(r) => self
                        .logger
                        .info(format!("test: DNS {} -> {:?}", r.host, r.addrs)),
                    Err(e) => self.logger.error(format!("test: {e:#}")),
                }
                match netcheck::tcp_connect(&ep.address, ep.port, Duration::from_secs(10)).await {
                    Ok((a, ms)) => self.logger.info(format!("test: TCP {a} ok ({ms} ms)")),
                    Err(e) => self.logger.error(format!("test: {e:#}")),
                }
                if ep.tls {
                    match netcheck::tls_probe(
                        &ep.address,
                        ep.port,
                        &ep.sni,
                        ep.allow_insecure,
                        Duration::from_secs(12),
                    )
                    .await
                    {
                        Ok(r) => self.logger.info(format!(
                            "test: TLS SNI '{}' ok (cert sha256 {})",
                            r.sni,
                            &r.cert_sha256[..r.cert_sha256.len().min(16)]
                        )),
                        Err(e) => self.logger.error(format!("test: {e:#}")),
                    }
                }
                return;
            }
        };

        if is_tun {
            match tun::verify_through_tun(tun::VERIFY_URL, Duration::from_secs(20)).await {
                Ok(r) if r.status == 200 => {
                    self.emit(
                        Phase::Forwarding,
                        format!("test passed through the TUN ({} ms)", r.ms),
                        r.egress_ip,
                    );
                }
                Ok(r) => self
                    .logger
                    .error(format!("test: unexpected HTTP {}", r.status)),
                Err(e) => self.logger.error(format!("test failed: {e:#}")),
            }
            return;
        }

        let http_port = http_port.unwrap_or_default();
        match netcheck::proxied_get(http_port, TEST_URL, Duration::from_secs(20)).await {
            Ok(r) if r.status == 200 => {
                let ip = if r.body.is_empty() {
                    None
                } else {
                    Some(r.body.clone())
                };
                self.emit(Phase::Forwarding, format!("test passed ({} ms)", r.ms), ip);
            }
            Ok(r) => self
                .logger
                .error(format!("test: unexpected HTTP {}", r.status)),
            Err(e) => self.logger.error(format!("test failed: {e:#}")),
        }
    }

    async fn refresh_ip(&mut self) {
        let (is_tun, http_port) = match self.live.as_ref() {
            Some(Live::Proxy(s)) => (false, Some(s.http_port)),
            Some(Live::Tun(_)) => (true, None),
            None => return,
        };
        if is_tun {
            match tun::verify_through_tun(tun::VERIFY_URL, Duration::from_secs(15)).await {
                Ok(r) => match r.egress_ip {
                    Some(ip) => self.logger.info(format!("external IP: {ip}")),
                    None => self.logger.info("external IP not reported by the remote"),
                },
                Err(e) => self
                    .logger
                    .error(format!("external IP lookup failed: {e:#}")),
            }
            return;
        }
        let http_port = http_port.unwrap_or_default();
        match netcheck::external_ip(http_port, Duration::from_secs(15)).await {
            Ok(ip) => self.logger.info(format!("external IP: {ip}")),
            Err(e) => self
                .logger
                .error(format!("external IP lookup failed: {e:#}")),
        }
    }

    async fn on_tick(&mut self) {
        #[cfg(target_os = "linux")]
        if self.linux_tunnel.is_some() {
            let mut messages = Vec::new();
            let tunnel = self.linux_tunnel.as_mut().unwrap();
            while let Ok(line) = tunnel.lines.try_recv() {
                messages.push(line);
            }
            let code = tunnel.exit_code();
            for line in messages {
                self.logger.info(line.clone());
                if line.starts_with("HEALTH: Kill switch")
                    || line.starts_with("HEALTH: IPv4/IPv6 output guard")
                {
                    let _ = self.events.send(EngineEvent::Protection(line.clone()));
                }
                if let Some(phase) = crate::linux_tunnel::LinuxTunnel::phase(&line) {
                    if phase.forwarding() {
                        self.linux_tunnel.as_mut().unwrap().mark_connected();
                    }
                    let ip = line.rsplit_once("exit ").map(|(_, ip)| ip.to_string());
                    self.emit(phase, line, ip);
                }
            }
            let stats = self.linux_tunnel.as_mut().unwrap().stats();
            let _ = self.events.send(EngineEvent::Stats(stats));
            if let Some(code) = code {
                let retain = code != 0 && self.cfg.lock().unwrap().settings.kill_switch;
                self.teardown_with(retain).await;
                self.emit(
                    if code == 0 {
                        Phase::Disconnected
                    } else {
                        Phase::Error
                    },
                    format!("Linux helper exited (code {code}); see connection events"),
                    None,
                );
            }
            return;
        }
        // Pending auto-reconnect?
        if let Some(at) = self.reconnect_at {
            if Instant::now() >= at {
                #[cfg(windows)]
                if !crate::network_events::physical_available() {
                    if !self.network_waiting {
                        self.emit(
                            Phase::Degraded,
                            "Physical network unavailable; recovery paused until it returns".into(),
                            None,
                        );
                    }
                    self.network_waiting = true;
                    self.reconnect_at = Some(Instant::now() + Duration::from_secs(5));
                    return;
                }
                self.network_waiting = false;
                self.reconnect_at = None;
                self.emit(Phase::Preparing, "auto-reconnecting".into(), None);
                self.connect().await;
                return;
            }
        }

        let Some(live) = self.live.as_mut() else {
            return;
        };

        // Stats: the local proxy counts the bytes in system-proxy mode; in TUN
        // mode the adapter itself is the only path for tunnelled traffic, so
        // its counters and its connection table are the real numbers.
        let snap = match live {
            Live::Proxy(s) => s.traffic.snapshot(),
            Live::Tun(t) => {
                let (rx, tx) = tun::adapter_traffic(&t.device).unwrap_or((0, 0));
                let (base_rx, base_tx) = self.tun_counter_baseline.unwrap_or((0, 0));
                let mut s = crate::stats::Traffic::from_counters(
                    rx.saturating_sub(base_rx),
                    tx.saturating_sub(base_tx),
                );
                // The operating system does not know the connections of a TUN
                // tunnel - the core takes the packets off the adapter itself -
                // so the count comes from the core's own log.
                s.active_conns = t.conns.active() as i64;
                s
            }
        };
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64().max(0.001);
        let up_speed = ((snap.up.saturating_sub(self.last.up)) as f64 / dt) as u64;
        let down_speed = ((snap.down.saturating_sub(self.last.down)) as f64 / dt) as u64;
        self.last = snap;
        self.last_tick = now;
        let _ = self.events.send(EngineEvent::Stats(StatsEvent {
            up: snap.up,
            down: snap.down,
            up_speed,
            down_speed,
            active: snap.active_conns,
            total: snap.total_conns,
            uptime_secs: self
                .connected_at
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0),
        }));

        // Core death detection.
        if let Some(code) = live.core_exit_code() {
            self.logger
                .error(format!("Xray core exited unexpectedly (code {code})"));
            let auto = self.cfg.lock().unwrap().settings.auto_reconnect;
            self.emit(Phase::Error, format!("core exited (code {code})"), None);
            let kill = matches!(self.live, Some(Live::Tun(_)))
                && self.cfg.lock().unwrap().settings.kill_switch;
            self.teardown_with(kill).await;
            if auto {
                self.schedule_reconnect_inner();
            }
            return;
        }
        if self
            .connected_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(120))
            && self.health_failures == 0
        {
            self.retries = 0;
        }
        let changed = crate::network_events::take_changed() || dt > 5.0;
        self.check_health(changed && self.last_health.elapsed() > Duration::from_secs(2))
            .await;
    }

    /// Poll a detached probe so Disconnect and stats remain responsive.
    async fn check_health(&mut self, force: bool) {
        if self
            .health_task
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            let result = self.health_task.take().unwrap().await;
            let healthy = matches!(result, Ok(Ok(_)));
            let exit = result
                .as_ref()
                .ok()
                .and_then(|r| r.as_ref().ok())
                .cloned()
                .flatten();
            if healthy {
                if self.trusted_exit.is_none() {
                    self.trusted_exit = exit.clone();
                }
                {
                    self.full_tunnel = matches!(self.live, Some(Live::Tun(_)));
                    self.emit(
                        if self.full_tunnel {
                            Phase::FullForwarding
                        } else {
                            Phase::Forwarding
                        },
                        "Traffic verification passed; system/proxy comparison required for full TUN status".into(),
                        exit,
                    );
                }
                self.health_failures = 0;
            } else {
                self.logger
                    .warn(format!("Connection health check failed: {result:?}"));
                self.health_failures += 1;
                self.full_tunnel = false;
                self.emit(
                    Phase::Degraded,
                    format!(
                        "Tunnel traffic check failed ({}/2): {result:?}",
                        self.health_failures
                    ),
                    None,
                );
                if self.health_failures >= 2 {
                    let settings = self.cfg.lock().unwrap().settings.clone();
                    let kill = matches!(self.live, Some(Live::Tun(_))) && settings.kill_switch;
                    self.teardown_with(kill).await;
                    self.schedule_reconnect(&settings);
                    return;
                }
            }
            self.last_health = Instant::now();
        }
        let health = self.cfg.lock().unwrap().settings.health.clone();
        if !health.enabled && !force {
            return;
        }
        let interval = if self.health_failures > 0 {
            5
        } else {
            health.interval()
        };
        if self.health_task.is_none()
            && (force || self.last_health.elapsed() >= Duration::from_secs(interval))
        {
            let port = match &self.live {
                Some(Live::Proxy(s)) => Some(s.http_port),
                Some(Live::Tun(_)) => None,
                None => return,
            };
            let tun_details = match &self.live {
                Some(Live::Tun(t)) => Some((t.health_port, t.server_addrs.clone())),
                _ => None,
            };
            let expected_exit = self.trusted_exit.clone();
            let logger = self.logger.clone();
            self.emit(
                Phase::Verifying,
                "Full-Tunnel verification started".into(),
                None,
            );
            self.health_task = Some(tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(health.timeout()), async move {
                    if let Some((health_port, addresses)) = tun_details {
                        let began = Instant::now();
                        let verdict = crate::diag::verify_full_tunnel(
                            health_port,
                            &addresses,
                            None,
                            None,
                            Duration::from_secs(health.timeout()),
                        )
                        .await;
                        logger.info(format!("Full-Tunnel check finished in {} ms; adapter traffic includes concurrent applications:\n{}", began.elapsed().as_millis(), verdict.report()));
                        anyhow::ensure!(verdict.ok(), "{}", verdict.report());
                        if let Some(expected) = expected_exit {
                            anyhow::ensure!(
                                verdict.ordinary_exit.as_deref() == Some(expected.as_str()),
                                "Verified exit changed from the session baseline"
                            );
                        }
                        Ok(verdict.ordinary_exit)
                    } else {
                        probe_live_traffic(port).await.map(|_| None)
                    }
                })
                .await
                .map_err(|_| anyhow::anyhow!("Health verification timed out"))?
            }));
        }
    }

    async fn teardown(&mut self) {
        self.teardown_with(false).await;
    }

    /// Tear the tunnel down. `kill_switch` keeps the tunnel routes in place so
    /// traffic cannot escape while the tunnel is down.
    async fn teardown_with(&mut self, kill_switch: bool) {
        if !kill_switch {
            crate::kill_switch::clear();
        }
        #[cfg(target_os = "linux")]
        if let Some(tunnel) = self.linux_tunnel.take() {
            if let Err(error) = tunnel.stop().await {
                self.logger.warn(format!("{error:#}"));
                self.linux_recovery_needed = true;
                let _ = self.events.send(EngineEvent::Protection(
                    "Cleanup unverified; use Disconnect / release protection".into(),
                ));
            }
        }
        #[cfg(target_os = "linux")]
        if !kill_switch && self.linux_recovery_needed {
            match crate::linux_tunnel::LinuxTunnel::recover().await {
                Ok(()) => {
                    self.linux_recovery_needed = false;
                    let _ = self.events.send(EngineEvent::Protection(
                        "Released by emergency recovery".into(),
                    ));
                }
                Err(error) => {
                    self.logger.error(format!("{error:#}"));
                    let _ = self.events.send(EngineEvent::Protection(
                        "Recovery failed; administrator recovery required".into(),
                    ));
                }
            }
        }
        #[cfg(target_os = "linux")]
        let released = !kill_switch && !self.linux_recovery_needed;
        #[cfg(not(target_os = "linux"))]
        let released = !kill_switch;
        if released {
            let _ = self.events.send(EngineEvent::Protection("Released".into()));
        }
        if let Some(task) = self.health_task.take() {
            task.abort();
        }
        self.health_failures = 0;
        self.full_tunnel = false;
        if let Some(snap) = self.proxy_snap.take() {
            match sysproxy::restore(&snap) {
                Ok(()) => self.logger.info("System proxy settings restored"),
                Err(e) => self
                    .logger
                    .error(format!("failed to restore system proxy: {e:#}")),
            }
        }
        if let Some(live) = self.live.take() {
            match live {
                Live::Proxy(s) => s.stop(&self.logger).await,
                Live::Tun(mut t) => {
                    // Keep the counter baseline so the speeds do not jump after
                    // a reconnect (the adapter counters are cumulative).
                    if let Some((rx, tx)) = tun::adapter_traffic(&t.device) {
                        self.last = crate::stats::Traffic::from_counters(rx, tx);
                    }
                    t.stop(&self.logger, kill_switch).await
                }
            }
        }
        self.connected_at = None;
        self.tun_counter_baseline = None;
        self.last = TrafficSnapshot::default();
        self.last_tick = Instant::now();
        let _ = self.events.send(EngineEvent::Stats(StatsEvent::default()));
    }
}

/// Turn a TUN setup error into a message that names the failing stage.
fn describe_tun_error(err: &anyhow::Error) -> String {
    // The error chain carries our `TunSetupError` (stage + specific reason).
    for cause in err.chain() {
        if let Some(te) = cause.downcast_ref::<tun::TunSetupError>() {
            return format!("{}: {}", te.stage.label(), te.failure.message());
        }
    }
    format!("{err:#}")
}

/// Two small HTTPS probes; a single endpoint outage must not trigger reconnect.
async fn probe_live_traffic(proxy_port: Option<u16>) -> anyhow::Result<()> {
    for url in [tun::VERIFY_URL, "https://www.cloudflare.com/cdn-cgi/trace"] {
        let valid = if let Some(port) = proxy_port {
            match netcheck::proxied_get(port, url, Duration::from_secs(6)).await {
                Ok(r) => r.status == 200 && tun::parse_trace_ip(&r.body).is_some(),
                Err(_) => false,
            }
        } else {
            match tun::verify_through_tun(url, Duration::from_secs(6)).await {
                Ok(r) => r.status == 200 && r.egress_ip.is_some(),
                Err(_) => false,
            }
        };
        if valid {
            return Ok(());
        }
    }
    anyhow::bail!("both tunnel traffic probes failed")
}

#[cfg(test)]
mod health_tests {
    use super::*;
    fn core(auto: bool) -> Core {
        let mut config = AppConfig::default();
        config.settings.auto_reconnect = auto;
        let (_, rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(16);
        Core {
            cfg: Arc::new(Mutex::new(config)),
            logger: Logger::new(),
            rx,
            events,
            live: None,
            proxy_snap: None,
            connected_at: None,
            last: TrafficSnapshot::default(),
            last_tick: Instant::now(),
            retries: 0,
            network_waiting: false,
            reconnect_at: None,
            recovery_endpoint: None,
            recovery_server_id: None,
            #[cfg(target_os = "linux")]
            linux_tunnel: None,
            #[cfg(target_os = "linux")]
            linux_recovery_needed: false,
            full_tunnel: true,
            trusted_exit: None,
            last_health: Instant::now(),
            health_failures: 0,
            health_task: None,
            tun_counter_baseline: None,
        }
    }
    #[tokio::test]
    async fn backoff_is_bounded_and_manual_disconnect_cancels_it() {
        let mut c = core(true);
        let mut delays = Vec::new();
        for _ in 0..6 {
            let before = Instant::now();
            c.schedule_reconnect_inner();
            if let Some(at) = c.reconnect_at {
                delays.push(at.duration_since(before).as_secs());
            }
        }
        assert_eq!(c.retries, 5);
        assert_eq!(&delays[..5], &[1, 2, 4, 8, 15]);
        c.recovery_endpoint = Some(Endpoint::default());
        c.handle(Command::Disconnect).await;
        assert!(c.reconnect_at.is_none());
        assert!(c.recovery_endpoint.is_none());
    }

    async fn complete_probe(core: &mut Core, healthy: bool) {
        core.health_task = Some(tokio::spawn(async move {
            if healthy {
                Ok(None)
            } else {
                anyhow::bail!("test outage")
            }
        }));
        while !core.health_task.as_ref().unwrap().is_finished() {
            tokio::task::yield_now().await;
        }
        core.check_health(false).await;
    }
    #[tokio::test]
    async fn transient_outage_invalidates_full_status_but_does_not_reconnect() {
        let mut c = core(true);
        let mut events = c.events.subscribe();
        complete_probe(&mut c, false).await;
        assert!(!c.full_tunnel);
        assert_eq!(c.health_failures, 1);
        assert!(c.reconnect_at.is_none());
        assert!(matches!(
            events.try_recv(),
            Ok(EngineEvent::Status(Status {
                phase: Phase::Degraded,
                ..
            }))
        ));
        complete_probe(&mut c, true).await;
        assert_eq!(c.health_failures, 0);
        assert!(c.reconnect_at.is_none());
        assert!(matches!(
            events.try_recv(),
            Ok(EngineEvent::Status(Status {
                phase: Phase::Forwarding,
                ..
            }))
        ));
    }
    #[tokio::test]
    async fn repeated_outage_respects_reconnect_preference() {
        for auto in [false, true] {
            let mut c = core(auto);
            complete_probe(&mut c, false).await;
            complete_probe(&mut c, false).await;
            assert_eq!(c.reconnect_at.is_some(), auto);
        }
    }
    #[tokio::test]
    async fn disconnect_cancels_pending_probe_and_clears_full_status() {
        let mut c = core(false);
        c.health_task = Some(tokio::spawn(std::future::pending()));
        let handle = c.health_task.as_ref().unwrap().abort_handle();
        c.teardown().await;
        tokio::task::yield_now().await;
        assert!(handle.is_finished());
        assert!(!c.full_tunnel);
        assert!(c.health_task.is_none());
    }
}
