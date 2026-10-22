//! SNI Tunnel — the command line front end.
//!
//! The implementation lives in the library crate (`sni_tunnel`); this binary
//! only parses the arguments and dispatches.

use anyhow::{bail, Context, Result};
use sni_tunnel::engine::{Command, EngineEvent};
#[cfg(feature = "gui")]
use sni_tunnel::gui;
use sni_tunnel::logging::Logger;
use sni_tunnel::model::{resolve_endpoint, AppConfig, ServerProfile, Settings, SniProfile};
use sni_tunnel::session::Session;
use sni_tunnel::util;
use sni_tunnel::{
    autostart, console, diag, engine, logging, model, netcheck, store, storecli, sysproxy, tun,
    vmess,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn usage() {
    println!(
        r#"SNI Tunnel - VMess with a custom TLS SNI (system proxy or TUN mode)

USAGE:
  sni-tunnel linux-config --vmess <file> [options]
      Emit a legacy-core configuration for the bundled Linux launcher.
      Contains credentials; do not share this output.
  sni-tunnel selftest [--http-port N] [--socks-port N]
      Validate the Xray core + local proxy plumbing using a direct outbound.
      Requires no VMess server.

  sni-tunnel check   --vmess <link|file|json> [options]
      Establish the tunnel, run diagnostics, verify real proxied traffic, exit.

  sni-tunnel connect --vmess <link|file|json> [options]
      Like `check`, but stays connected until Ctrl+C.

  sni-tunnel proxy --vmess <link|file|json> [options]
      Establish the tunnel AND set the Windows system proxy; restores the
      previous proxy settings on Ctrl+C. (Stage 3)

  sni-tunnel proxy-off
      Restore/repair the system proxy from a previous run (crash recovery).

  sni-tunnel tun-selftest [--test-ip <ip>]
      (Administrator) Bring up the wintun TUN datapath with a DIRECT outbound
      and route only <ip> (default 1.1.1.1) through it, then prove packets
      traverse the tunnel. Safe: the default route is never touched.

  sni-tunnel gui
      Open the desktop UI (dashboard, servers, SNI editor, logs, settings).
      Uses the stored/selected profiles, or an imported one via --vmess.
      Runs in the system tray; closing the window hides it to the tray.

  sni-tunnel autostart [on|off|status]
      Manage "start with Windows" (per-user Run key; no admin required).

  sni-tunnel            (no arguments)  -> launches the GUI.

PROFILE MANAGEMENT (stage 2):  run `sni-tunnel list` to see saved servers.
  list | profiles | add | import | remove | use | sni <...> | export
  (see `sni-tunnel add` usage via the PROFILE COMMANDS section below)

"#
    );
    print!("{}", storecli::help());
    println!(
        r#"
OPTIONS:
  --sni <host>          Override the TLS SNI / server name
  --ws-host <host>      Override the WebSocket Host header
  --ws-path <path>      Override the WebSocket path
  --connect-addr <host> Override the dial address (keeps VMess credentials)
  --alpn <a,b>          Override ALPN list
  --allow-insecure      Do NOT verify the TLS certificate (unsafe)
  --mux                 Enable multiplexing (MUX) for this run
  --no-mux              Disable multiplexing (MUX) for this run
  --mux-concurrency <N> How many connections share one transport connection
  --mux-xudp            Carry UDP over the multiplexed connection (XUDP)
  --no-mux-xudp         Do not carry UDP over the multiplexed connection
  --mux-xudp-concurrency <N>  How many UDP streams share the connection
  --mux-udp443 <policy> What to do with UDP to port 443: reject|allow|skip
  --mode system|tun     Routing mode (default system; tun = whole-device TUN,
                        requires Administrator)
  --auto-reconnect      Retry automatically if the core dies / no traffic
  --http-port <N>       Local HTTP proxy port (default 2080)
  --socks-port <N>      Local SOCKS5 proxy port (default 1080)
  --test-url <url>      URL used to verify proxied traffic (default ipify)
  --test-ip <ip>        TUN self-test target IP to route (default 1.1.1.1)
  --no-verify-hash      Skip SHA-256 verification of the core download
"#
    );
}

struct Opts {
    vmess: Option<String>,
    sni: Option<String>,
    ws_host: Option<String>,
    ws_path: Option<String>,
    connect_addr: Option<String>,
    alpn: Vec<String>,
    allow_insecure: bool,
    mode: Option<String>,
    auto_reconnect: bool,
    http_port: u16,
    socks_port: u16,
    test_url: String,
    test_ip: String,
    tun_full: bool,
    /// A configuration exported by another client, to compare against.
    compare: Option<String>,
    /// A compatibility preset to apply to the selected profile.
    preset: Option<String>,
    verify_hash: bool,
    /// MUX: `Some(true)` = on, `Some(false)` = off, `None` = keep the profile.
    mux: Option<bool>,
    mux_concurrency: Option<i32>,
    mux_xudp: Option<bool>,
    mux_xudp_concurrency: Option<i32>,
    mux_udp443: Option<String>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            vmess: None,
            sni: None,
            ws_host: None,
            ws_path: None,
            connect_addr: None,
            alpn: Vec::new(),
            allow_insecure: false,
            mode: None,
            auto_reconnect: false,
            http_port: model::DEFAULT_HTTP_PORT,
            socks_port: model::DEFAULT_SOCKS_PORT,
            test_url: "https://api.ipify.org".into(),
            test_ip: "1.1.1.1".into(),
            tun_full: false,
            compare: None,
            preset: None,
            verify_hash: true,
            mux: None,
            mux_concurrency: None,
            mux_xudp: None,
            mux_xudp_concurrency: None,
            mux_udp443: None,
        }
    }
}

fn val(args: &[String], i: &mut usize) -> Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing value for argument"))
}

fn parse_args(args: &[String]) -> Result<(String, Opts)> {
    let mut cmd = String::new();
    let mut o = Opts::default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        match a.as_str() {
            "linux-config" | "selftest" | "check" | "connect" | "proxy" | "proxy-off" | "gui"
            | "tun-selftest" | "verify-full" | "diagnose" | "preset" => cmd = a.clone(),
            "--help" | "-h" => cmd = "help".into(),
            "--vmess" => o.vmess = Some(val(args, &mut i)?),
            "--sni" => o.sni = Some(val(args, &mut i)?),
            "--ws-host" => o.ws_host = Some(val(args, &mut i)?),
            "--ws-path" => o.ws_path = Some(val(args, &mut i)?),
            "--connect-addr" => o.connect_addr = Some(val(args, &mut i)?),
            "--alpn" => {
                o.alpn = val(args, &mut i)?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--allow-insecure" => o.allow_insecure = true,
            "--mux" => o.mux = Some(true),
            "--no-mux" => o.mux = Some(false),
            "--mux-concurrency" => o.mux_concurrency = Some(val(args, &mut i)?.parse()?),
            "--mux-xudp" => o.mux_xudp = Some(true),
            "--no-mux-xudp" => o.mux_xudp = Some(false),
            "--mux-xudp-concurrency" => o.mux_xudp_concurrency = Some(val(args, &mut i)?.parse()?),
            "--mux-udp443" => o.mux_udp443 = Some(val(args, &mut i)?),
            "--mode" => o.mode = Some(val(args, &mut i)?),
            "--auto-reconnect" => o.auto_reconnect = true,
            "--http-port" => o.http_port = val(args, &mut i)?.parse()?,
            "--socks-port" => o.socks_port = val(args, &mut i)?.parse()?,
            "--test-url" => o.test_url = val(args, &mut i)?,
            "--test-ip" => o.test_ip = val(args, &mut i)?,
            "--full" => o.tun_full = true,
            "--compare" => o.compare = Some(val(args, &mut i)?),
            "--preset" => o.preset = Some(val(args, &mut i)?),
            "--no-verify-hash" => o.verify_hash = false,
            other => {
                // "sni-tunnel preset nekoray" - the preset name is positional.
                if cmd == "preset" && o.preset.is_none() {
                    o.preset = Some(other.to_string());
                } else {
                    bail!("unknown argument: {other}");
                }
            }
        }
        i += 1;
    }
    if cmd.is_empty() {
        cmd = "help".into();
    }
    Ok((cmd, o))
}

fn load_profile(source: &str) -> Result<ServerProfile> {
    let path = std::path::Path::new(source);
    let text = if path.is_file() {
        std::fs::read_to_string(path)?
    } else {
        source.to_string()
    };
    let mut list = vmess::parse_any(&text)?;
    if list.len() > 1 {
        println!("note: {} profiles found, using the first", list.len());
    }
    Ok(list.remove(0))
}

fn apply_overrides(
    server: &ServerProfile,
    o: &Opts,
) -> Result<(ServerProfile, Option<SniProfile>)> {
    let sni_profile = if o.sni.is_some()
        || o.ws_host.is_some()
        || o.ws_path.is_some()
        || o.connect_addr.is_some()
        || !o.alpn.is_empty()
        || o.allow_insecure
    {
        Some(SniProfile {
            name: "cli-override".into(),
            server_id: server.id.clone(),
            tls: server.tls || o.sni.is_some(),
            sni: o.sni.clone().unwrap_or_default(),
            connect_address: o.connect_addr.clone().unwrap_or_default(),
            ws_host: o.ws_host.clone().unwrap_or_default(),
            ws_path: o.ws_path.clone().unwrap_or_default(),
            alpn: o.alpn.clone(),
            allow_insecure: o.allow_insecure,
            ..Default::default()
        })
    } else {
        None
    };
    let mut server = server.clone();
    // The MUX settings are applied to the profile itself, so the generated
    // configuration carries them.
    if let Some(on) = o.mux {
        server.mux_enabled = on;
    }
    if let Some(c) = o.mux_concurrency {
        server.mux_concurrency = c;
    }
    if let Some(on) = o.mux_xudp {
        server.mux_xudp_enabled = on;
    }
    if let Some(c) = o.mux_xudp_concurrency {
        server.mux_xudp_concurrency = c;
    }
    if let Some(p) = &o.mux_udp443 {
        let p = p.trim().to_ascii_lowercase();
        if !model::UDP443_POLICIES.contains(&p.as_str()) {
            anyhow::bail!(
                "unknown UDP/443 policy '{p}' (supported: {})",
                model::UDP443_POLICIES.join(", ")
            );
        }
        server.mux_xudp_udp443 = p;
    }
    Ok((server, sni_profile))
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // With no arguments (double-click / autostart) we open the GUI. In that
    // case, hide the console window if we are its only owner.
    let wants_gui = args
        .first()
        .map(|a| a.eq_ignore_ascii_case("gui"))
        .unwrap_or(cfg!(feature = "gui"));
    if wants_gui {
        console::hide_if_sole_owner();
    }
    if args.is_empty() {
        args.push("gui".into());
    }
    let code = match run(args).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

async fn run(args: Vec<String>) -> Result<i32> {
    if args.first().is_some_and(|a| a == "migrate") {
        let source = args
            .get(1)
            .context("usage: sni-tunnel migrate SOURCE_DIRECTORY")?;
        let report = sni_tunnel::migration::migrate(std::path::Path::new(source))?;
        println!(
            "Migration verified: {} profiles. Backup: {}",
            report.profiles,
            report.backup.display()
        );
        return Ok(0);
    }

    let logger = Logger::new();
    logging::install_global(&logger);

    // Crash recovery, on *every* start: a previous run may have left TUN
    // routes or the adapter address behind. A stale address makes the next
    // Xray start fail ("initialization has already been completed"), so it
    // has to be cleared before anything else happens.
    tun::clean_stale_routes(&logger).await;
    // A crashed run may have left the DNS of the physical adapters pointing
    // into the tunnel; put it back.
    tun::restore_dns_from_journal(&logger);

    let first = args.first().cloned().unwrap_or_default();
    if storecli::is_store_command(&first) {
        return storecli::run(&args, &logger);
    }
    if first == "autostart" {
        return autostart_cmd(&args);
    }
    let (cmd, o) = parse_args(&args)?;
    if cmd == "help" {
        usage();
        return Ok(0);
    }

    match cmd.as_str() {
        "linux-config" => {
            let cfg = build_app_config(&o)?;
            let mut server = cfg
                .selected_server_profile()
                .context("no selected server")?
                .clone();
            let (overridden, cli_sni) = apply_overrides(&server, &o)?;
            server = overridden;
            let ep = resolve_endpoint(&server, cli_sni.as_ref().or(cfg.selected_sni()));
            let doc = sni_tunnel::compat::config(&ep, o.socks_port, "", "warning")?;
            println!("{}", serde_json::to_string(&doc)?);
            Ok(0)
        }
        "selftest" => selftest(&o, &logger).await,
        "tun-selftest" => tun_selftest(&o, &logger).await,
        "verify-full" => verify_full_tunnel(&o, &logger).await,
        "diagnose" => diagnose_cmd(&o, &logger).await,
        "preset" => preset_cmd(&o),
        "check" | "connect" => vmess_cmd(&cmd, &o, &logger).await,
        "proxy" => proxy_cmd(&o, &logger).await,
        #[cfg(feature = "gui")]
        "gui" => gui_cmd(&o, &logger),
        #[cfg(not(feature = "gui"))]
        "gui" => {
            println!("this build has no desktop GUI (built without the 'gui' feature)");
            Ok(2)
        }
        "proxy-off" => {
            match sysproxy::recover_if_needed() {
                Ok(true) => println!("system proxy settings restored from backup"),
                Ok(false) => println!("nothing to restore"),
                Err(e) => eprintln!("restore failed: {e:#}"),
            }
            Ok(0)
        }
        _ => {
            usage();
            Ok(1)
        }
    }
}

fn has_overrides(o: &Opts) -> bool {
    o.sni.is_some()
        || o.ws_host.is_some()
        || o.ws_path.is_some()
        || o.connect_addr.is_some()
        || !o.alpn.is_empty()
        || o.allow_insecure
}

fn build_app_config(o: &Opts) -> Result<AppConfig> {
    match o.vmess.as_deref() {
        Some(source) => {
            let server = load_profile(source)?;
            let (server, sni) = apply_overrides(&server, o)?;
            let mut cfg = AppConfig::default();
            cfg.settings.http_port = o.http_port;
            cfg.settings.socks_port = o.socks_port;
            cfg.settings.mode = o.mode.clone().unwrap_or_else(|| cfg.settings.mode.clone());
            cfg.settings.auto_reconnect = o.auto_reconnect;
            cfg.selected_server = Some(server.id.clone());
            if let Some(mut sp) = sni {
                sp.id = uuid::Uuid::new_v4().to_string();
                cfg.selected_sni_profile = Some(sp.id.clone());
                cfg.sni_profiles.push(sp);
            }
            cfg.servers.push(server);
            Ok(cfg)
        }
        None => {
            let mut cfg = store::load();
            if cfg.selected_server_profile().is_none() {
                bail!("no server selected; add one with `add` + `use`, or pass --vmess");
            }
            cfg.settings.mode = o.mode.clone().unwrap_or_else(|| cfg.settings.mode.clone());
            if o.auto_reconnect {
                cfg.settings.auto_reconnect = true;
            }
            if has_overrides(o) {
                let server = cfg.selected_server_profile().unwrap().clone();
                let (server, sni) = apply_overrides(&server, o)?;
                if let Some(mut sp) = sni {
                    sp.id = uuid::Uuid::new_v4().to_string();
                    cfg.selected_sni_profile = Some(sp.id.clone());
                    cfg.sni_profiles.push(sp);
                }
                if let Some(idx) = cfg.servers.iter().position(|s| s.id == server.id) {
                    cfg.servers[idx] = server;
                }
            }
            Ok(cfg)
        }
    }
}

async fn proxy_cmd(o: &Opts, logger: &Logger) -> Result<i32> {
    let _instance = util::lock_instance()?;
    let cfg = build_app_config(o)?;
    if !matches!(cfg.settings.mode.as_str(), "system" | "none" | "tun") {
        eprintln!(
            "unknown mode '{}' (expected system, none or tun).",
            cfg.settings.mode
        );
        return Ok(2);
    }
    if cfg.settings.mode == "tun" {
        println!("mode: tun (whole-device tunnel, Administrator required; the system proxy is left untouched)");
    }
    logger.set_secrets(cfg.servers.iter().map(|s| s.uuid.clone()).collect());
    // The log mode comes from the settings: simple shows the connection
    // events, advanced shows everything.
    if let Some(mode) = logging::LogMode::parse(&cfg.settings.log_mode) {
        logger.set_mode(mode);
    }

    let cfg = Arc::new(Mutex::new(cfg));
    let engine = engine::spawn(cfg, logger.clone());

    let mut rx = engine.subscribe();
    let printer = tokio::spawn(async move {
        let mut ticks = 0u32;
        while let Ok(ev) = rx.recv().await {
            match ev {
                EngineEvent::Status(s) => {
                    println!("[{}] {}", s.phase.label(), s.detail);
                    if let Some(ip) = s.external_ip {
                        println!("            external IP: {ip}");
                    }
                }
                EngineEvent::Stats(t) => {
                    ticks += 1;
                    if ticks.is_multiple_of(5) {
                        println!(
                            "            up {}  down {}  active {}  uptime {}s",
                            util::format_rate(t.up_speed),
                            util::format_rate(t.down_speed),
                            t.active,
                            t.uptime_secs
                        );
                    }
                }
            }
        }
    });

    engine.send(Command::Connect);
    println!("press Ctrl+C to disconnect and restore the system proxy");
    tokio::signal::ctrl_c().await.ok();
    engine
        .shutdown_and_wait_async(Duration::from_secs(15))
        .await;
    printer.abort();
    println!("done");
    Ok(0)
}

fn autostart_cmd(args: &[String]) -> Result<i32> {
    match args.get(1).map(|s| s.as_str()) {
        None | Some("status") => {
            println!(
                "start with Windows: {}",
                if autostart::is_enabled() {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            Ok(0)
        }
        Some("on") => {
            autostart::set_enabled(true)?;
            println!("autostart enabled");
            Ok(0)
        }
        Some("off") => {
            autostart::set_enabled(false)?;
            println!("autostart disabled");
            Ok(0)
        }
        Some(other) => bail!("unknown autostart argument '{other}' (use on | off | status)"),
    }
}

#[cfg(feature = "gui")]
fn gui_cmd(o: &Opts, logger: &Logger) -> Result<i32> {
    let _instance = util::lock_instance()?;
    let mut cfg = store::load_checked()?;
    if let Some(source) = o.vmess.as_deref() {
        let imported = load_profile(source)?;
        let (profile, _) = apply_overrides(&imported, o)?;
        sni_tunnel::profile_import::append(&mut cfg, vec![profile]);
        store::save(&cfg)?;
    } else if has_overrides(o) {
        bail!("GUI overrides require --vmess; edit the selected profile in the app instead");
    }
    logger.set_secrets(cfg.servers.iter().map(|s| s.uuid.clone()).collect());
    let cfg = Arc::new(Mutex::new(cfg));
    gui::run(cfg, logger.clone())
}

async fn selftest(o: &Opts, logger: &Logger) -> Result<i32> {
    println!("== self-test: Xray core + local proxy (direct outbound) ==");

    let mut s = Session::start_direct(o.http_port, o.socks_port, o.verify_hash, logger).await?;
    println!(
        "local proxy up: HTTP {} / SOCKS5 {}",
        s.http_port, s.socks_port
    );

    let ok = match netcheck::proxied_get(o.http_port, &o.test_url, Duration::from_secs(20)).await {
        Ok(r) => {
            println!(
                "proxied HTTP GET -> {} ({} ms): {}",
                r.status,
                r.ms,
                truncate(&r.body, 80)
            );
            r.status == 200
        }
        Err(e) => {
            println!("proxied HTTP GET failed: {e:#}");
            false
        }
    };

    if let Some(code) = s.core_exit_code() {
        println!("core exited early with code {code}");
    }
    s.stop(logger).await;
    println!(
        "{}",
        if ok {
            "SELF-TEST PASSED"
        } else {
            "SELF-TEST FAILED"
        }
    );
    Ok(if ok { 0 } else { 1 })
}

async fn tun_selftest(o: &Opts, logger: &Logger) -> Result<i32> {
    println!("== tun self-test: wintun datapath (direct outbound) ==");
    if !tun::is_elevated() {
        eprintln!("this test needs Administrator privileges (the wintun adapter and route changes require it).");
        return Ok(2);
    }
    if o.tun_full {
        println!("routing the WHOLE device through the TUN (two /1 routes; the original default route is kept)");
    } else {
        println!(
            "routing only {}/32 through the TUN (default route untouched)",
            o.test_ip
        );
    }

    let settings = Settings {
        log_level: "info".into(),
        ..Default::default()
    };
    let mode = if o.tun_full {
        tun::RouteMode::DefaultWithExclusions(Vec::new())
    } else {
        tun::RouteMode::Hosts(vec![o.test_ip.clone()])
    };
    let mut s = tun::TunSession::start_direct(&settings, mode, o.verify_hash, logger).await?;
    println!("{}", s.stage_report());

    let ok = match tun::verify_through_tun(tun::VERIFY_URL, Duration::from_secs(25)).await {
        Ok(r) => {
            println!("TUN HTTPS GET -> {} ({} ms)", r.status, r.ms);
            match &r.egress_ip {
                Some(ip) => println!("egress IP seen by the remote: {ip}"),
                None => println!("egress IP not reported"),
            }
            r.status == 200
        }
        Err(e) => {
            println!("TUN datapath failed: {e:#}");
            false
        }
    };

    if let Some(code) = s.core_exit_code() {
        println!("core exited early with code {code}");
    }
    s.stop(logger, false).await;
    println!(
        "{}",
        if ok {
            "TUN SELF-TEST PASSED"
        } else {
            "TUN SELF-TEST FAILED"
        }
    );
    Ok(if ok { 0 } else { 1 })
}

async fn vmess_cmd(cmd: &str, o: &Opts, logger: &Logger) -> Result<i32> {
    let stored = if o.vmess.is_none() {
        Some(store::load())
    } else {
        None
    };
    let server = match o.vmess.as_deref() {
        Some(src) => load_profile(src)?,
        None => stored
            .as_ref()
            .unwrap()
            .selected_server_profile()
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("no server selected; add one with `add` + `use`, or pass --vmess")
            })?,
    };
    logger.set_secrets(vec![server.uuid.clone()]);

    let (server, cli_sni) = apply_overrides(&server, o)?;
    let sni = cli_sni.or_else(|| stored.as_ref().and_then(|c| c.selected_sni().cloned()));
    let mut ep = resolve_endpoint(&server, sni.as_ref());

    // Xray 26 removed `allowInsecure`: pin the server certificate instead.
    let preparation = if cfg!(target_os = "linux") {
        sni_tunnel::compat::validate_endpoint(&ep)
    } else if stored
        .as_ref()
        .is_some_and(|c| c.settings.netmod_compatibility)
    {
        sni_tunnel::compat::validate_endpoint(&ep)
    } else {
        netcheck::ensure_cert_pin(&mut ep).await
    };
    if let Err(e) = preparation {
        eprintln!("error: {e:#}");
        return Ok(2);
    }

    let settings = match &stored {
        Some(c) => c.settings.clone(),
        None => Settings {
            http_port: o.http_port,
            socks_port: o.socks_port,
            log_level: "warning".into(),
            ..Default::default()
        },
    };
    // The CLI flag wins, otherwise honour the stored mode.
    let mode = o.mode.clone().unwrap_or_else(|| {
        stored
            .as_ref()
            .map(|c| c.settings.mode.clone())
            .unwrap_or_else(|| "system".into())
    });

    println!("== diagnostics ==");
    println!("server      : {}", ep.address);
    println!("port        : {}", ep.port);
    println!("network/tls : {} / {}", ep.network, ep.tls);
    println!(
        "SNI         : {}",
        if ep.sni.is_empty() { "(none)" } else { &ep.sni }
    );
    println!("Host header : {}", ep.ws_host);

    // 1. DNS
    match netcheck::resolve(&ep.address, ep.port).await {
        Ok(r) => println!("[ok]   DNS      {} -> {:?} ({} ms)", r.host, r.addrs, r.ms),
        Err(e) => println!("[fail] DNS      {e:#}"),
    }
    // 2. TCP
    match netcheck::tcp_connect(&ep.address, ep.port, Duration::from_secs(10)).await {
        Ok((a, ms)) => println!("[ok]   TCP      connected to {a} ({ms} ms)"),
        Err(e) => println!("[fail] TCP      {e:#}"),
    }
    // 3. TLS (only when TLS is enabled)
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
            Ok(r) => println!(
                "[ok]   TLS      SNI '{}' accepted, cert sha256 {} ({} ms)",
                r.sni,
                &r.cert_sha256[..16.min(r.cert_sha256.len())],
                r.ms
            ),
            Err(e) => println!("[fail] TLS      {e:#}"),
        }
    } else {
        println!("[skip] TLS      disabled for this profile");
    }

    // 4. Establish the real tunnel and verify forwarded traffic.
    if mode == "tun" {
        return tun_check(cmd, o, &ep, &settings, logger).await;
    }
    println!("== tunnel ==");
    let s = Session::start(ep, &settings, o.verify_hash, logger).await?;
    println!(
        "local proxy : HTTP 127.0.0.1:{} / SOCKS5 127.0.0.1:{}",
        s.http_port, s.socks_port
    );

    let verified = match netcheck::proxied_get(s.http_port, &o.test_url, Duration::from_secs(25))
        .await
    {
        Ok(r) => {
            println!(
                "[ok]   FORWARD  proxied GET -> HTTP {} ({} ms): {}",
                r.status,
                r.ms,
                truncate(&r.body, 100)
            );
            r.status == 200
        }
        Err(e) => {
            println!("[fail] FORWARD  {e:#}");
            println!(
                "       The VMess connection could not carry traffic. Check the server, UUID,"
            );
            println!("       transport settings and SNI. (A pure TCP/TLS success above does NOT");
            println!("       guarantee the VMess handshake succeeded.)");
            false
        }
    };

    if verified {
        if let Ok(ip) = netcheck::external_ip(s.http_port, Duration::from_secs(15)).await {
            println!("external IP : {ip}");
        }
    }

    if cmd == "connect" && verified {
        println!(
            "connected. local proxy ports {}/{} — press Ctrl+C to stop.",
            s.http_port, s.socks_port
        );
        tokio::signal::ctrl_c().await.ok();
        s.stop(logger).await;
        return Ok(0);
    }

    s.stop(logger).await;
    println!(
        "{}",
        if verified {
            "TUNNEL VERIFIED"
        } else {
            "TUNNEL NOT VERIFIED"
        }
    );
    Ok(if verified { 0 } else { 1 })
}

/// The full-tunnel diagnostic: proves that *ordinary* system traffic really
/// uses the tunnel (not just our own client).
async fn verify_full_tunnel(o: &Opts, logger: &Logger) -> Result<i32> {
    println!("== full tunnel verification ==");
    if !tun::is_elevated() {
        eprintln!("TUN mode needs Administrator privileges - run this from an elevated shell.");
        return Ok(2);
    }

    // 1. The public IP before connecting.
    let before = match netcheck::public_ip(Duration::from_secs(15)).await {
        Ok(ip) => {
            println!("[1] public IP before connecting : {ip}");
            ip
        }
        Err(e) => {
            println!("[1] public IP before connecting : unknown ({e:#})");
            String::new()
        }
    };

    // 2. Start the tunnel.
    let server = {
        let c = store::load();
        match c.selected_server_profile().cloned() {
            Some(s) => s,
            None => {
                eprintln!("no server selected");
                return Ok(2);
            }
        }
    };
    // The command line may override the profile (MUX, SNI, ...).
    let (server, _cli_sni) = apply_overrides(&server, o)?;
    let mut ep = resolve_endpoint(&server, None);
    let preparation = if store::load().settings.netmod_compatibility {
        sni_tunnel::compat::validate_endpoint(&ep)
    } else {
        netcheck::ensure_cert_pin(&mut ep).await
    };
    if let Err(e) = preparation {
        eprintln!("error: {e:#}");
        return Ok(2);
    }
    let settings = store::load().settings;
    let mut exclusions = Vec::new();
    if let Ok(r) = netcheck::resolve(&ep.address, ep.port).await {
        exclusions = r.addrs.iter().map(|a| a.to_string()).collect();
    }
    let mode = tun::RouteMode::DefaultWithExclusions(exclusions.clone());
    let mut s = tun::TunSession::start(ep, &settings, mode, o.verify_hash, logger).await?;
    println!("{}", s.stage_report());

    // 3. The default routes.
    let routes = tun::installed_routes().await.unwrap_or((0, 0));
    println!(
        "[3] system routes via the TUN   : {} IPv4, {} IPv6",
        routes.0, routes.1
    );

    // 4. The decisive checks: the system routes have to point into the TUN,
    //    the measured host has to be routed through it, the tunnel has to have
    //    carried the bytes and the exit addresses have to agree.
    let baseline = if before.is_empty() {
        None
    } else {
        Some(before.clone())
    };
    let verdict =
        diag::verify_full_tunnel(None, &exclusions, baseline, None, Duration::from_secs(20)).await;
    println!();
    println!("== full tunnel verification ==");
    println!("{}", verdict.report());
    for p in &verdict.problems {
        println!("  !! {p}");
    }

    // 5. DNS through the tunnel.
    match netcheck::system_dns_lookup("www.microsoft.com").await {
        Ok(addrs) => println!("[5] system DNS resolution      : ok ({addrs:?})"),
        Err(e) => println!("[5] system DNS resolution      : FAILED ({e:#})"),
    }

    // 6. IPv4 / IPv6 separately.
    let v4 = netcheck::public_ip_v4(Duration::from_secs(20)).await;
    let v6 = netcheck::public_ip_v6(Duration::from_secs(20)).await;
    println!("[6] IPv4 exit                   : {}", describe(&v4));
    println!("[6] IPv6 exit                   : {}", describe(&v6));
    println!("[7] open connections in the TUN : {}", s.conns.active());

    s.stop(logger, false).await;
    println!(
        "{}",
        if verdict.ok() {
            "FULL TUNNEL VERIFIED"
        } else {
            "FULL TUNNEL NOT VERIFIED"
        }
    );
    Ok(if verdict.ok() { 0 } else { 1 })
}

fn describe(r: &Result<String>) -> String {
    match r {
        Ok(ip) => ip.clone(),
        Err(e) => format!("not available ({e:#})"),
    }
}

/// Show the effective configuration (secrets redacted), the settings that
/// reach the wire, the traffic that bypasses the tunnel and - optionally - the
/// differences to a configuration exported by another client.
async fn diagnose_cmd(o: &Opts, _logger: &Logger) -> Result<i32> {
    let cfg = store::load();
    let (document, source) = sni_tunnel::effective_diag::configuration(&cfg)?;
    println!(
        "{}",
        sni_tunnel::effective_diag::report(&cfg, &document, &source).await
    );
    if let Some(path) = o.compare.as_deref() {
        let text = std::fs::read_to_string(path).context("read reference export")?;
        println!(
            "{}",
            sni_tunnel::effective_diag::compare_json(&document, &text)?
        );
    }
    Ok(0)
}

/// Apply a compatibility preset to the selected profile.
fn preset_cmd(o: &Opts) -> Result<i32> {
    let Some(preset) = o.preset.as_deref() else {
        println!("No verified client presets. Use diagnose --compare with an effective Xray JSON export.");
        for (name, desc) in diag::PRESETS {
            println!("  {name:<10} {desc}");
        }
        return Ok(0);
    };
    let mut cfg = store::load();
    let Some(server) = cfg.selected_server_profile().cloned() else {
        eprintln!("no server selected");
        return Ok(2);
    };
    let mut updated = server.clone();
    if !diag::apply_preset(&mut updated, preset) {
        eprintln!("unknown preset '{preset}'");
        for (name, desc) in diag::PRESETS {
            println!("  {name:<10} {desc}");
        }
        return Ok(2);
    }
    if let Some(slot) = cfg.servers.iter_mut().find(|s| s.id == updated.id) {
        *slot = updated;
    }
    store::save(&cfg)?;
    println!("preset '{preset}' applied to '{}'", server.name);
    Ok(0)
}

/// `check`/`connect` in TUN mode: bring up the wintun tunnel and prove that
/// real traffic is carried through it.
async fn tun_check(
    cmd: &str,
    o: &Opts,
    ep: &model::Endpoint,
    settings: &Settings,
    logger: &Logger,
) -> Result<i32> {
    println!("== tunnel (TUN mode) ==");
    if !tun::is_elevated() {
        eprintln!("TUN mode needs Administrator privileges - run this from an elevated shell.");
        return Ok(2);
    }

    // Resolve the server before the routes change so it can stay outside the
    // tunnel.
    let mut exclusions = Vec::new();
    match netcheck::resolve(&ep.address, ep.port).await {
        Ok(r) => {
            exclusions = r.addrs.iter().map(|a| a.to_string()).collect();
            println!("server IP   : {:?}", r.addrs);
        }
        Err(e) => println!("server DNS  : failed ({e:#}) - the server may be reached by IP only"),
    }

    let mode = tun::RouteMode::DefaultWithExclusions(exclusions);
    let mut s = tun::TunSession::start(ep.clone(), settings, mode, o.verify_hash, logger).await?;
    println!(
        "TUN adapter : {} ({}/{})",
        tun::DEVICE_NAME,
        tun::TUN_IP,
        tun::TUN_PREFIX
    );

    let verified = match tun::verify_through_tun(tun::VERIFY_URL, Duration::from_secs(25)).await {
        Ok(r) => {
            println!("[ok]   FORWARD  TUN GET -> HTTP {} ({} ms)", r.status, r.ms);
            match &r.egress_ip {
                Some(ip) => println!("external IP : {ip}"),
                None => println!("external IP : not reported"),
            }
            r.status == 200
        }
        Err(e) => {
            println!("[fail] FORWARD  {e:#}");
            println!(
                "       The VMess connection could not carry traffic. Check the server, UUID,"
            );
            println!("       transport settings and SNI.");
            false
        }
    };

    if cmd == "connect" && verified {
        println!("connected. TUN is live — press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await.ok();
        s.stop(logger, false).await;
        return Ok(0);
    }

    s.stop(logger, false).await;
    println!(
        "{}",
        if verified {
            "TUNNEL VERIFIED"
        } else {
            "TUNNEL NOT VERIFIED"
        }
    );
    Ok(if verified { 0 } else { 1 })
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > n {
        format!("{} …", s.chars().take(n).collect::<String>())
    } else {
        s
    }
}
