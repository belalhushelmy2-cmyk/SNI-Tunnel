//! Stage 4: Slint desktop front-end over the Stage-3 engine.
//!
//! Threading model: the UI (and winit's event loop) runs on the main thread.
//! The engine lives on its own multi-thread Tokio runtime in a background
//! thread; the UI drives it through `EngineHandle` commands and polls the
//! engine/log broadcast channels from a `Timer` on the UI thread.

use crate::editor::{duplicate_profile, EditorFields};
use crate::engine::{self, Command, EngineEvent, Phase};
use crate::logging::Logger;
use crate::model::{resolve_endpoint, AppConfig, ServerProfile, SniProfile};
use crate::tray::{self, TrayCommand};
use crate::{autostart, clipboard, elevate, netcheck, store, tun, util, vmess};
use anyhow::Result;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast::error::TryRecvError;

slint::include_modules!();

/// UI state that is shared between the event pump, the callbacks and the
/// background tasks. Must be `Send + Sync` because background tasks read it.
#[derive(Default)]
struct UiState {
    /// Is the tunnel currently up?
    connected: AtomicBool,
    /// Id of the server the tunnel is using.
    active_server: Mutex<String>,
}

impl UiState {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn set_connected(&self, v: bool) {
        self.connected.store(v, Ordering::Relaxed);
    }

    fn active(&self) -> String {
        self.active_server.lock().unwrap().clone()
    }

    fn set_active(&self, id: &str) {
        *self.active_server.lock().unwrap() = id.to_string();
    }
}

pub fn run(cfg: Arc<Mutex<AppConfig>>, logger: Logger) -> Result<i32> {
    let _instance = crate::util::lock_instance()?;
    #[cfg(target_os = "linux")]
    if std::env::var_os("SLINT_BACKEND").is_none() {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()?;
    }
    // ---- engine runtime on a background thread ----
    let (tx, rx) = std::sync::mpsc::channel::<(engine::EngineHandle, tokio::runtime::Handle)>();
    let (cfg_bg, logger_bg) = (cfg.clone(), logger.clone());
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for the engine");
        let _guard = rt.enter();
        let handle = engine::spawn(cfg_bg, logger_bg);
        let _ = tx.send((handle, rt.handle().clone()));
        loop {
            std::thread::park();
        }
    });
    let (engine, rt) = rx.recv()?;

    // ---- UI ----
    let ui = AppWindow::new()?;
    ui.set_platform_windows(cfg!(windows));
    ui.on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
    let editor_slot: Rc<RefCell<Option<ServerEditor>>> = Rc::new(RefCell::new(None));
    let state = Arc::new(UiState::default());
    refresh_servers(&ui, &cfg, &state);
    refresh_snis(&ui, &cfg);
    apply_settings(&ui, &cfg);

    // Find out what the installed core understands (XUDP) in the background,
    // so the editor can offer the XUDP options. The core may still be
    // missing - it is probed again before the first connection.
    {
        let logger_probe = logger.clone();
        std::thread::spawn(move || {
            let exe = crate::corebin::xray_path();
            if exe.is_file() {
                crate::xray::ensure_core_caps(&exe, &logger_probe);
            }
        });
    }

    // The log lines are shared with the log panel's buttons (copy, export,
    // clear).
    let logs: Rc<RefCell<Vec<LogRow>>> = Rc::new(RefCell::new(Vec::new()));

    wire_callbacks(
        &ui,
        &cfg,
        &engine,
        &editor_slot,
        &state,
        rt.clone(),
        logger.clone(),
        &logs,
    );

    // Reports run away from the UI thread and never mutate connection settings.
    {
        let weak = ui.as_weak();
        let config = cfg.clone();
        let runtime = rt.clone();
        ui.on_refresh_diagnostics(move || {
            let snapshot = config.lock().unwrap().clone();
            let weak = weak.clone();
            if let Some(ui) = weak.upgrade() {
                ui.set_diagnostics(
                    "Collecting configuration, routes and HTTP probe results...".into(),
                );
            }
            runtime.spawn(async move {
                let text = match crate::effective_diag::configuration(&snapshot) {
                    Ok((v, source)) => crate::effective_diag::report(&snapshot, &v, &source).await,
                    Err(e) => format!("Diagnostics unavailable: {e:#}"),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| ui.set_diagnostics(text.into()));
            });
        });
    }
    {
        let weak = ui.as_weak();
        let config = cfg.clone();
        ui.on_compare_diagnostics(move |path| {
            let result = (|| -> anyhow::Result<String> {
                let (v, source) = crate::effective_diag::configuration(&config.lock().unwrap())?;
                let input = std::fs::read_to_string(path.as_str())?;
                Ok(format!("{source}\n{}", crate::effective_diag::compare_json(&v, &input)?))
            })();
            if let Some(ui) = weak.upgrade() {
                ui.set_diagnostics(result.unwrap_or_else(|_| "Comparison failed: check file path and supply effective Xray JSON with exactly one proxy outbound.".into()).into());
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_export_diagnostics(move || {
            if let Some(ui) = weak.upgrade() {
                let path = util::app_dir().join("connection-diagnostics.txt");
                match std::fs::write(&path, ui.get_diagnostics().as_str()) {
                    Ok(()) => ui.set_log_status(
                        format!("Diagnostic report exported to {}", path.display()).into(),
                    ),
                    Err(_) => ui.set_log_status("Could not export diagnostic report".into()),
                }
            }
        });
    }

    // ---- system tray ----
    match tray::spawn() {
        Ok(()) => logger.info("system tray ready"),
        Err(e) => logger.warn(format!("system tray unavailable: {e:#}")),
    }

    // Closing the window hides it to the tray (unless the user disabled that),
    // so the tunnel keeps running in the background.
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.window().on_close_requested(move || {
            let to_tray = cfg!(windows) && cfg.lock().unwrap().settings.minimize_to_tray;
            if to_tray {
                if let Some(ui) = weak.upgrade() {
                    let _ = ui.hide();
                }
                slint::CloseRequestResponse::KeepWindowShown
            } else {
                slint::CloseRequestResponse::HideWindow
            }
        });
    }

    // ---- event pump: engine status/stats + log lines + tray ----
    let ui_weak = ui.as_weak();
    let engine_pump = engine.clone();
    let mut eng_rx = engine.subscribe();
    let log_revision = std::cell::Cell::new(u64::MAX);
    let logs_pump = logs.clone();
    let traffic_history = Rc::new(RefCell::new(std::collections::VecDeque::<(u64, u64)>::new()));
    let pump_cfg = cfg.clone();
    let pump_state = state.clone();
    let logger_tab = logger.clone();
    let last_tab = std::cell::Cell::new(-1i32);
    let last_mode = std::cell::RefCell::new(String::new());
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let cfg = &pump_cfg;
        let state = &pump_state;
        let tab = ui.get_tab();
        if tab != last_tab.get() {
            last_tab.set(tab);
            logger_tab.debug(format!("tab changed to {tab}"));
        }

        // The log mode can be switched at any time; the core picks it up at
        // the next connection.
        let mode = ui.get_log_mode().to_string();
        if mode != *last_mode.borrow() {
            *last_mode.borrow_mut() = mode.clone();
            if let Some(m) = crate::logging::LogMode::parse(&mode) {
                logger_tab.set_mode(m);
                logger_tab.info(format!(
                    "the log mode is now '{}'; reconnect to change core verbosity",
                    m.label()
                ));
                let mut c = cfg.lock().unwrap();
                c.settings.log_mode = mode.to_ascii_lowercase();
                let _ = store::save(&c);
            }
        }

        for cmd in tray::poll() {
            match cmd {
                TrayCommand::Show => {
                    let _ = ui.show();
                    ui.window().set_minimized(false);
                }
                TrayCommand::Connect => engine_pump.send(Command::Connect),
                TrayCommand::Disconnect => engine_pump.send(Command::Disconnect),
                TrayCommand::Test => engine_pump.send(Command::TestConnection),
                TrayCommand::Quit => {
                    let _ = slint::quit_event_loop();
                }
            }
        }

        loop {
            match eng_rx.try_recv() {
                Ok(EngineEvent::Status(s)) => {
                    ui.set_connection_live(s.phase.forwarding());
                    ui.set_connection_busy(!matches!(s.phase, Phase::Disconnected | Phase::Error));
                    ui.set_phase(s.phase.label().into());
                    ui.set_detail(s.detail.into());
                    ui.set_status_color(phase_color(s.phase));
                    if let Some(ip) = s.external_ip {
                        ui.set_external_ip(ip.into());
                    }
                    let was = state.is_connected();
                    let now = s.phase.forwarding();
                    if was != now {
                        state.set_connected(now);
                        if now {
                            // Remember which server the tunnel is using.
                            let id = s.server_id.unwrap_or_else(|| {
                                cfg.lock()
                                    .ok()
                                    .and_then(|c| c.selected_server.clone())
                                    .unwrap_or_default()
                            });
                            state.set_active(&id);
                        }
                        refresh_servers(&ui, cfg, state);
                    }
                }
                Ok(EngineEvent::Stats(t)) => {
                    let mut history = traffic_history.borrow_mut();
                    if t.uptime_secs > 0 || t.up > 0 || t.down > 0 {
                        history.push_back((t.up_speed, t.down_speed));
                        if history.len() > 60 {
                            history.pop_front();
                        }
                    }
                    let peak = history
                        .iter()
                        .map(|(up, down)| (*up).max(*down))
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    ui.set_traffic_peak(peak as f32);
                    ui.set_traffic(ModelRc::new(VecModel::from(
                        history
                            .iter()
                            .map(|(up, down)| TrafficPoint {
                                up: *up as f32,
                                down: *down as f32,
                            })
                            .collect::<Vec<_>>(),
                    )));
                    ui.set_up_speed(util::format_rate(t.up_speed).into());
                    ui.set_down_speed(util::format_rate(t.down_speed).into());
                    ui.set_up_total(util::format_bytes(t.up).into());
                    ui.set_down_total(util::format_bytes(t.down).into());
                    ui.set_active_conns(t.active.to_string().into());
                    ui.set_uptime(util::format_duration(t.uptime_secs).into());
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => continue,
            }
        }

        if let Some((revision, history)) = if tab == 3 {
            logger_tab.snapshot_since(log_revision.get())
        } else {
            None
        } {
            log_revision.set(revision);
            let rows: Vec<LogRow> = history
                .into_iter()
                .flat_map(|l| {
                    l.text
                        .lines()
                        .map(|text| LogRow {
                            ts: l.ts.clone().into(),
                            level: l.level.clone().into(),
                            text: text.into(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            *logs_pump.borrow_mut() = rows.clone();
            ui.set_logs(ModelRc::new(VecModel::from(rows)));
            ui.set_log_scroll_request(ui.get_log_scroll_request().wrapping_add(1));
        }
    });

    ui.run()?;

    // Graceful shutdown: restores the system proxy and stops the core.
    engine.shutdown_and_wait(Duration::from_secs(15));
    drop(timer);
    Ok(0)
}

fn phase_color(p: Phase) -> slint::Color {
    let (r, g, b) = match p {
        Phase::Forwarding => (63, 185, 80),
        Phase::CoreReady | Phase::Verifying | Phase::Preparing => (210, 153, 34),
        Phase::Error => (248, 81, 73),
        _ => (139, 147, 167),
    };
    slint::Color::from_rgb_u8(r, g, b)
}

fn wire_callbacks(
    ui: &AppWindow,
    cfg: &Arc<Mutex<AppConfig>>,
    engine: &engine::EngineHandle,
    editor_slot: &Rc<RefCell<Option<ServerEditor>>>,
    state: &Arc<UiState>,
    rt: tokio::runtime::Handle,
    logger: Logger,
    logs: &Rc<RefCell<Vec<LogRow>>>,
) {
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_sort_changed(move || {
            if let Some(ui) = weak.upgrade() {
                refresh_servers(&ui, &cfg, &state);
            }
        });
    }
    {
        let e = engine.clone();
        let cfg = cfg.clone();
        let weak = ui.as_weak();
        ui.on_connect_clicked(move || {
            // TUN mode needs Administrator: relaunch ourselves elevated
            // instead of failing with a dead end.
            let needs_admin = cfg
                .lock()
                .map(|c| c.settings.mode == "tun")
                .unwrap_or(false);
            if cfg!(windows) && needs_admin && !tun::is_elevated() {
                match elevate::relaunch_elevated(&["gui".to_string()]) {
                    Ok(()) => {
                        if let Some(ui) = weak.upgrade() {
                            ui.set_detail(
                                "Administrator rights are needed for the TUN tunnel - \
                                 restarting as Administrator..."
                                    .into(),
                            );
                        }
                        let _ = slint::quit_event_loop();
                        return;
                    }
                    Err(err) => {
                        // The user declined the UAC prompt: report it and let
                        // the engine produce the specific error.
                        e.send(Command::Connect);
                        if let Some(ui) = weak.upgrade() {
                            ui.set_detail(
                                format!(
                                    "the Administrator request was declined ({err}) - \
                                     the TUN tunnel needs elevated rights"
                                )
                                .into(),
                            );
                        }
                        return;
                    }
                }
            }
            e.send(Command::Connect);
        });
    }
    {
        let e = engine.clone();
        ui.on_disconnect_clicked(move || e.send(Command::Disconnect));
    }
    {
        let e = engine.clone();
        ui.on_test_clicked(move || e.send(Command::TestConnection));
    }
    {
        let e = engine.clone();
        ui.on_verify_full_tunnel_clicked(move || e.send(Command::VerifyFullTunnel));
    }
    {
        let e = engine.clone();
        ui.on_refresh_ip_clicked(move || e.send(Command::RefreshIp));
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_import_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let text = ui.get_import_text().to_string();
            if text.trim().is_empty() {
                ui.set_import_error(true);
                ui.set_import_status("Paste a server link or JSON profile first.".into());
                return;
            }
            match import_profile(&cfg, &text) {
                Ok(msg) => {
                    ui.set_import_error(false);
                    ui.set_import_status(msg.into());
                    ui.set_import_text("".into());
                    ui.set_server_search("".into());
                    refresh_servers(&ui, &cfg, &state);
                }
                Err(e) => {
                    ui.set_import_error(true);
                    ui.set_import_status(format!("Import failed: {e:#}").into());
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_use_server(move |i| {
            let mut c = cfg.lock().unwrap();
            if let Some(s) = c.servers.get(i as usize) {
                c.selected_server = Some(s.id.clone());
            }
            let _ = store::save(&c);
            drop(c);
            if let Some(ui) = weak.upgrade() {
                refresh_servers(&ui, &cfg, &state);
            }
        });
    }
    {
        // ---- add a new (empty) server in the editor ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let slot = editor_slot.clone();
        let rt = rt.clone();
        let state = state.clone();
        let logger = logger.clone();
        ui.on_add_server_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut base = ServerProfile {
                id: uuid::Uuid::new_v4().to_string(),
                name: String::new(),
                ..Default::default()
            };
            // A new profile starts with the global defaults (the existing
            // profiles keep their own settings).
            let settings = cfg.lock().unwrap().settings.clone();
            settings.apply_new_profile_defaults(&mut base);
            open_editor(
                &ui,
                &cfg,
                &slot,
                &state,
                rt.clone(),
                logger.clone(),
                base,
                None,
            );
        });
    }
    {
        // ---- edit the selected server ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let slot = editor_slot.clone();
        let rt = rt.clone();
        let state = state.clone();
        let logger = logger.clone();
        ui.on_edit_server(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            // Only the Servers tab can open the editor; a stray event from
            // another tab must not pop the window up.
            if ui.get_tab() != 1 {
                return;
            }
            logger.info(format!(
                "edit_server callback: index={i} tab={} servers={}",
                ui.get_tab(),
                cfg.lock().map(|c| c.servers.len()).unwrap_or(0)
            ));
            let base = {
                let c = cfg.lock().unwrap();
                match c.servers.get(i as usize).cloned() {
                    Some(s) => s,
                    None => return,
                }
            };
            open_editor(
                &ui,
                &cfg,
                &slot,
                &state,
                rt.clone(),
                logger.clone(),
                base,
                Some(i as usize),
            );
        });
    }
    {
        // ---- test a server (DNS / TCP / TLS) ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        let rt = rt.clone();
        let logger = logger.clone();
        ui.on_test_server(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let profile = {
                let c = cfg.lock().unwrap();
                match c.servers.get(i as usize).cloned() {
                    Some(s) => s,
                    None => return,
                }
            };
            ui.set_detail(format!("testing {}...", profile.name).into());
            run_server_test(&ui, &cfg, &state, rt.clone(), logger.clone(), profile);
        });
    }
    {
        // ---- duplicate a server ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_duplicate_server(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let mut c = cfg.lock().unwrap();
                if let Some(src) = c.servers.get(i as usize).cloned() {
                    let copy = duplicate_profile(&src);
                    let name = copy.name.clone();
                    c.servers.push(copy);
                    let _ = store::save(&c);
                    ui.set_detail(format!("duplicated '{name}'").into());
                }
            }
            refresh_servers(&ui, &cfg, &state);
        });
    }
    {
        // ---- export as a vmess:// link (also copied to the clipboard) ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_export_server(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let profile = {
                let c = cfg.lock().unwrap();
                match c.servers.get(i as usize).cloned() {
                    Some(s) => s,
                    None => return,
                }
            };
            let link = vmess::to_vmess_link(&profile);
            match clipboard::write_text(&link) {
                Ok(()) => ui.set_detail(
                    format!("vmess:// link copied to the clipboard ({})", profile.name).into(),
                ),
                Err(e) => ui.set_detail(format!("clipboard failed: {e:#}").into()),
            }
        });
    }
    {
        // ---- import from the clipboard ----
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_import_clipboard_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let text = match clipboard::read_text() {
                Ok(t) => t,
                Err(e) => {
                    ui.set_import_error(true);
                    ui.set_import_status(
                        format!("Clipboard failed: {e:#}. Paste the link below instead.").into(),
                    );
                    return;
                }
            };
            if text.trim().is_empty() {
                ui.set_import_error(true);
                ui.set_import_status("The clipboard is empty. Copy a server link first.".into());
                return;
            }
            match import_profile(&cfg, &text) {
                Ok(msg) => {
                    ui.set_import_error(false);
                    ui.set_import_status(msg.into());
                    ui.set_server_search("".into());
                    refresh_servers(&ui, &cfg, &state);
                }
                Err(e) => {
                    ui.set_import_error(true);
                    ui.set_import_status(format!("Import failed: {e:#}").into());
                }
            }
        });
    }
    {
        // ---- reconnect with the updated configuration ----
        let weak = ui.as_weak();
        let e = engine.clone();
        ui.on_reconnect_now_clicked(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_reconnect_prompt(false);
            }
            e.send(Command::Reconnect);
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_reconnect_later_clicked(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_reconnect_prompt(false);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        let state = state.clone();
        ui.on_remove_server(move |i| {
            {
                let mut c = cfg.lock().unwrap();
                if let Some(removed) = c.servers.get(i as usize).cloned() {
                    c.servers.remove(i as usize);
                    c.sni_profiles.retain(|p| p.server_id != removed.id);
                    if c.selected_server.as_deref() == Some(removed.id.as_str()) {
                        c.selected_server = c.servers.first().map(|s| s.id.clone());
                    }
                    if c.selected_sni_profile
                        .as_deref()
                        .map(|id| !c.sni_profiles.iter().any(|p| p.id == id))
                        .unwrap_or(false)
                    {
                        c.selected_sni_profile = None;
                    }
                    let _ = store::save(&c);
                }
            }
            if let Some(ui) = weak.upgrade() {
                refresh_servers(&ui, &cfg, &state);
                refresh_snis(&ui, &cfg);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_add_sni_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let name = ui.get_sni_name().to_string();
            if name.trim().is_empty() {
                ui.set_detail("SNI profile needs a name".into());
                return;
            }
            let mut c = cfg.lock().unwrap();
            let server_id = c
                .selected_server_profile()
                .map(|s| s.id.clone())
                .unwrap_or_default();
            let profile = SniProfile {
                inherit_tls: false,
                id: uuid::Uuid::new_v4().to_string(),
                name,
                server_id,
                tls: !ui.get_sni_no_tls(),
                sni: ui.get_sni_value().to_string(),
                connect_address: ui.get_sni_connect_addr().to_string(),
                connect_port: ui
                    .get_sni_connect_port()
                    .to_string()
                    .trim()
                    .parse()
                    .unwrap_or(0),
                ws_host: ui.get_sni_ws_host().to_string(),
                ws_path: ui.get_sni_ws_path().to_string(),
                alpn: Vec::new(),
                allow_insecure: ui.get_sni_allow_insecure(),
                fingerprint: String::new(),
            };
            c.selected_sni_profile = Some(profile.id.clone());
            c.sni_profiles.push(profile);
            let _ = store::save(&c);
            drop(c);
            ui.set_sni_name("".into());
            ui.set_sni_value("".into());
            ui.set_sni_ws_host("".into());
            ui.set_sni_ws_path("".into());
            ui.set_sni_connect_addr("".into());
            ui.set_sni_connect_port("".into());
            ui.set_detail("SNI profile added and selected".into());
            refresh_snis(&ui, &cfg);
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_use_sni(move |i| {
            let mut c = cfg.lock().unwrap();
            if let Some(p) = c.sni_profiles.get(i as usize) {
                c.selected_sni_profile = Some(p.id.clone());
            }
            let _ = store::save(&c);
            drop(c);
            if let Some(ui) = weak.upgrade() {
                refresh_snis(&ui, &cfg);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_off_sni_clicked(move || {
            let mut c = cfg.lock().unwrap();
            c.selected_sni_profile = None;
            let _ = store::save(&c);
            drop(c);
            if let Some(ui) = weak.upgrade() {
                refresh_snis(&ui, &cfg);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_remove_sni(move |i| {
            let mut c = cfg.lock().unwrap();
            if let Some(removed) = c.sni_profiles.get(i as usize).cloned() {
                c.sni_profiles.remove(i as usize);
                if c.selected_sni_profile.as_deref() == Some(removed.id.as_str()) {
                    c.selected_sni_profile = None;
                }
                let _ = store::save(&c);
            }
            drop(c);
            if let Some(ui) = weak.upgrade() {
                refresh_snis(&ui, &cfg);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_save_settings_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = cfg.lock().unwrap();
            let previous = c.clone();
            let valid_port =
                |text: slint::SharedString| text.trim().parse::<u16>().ok().filter(|p| *p > 0);
            if valid_port(ui.get_http_port()).is_none()
                || valid_port(ui.get_socks_port()).is_none()
                || ui.get_http_port() == ui.get_socks_port()
            {
                ui.set_detail("Use different HTTP/SOCKS ports between 1 and 65535".into());
                return;
            }
            if ui
                .get_default_mux_concurrency()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|n| *n > 0)
                .is_none()
            {
                ui.set_detail("MUX concurrency must be a positive integer".into());
                return;
            }
            if let Ok(p) = ui.get_http_port().to_string().trim().parse::<u16>() {
                c.settings.http_port = p;
            }
            if let Ok(p) = ui.get_socks_port().to_string().trim().parse::<u16>() {
                c.settings.socks_port = p;
            }
            c.settings.mode = if ui.get_tun_mode() {
                "tun".into()
            } else if ui.get_system_proxy() {
                "system".into()
            } else {
                "none".into()
            };
            c.settings.kill_switch = ui.get_kill_switch();
            c.settings.netmod_compatibility = ui.get_netmod_compatibility();
            c.settings.auto_reconnect = ui.get_auto_reconnect();
            c.settings.minimize_to_tray = ui.get_minimize_to_tray();
            c.settings.log_level = ui.get_log_level().to_string();
            c.settings.custom_dns = ui.get_custom_dns().to_string();
            c.settings.reduced_motion = ui.get_reduced_motion();
            c.settings.compact_layout = ui.get_compact_layout();
            // The defaults for new server profiles.
            c.settings.default_mux_enabled = ui.get_default_mux_enabled();
            c.settings.default_mux_xudp_enabled = ui.get_default_mux_xudp_enabled();
            if let Ok(v) = ui
                .get_default_mux_concurrency()
                .to_string()
                .trim()
                .parse::<i32>()
            {
                if v > 0 {
                    c.settings.default_mux_concurrency = v;
                }
            }
            let want_autostart = ui.get_start_with_windows();
            c.settings.start_with_windows = want_autostart;
            match store::save(&c) {
                Ok(()) => ui.set_detail("settings saved".into()),
                Err(e) => {
                    *c = previous;
                    ui.set_detail(format!("save failed: {e:#}").into());
                    return;
                }
            }
            drop(c);
            match autostart::set_enabled(want_autostart) {
                Ok(()) => ui.set_detail(
                    if want_autostart {
                        "settings saved; will start with your desktop session"
                    } else {
                        "settings saved; autostart disabled"
                    }
                    .into(),
                ),
                Err(e) => {
                    ui.set_detail(format!("settings saved, but autostart failed: {e:#}").into())
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_hide_to_tray_clicked(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_minimized(true);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let logs_clear = logs.clone();
        let logger_clear = logger.clone();
        ui.on_clear_logs_clicked(move || {
            logger_clear.clear_history();
            logs_clear.borrow_mut().clear();
            if let Some(ui) = weak.upgrade() {
                ui.set_logs(ModelRc::new(VecModel::from(Vec::<LogRow>::new())));
                ui.set_log_status("the log was cleared".into());
            }
        });
    }
    {
        // Copy the log to the clipboard. The secrets are already scrubbed, but
        // the copy is scrubbed again - a log that leaves the machine must not
        // carry a credential.
        let weak = ui.as_weak();
        let logs_copy = logs.clone();
        ui.on_copy_logs_clicked(move || {
            let text = logs_copy
                .borrow()
                .iter()
                .map(|r| format!("[{}] {:<5} {}", r.ts, r.level, r.text))
                .collect::<Vec<_>>()
                .join("\n");
            let text = crate::util::scrub_sensitive(&text);
            match crate::clipboard::write_text(&text) {
                Ok(()) => {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_log_status("the log was copied to the clipboard".into());
                    }
                }
                Err(e) => {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_log_status(format!("could not copy the log: {e:#}").into());
                    }
                }
            }
        });
    }
    {
        // Write the log to a file, with the secrets removed.
        let weak = ui.as_weak();
        let logs_export = logs.clone();
        ui.on_export_logs_clicked(move || {
            let text = logs_export
                .borrow()
                .iter()
                .map(|r| format!("[{}] {:<5} {}", r.ts, r.level, r.text))
                .collect::<Vec<_>>()
                .join("\n");
            let text = crate::util::scrub_sensitive(&text);
            let dir = crate::util::app_dir().join("logs");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("sni-tunnel-{}.log", crate::util::now_stamp()));
            match std::fs::write(&path, text) {
                Ok(()) => {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_log_status(
                            format!("the log was written to {}", path.display()).into(),
                        );
                    }
                }
                Err(e) => {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_log_status(format!("could not write the log: {e:#}").into());
                    }
                }
            }
        });
    }
}

fn display_name(s: &ServerProfile) -> String {
    if s.name.trim().is_empty() {
        format!("{}:{}", s.address, s.port)
    } else {
        s.name.clone()
    }
}

fn refresh_servers(ui: &AppWindow, cfg: &Arc<Mutex<AppConfig>>, state: &Arc<UiState>) {
    let c = cfg.lock().unwrap();
    let selected = c
        .selected_server
        .clone()
        .or_else(|| c.servers.first().map(|s| s.id.clone()));
    let active = state.active();
    let connected = state.is_connected();
    let mut ordered: Vec<_> = c.servers.iter().enumerate().collect();
    if ui.get_server_sort() == "Latency" {
        ordered.sort_by_key(|(_, s)| {
            if s.latency_ms == 0 {
                u32::MAX
            } else {
                s.latency_ms
            }
        });
    } else {
        ordered.sort_by_key(|(_, s)| s.name.to_lowercase());
    }
    let rows: Vec<ServerRow> = ordered
        .into_iter()
        .map(|(index, s)| {
            let is_active = s.id == active;
            let status = if is_active && connected {
                "connected"
            } else if s.last_test.trim().is_empty() {
                "untested"
            } else {
                s.last_test.as_str()
            };
            let latency = if s.latency_ms == 0 {
                "-".to_string()
            } else {
                format!("{} ms", s.latency_ms)
            };
            ServerRow {
                source_index: index as i32,
                name: display_name(s).into(),
                addr: s.address.clone().into(),
                port: s.port.to_string().into(),
                transport: format!("{}/{}", s.network, if s.tls { "tls" } else { "none" }).into(),
                sni: if s.sni.trim().is_empty() {
                    "sni: (from address)".into()
                } else {
                    format!("sni: {}", s.sni).into()
                },
                status: status.into(),
                latency: latency.into(),
                selected: selected.as_deref() == Some(s.id.as_str()),
            }
        })
        .collect();
    let dashboard = c
        .servers
        .iter()
        .find(|s| s.id == active)
        .or_else(|| c.selected_server_profile());
    ui.set_active_profile(
        dashboard
            .map(|s| format!("{}  ·  SNI: {}", display_name(s), s.sni))
            .unwrap_or_else(|| "No active server".into())
            .into(),
    );
    ui.set_latency(
        dashboard
            .map(|s| {
                if s.latency_ms == 0 {
                    "Latency not measured".to_string()
                } else {
                    format!("{} ms latency", s.latency_ms)
                }
            })
            .unwrap_or_else(|| "Latency not measured".into())
            .into(),
    );
    ui.set_tunnel_mode(
        if c.settings.mode == "tun" {
            "TUN · whole device"
        } else {
            "HTTP / SOCKS proxy"
        }
        .into(),
    );
    ui.set_servers(ModelRc::new(VecModel::from(rows)));
}

fn refresh_snis(ui: &AppWindow, cfg: &Arc<Mutex<AppConfig>>) {
    let c = cfg.lock().unwrap();
    let rows: Vec<SniRow> = c
        .sni_profiles
        .iter()
        .map(|p| SniRow {
            name: p.name.clone().into(),
            target: c
                .server_by_id(&p.server_id)
                .map(display_name)
                .unwrap_or_else(|| "(any server)".into())
                .into(),
            sni: if p.sni.trim().is_empty() {
                "-".into()
            } else {
                p.sni.clone().into()
            },
            host: if p.ws_host.trim().is_empty() {
                "-".into()
            } else {
                p.ws_host.clone().into()
            },
            selected: c.selected_sni_profile.as_deref() == Some(p.id.as_str()),
        })
        .collect();
    ui.set_snis(ModelRc::new(VecModel::from(rows)));
}

fn apply_settings(ui: &AppWindow, cfg: &Arc<Mutex<AppConfig>>) {
    let c = cfg.lock().unwrap();
    ui.set_http_port(c.settings.http_port.to_string().into());
    ui.set_socks_port(c.settings.socks_port.to_string().into());
    ui.set_system_proxy(c.settings.mode == "system");
    ui.set_tun_mode(c.settings.mode == "tun");
    ui.set_kill_switch(c.settings.kill_switch);
    ui.set_netmod_compatibility(c.settings.netmod_compatibility);
    ui.set_auto_reconnect(c.settings.auto_reconnect);
    ui.set_minimize_to_tray(c.settings.minimize_to_tray);
    ui.set_start_with_windows(c.settings.start_with_windows);
    ui.set_log_level(c.settings.log_level.clone().into());
    ui.set_custom_dns(c.settings.custom_dns.clone().into());
    ui.set_reduced_motion(c.settings.reduced_motion);
    ui.set_compact_layout(c.settings.compact_layout);
    ui.set_log_mode(c.settings.log_mode.clone().into());
    ui.set_log_auto_scroll(true);
    // The defaults for new server profiles.
    ui.set_default_mux_enabled(c.settings.default_mux_enabled);
    ui.set_default_mux_concurrency(c.settings.default_mux_concurrency.to_string().into());
    ui.set_default_mux_xudp_enabled(c.settings.default_mux_xudp_enabled);
}

/// Open the configuration editor for a profile (or for a new one).
#[allow(clippy::too_many_arguments)]
fn open_editor(
    ui: &AppWindow,
    cfg: &Arc<Mutex<AppConfig>>,
    slot: &Rc<RefCell<Option<ServerEditor>>>,
    state: &Arc<UiState>,
    rt: tokio::runtime::Handle,
    logger: Logger,
    base: ServerProfile,
    index: Option<usize>,
) {
    // The editor component is not `Clone`, so it is kept in the slot and used
    // through the borrow.
    if slot.borrow().is_none() {
        let e = match ServerEditor::new() {
            Ok(e) => e,
            Err(err) => {
                ui.set_detail(format!("could not open the editor: {err}").into());
                return;
            }
        };
        wire_editor(&e, ui.as_weak(), cfg, state, rt.clone(), logger.clone());
        *slot.borrow_mut() = Some(e);
    }

    // The editor only offers the XUDP options when the core knows them. The
    // probe runs in the background when the app starts; if it is not done
    // yet, it is finished here (it only runs once).
    let borrowed = slot.borrow();
    let editor = match borrowed.as_ref() {
        Some(e) => e,
        None => return,
    };

    let f = EditorFields::from_profile(&base);
    editor.set_ed_id(f.id.clone().into());
    editor.set_ed_name(f.name.clone().into());
    editor.set_ed_protocol(f.protocol.clone().into());
    editor.set_ed_address(f.address.clone().into());
    editor.set_ed_port(f.port.clone().into());
    editor.set_ed_uuid(f.uuid.clone().into());
    editor.set_ed_alter_id(f.alter_id.clone().into());
    editor.set_ed_security(f.security.clone().into());
    editor.set_ed_network(f.network.clone().into());
    editor.set_ed_tls(f.tls);
    editor.set_ed_sni(f.sni.clone().into());
    editor.set_ed_alpn(f.alpn.clone().into());
    editor.set_ed_fingerprint(f.fingerprint.clone().into());
    editor.set_ed_allow_insecure(f.allow_insecure);
    editor.set_ed_cert_pin(f.pinned_cert_sha256.into());
    editor.set_ed_ws_host(f.ws_host.clone().into());
    editor.set_ed_ws_path(f.ws_path.clone().into());
    editor.set_ed_ws_headers(f.ws_headers.clone().into());
    editor.set_ed_grpc_service(f.grpc_service.clone().into());
    editor.set_ed_grpc_multi(f.grpc_multi);
    editor.set_ed_h2_host(f.h2_host.clone().into());
    editor.set_ed_h2_path(f.h2_path.clone().into());
    editor.set_ed_xhttp_mode(f.xhttp_mode.clone().into());
    editor.set_ed_tcp_header(f.tcp_header.clone().into());
    editor.set_ed_kcp_seed(f.kcp_seed.clone().into());
    editor.set_ed_kcp_header(f.kcp_header.clone().into());
    editor.set_ed_quic_security(f.quic_security.clone().into());
    editor.set_ed_quic_header(f.quic_header.clone().into());
    editor.set_ed_dns(f.dns.clone().into());
    editor.set_ed_mux(f.mux_enabled);
    editor.set_ed_mux_concurrency(f.mux_concurrency.clone().into());
    editor.set_ed_mux_xudp(f.mux_xudp_enabled);
    editor.set_ed_mux_xudp_concurrency(f.mux_xudp_concurrency.clone().into());
    editor.set_ed_mux_udp443(f.mux_xudp_udp443.clone().into());
    // The XUDP fields are only offered when the installed core knows them.
    editor.set_ed_xudp_supported(crate::xray::core_caps().xudp);
    editor.set_ed_timeout(f.timeout_secs.clone().into());
    editor.set_ed_http_port(f.http_port.clone().into());
    editor.set_ed_socks_port(f.socks_port.clone().into());
    editor.set_ed_remark(f.remark.clone().into());
    editor.set_ed_advanced(false);
    editor.set_ed_status(match index {
        Some(_) => "editing an existing server".into(),
        None => "new server".into(),
    });

    // Remember which entry we are editing so Save knows where to put it back.
    EDIT_TARGET.with(|c| *c.borrow_mut() = index);
    logger.info(format!(
        "opening the editor for '{}' (row {:?})",
        base.name, index
    ));

    EDIT_BASE.with(|baseline| *baseline.borrow_mut() = Some(read_editor(editor)));
    editor.set_ed_confirm_close(false);
    editor.set_ed_show_secret(false);
    let _ = editor.show();
}

// Index of the server row the editor is currently editing.
thread_local! {
    static EDIT_BASE: RefCell<Option<EditorFields>> = const { RefCell::new(None) };
    static EDIT_TARGET: RefCell<Option<usize>> = const { RefCell::new(None) };
}

/// Read the editor's fields back into a flat structure.
fn read_editor(editor: &ServerEditor) -> EditorFields {
    EditorFields {
        id: editor.get_ed_id().to_string(),
        name: editor.get_ed_name().to_string(),
        protocol: editor.get_ed_protocol().to_string(),
        address: editor.get_ed_address().to_string(),
        port: editor.get_ed_port().to_string(),
        uuid: editor.get_ed_uuid().to_string(),
        alter_id: editor.get_ed_alter_id().to_string(),
        security: editor.get_ed_security().to_string(),
        network: editor.get_ed_network().to_string(),
        tls: editor.get_ed_tls(),
        sni: editor.get_ed_sni().to_string(),
        alpn: editor.get_ed_alpn().to_string(),
        fingerprint: editor.get_ed_fingerprint().to_string(),
        allow_insecure: editor.get_ed_allow_insecure(),
        pinned_cert_sha256: editor.get_ed_cert_pin().to_string(),
        ws_host: editor.get_ed_ws_host().to_string(),
        ws_path: editor.get_ed_ws_path().to_string(),
        ws_headers: editor.get_ed_ws_headers().to_string(),
        grpc_service: editor.get_ed_grpc_service().to_string(),
        grpc_multi: editor.get_ed_grpc_multi(),
        h2_host: editor.get_ed_h2_host().to_string(),
        h2_path: editor.get_ed_h2_path().to_string(),
        xhttp_mode: editor.get_ed_xhttp_mode().to_string(),
        tcp_header: editor.get_ed_tcp_header().to_string(),
        kcp_seed: editor.get_ed_kcp_seed().to_string(),
        kcp_header: editor.get_ed_kcp_header().to_string(),
        quic_security: editor.get_ed_quic_security().to_string(),
        quic_header: editor.get_ed_quic_header().to_string(),
        dns: editor.get_ed_dns().to_string(),
        mux_enabled: editor.get_ed_mux(),
        mux_concurrency: editor.get_ed_mux_concurrency().to_string(),
        mux_xudp_enabled: editor.get_ed_mux_xudp(),
        mux_xudp_concurrency: editor.get_ed_mux_xudp_concurrency().to_string(),
        mux_xudp_udp443: editor.get_ed_mux_udp443().to_string(),
        timeout_secs: editor.get_ed_timeout().to_string(),
        http_port: editor.get_ed_http_port().to_string(),
        socks_port: editor.get_ed_socks_port().to_string(),
        remark: editor.get_ed_remark().to_string(),
    }
}

/// Wire the editor's buttons. Done once, when the editor is created.
fn wire_editor(
    editor: &ServerEditor,
    main_weak: slint::Weak<AppWindow>,
    cfg: &Arc<Mutex<AppConfig>>,
    state: &Arc<UiState>,
    rt: tokio::runtime::Handle,
    logger: Logger,
) {
    let weak = editor.as_weak();
    let cfg_save = cfg.clone();
    let state_save = state.clone();
    editor.on_save_clicked(move || {
        let Some(editor) = weak.upgrade() else { return };
        let fields = read_editor(&editor);
        let index = EDIT_TARGET.with(|c| *c.borrow());
        let mut c = cfg_save.lock().unwrap();
        let base = match index {
            Some(i) => match c.servers.get(i).cloned() {
                Some(s) => s,
                None => {
                    editor.set_ed_status("the server no longer exists".into());
                    return;
                }
            },
            None => ServerProfile {
                id: uuid::Uuid::new_v4().to_string(),
                ..Default::default()
            },
        };
        let updated = match fields.to_profile(&base) {
            Ok(p) => p,
            Err(e) => {
                editor.set_ed_status(e.into());
                return;
            }
        };
        // XUDP is only written when the installed core knows the fields.
        if updated.mux_xudp_enabled && !base.mux_xudp_enabled && !crate::xray::core_caps().xudp {
            editor.set_ed_status(
                "the installed Xray-core does not support XUDP - switch it off".into(),
            );
            return;
        }
        // The name may be empty: fall back to address:port.
        let mut updated = updated;
        if updated.name.trim().is_empty() {
            updated.name = format!("{}:{}", updated.address, updated.port);
        }
        let previous = c.clone();
        let was_active = c.selected_server.as_deref() == Some(updated.id.as_str());
        match index {
            Some(i) => {
                if let Some(slot_entry) = c.servers.get_mut(i) {
                    *slot_entry = updated;
                }
            }
            None => {
                c.servers.push(updated);
            }
        }
        if let Err(e) = store::save(&c) {
            *c = previous;
            editor.set_ed_status(format!("could not save: {e:#}").into());
            return;
        }
        drop(c);
        EDIT_BASE.with(|baseline| *baseline.borrow_mut() = Some(read_editor(&editor)));
        editor.set_ed_status("saved".into());
        let _ = editor.hide();
        // Refresh the server list in the main window.
        if let Some(main) = main_weak.upgrade() {
            refresh_servers(&main, &cfg_save, &state_save);
        }
        // If the active server changed while connected, offer a reconnect.
        if was_active && state_save.is_connected() {
            if let Some(main) = main_weak.upgrade() {
                main.set_reconnect_prompt(true);
                main.set_reconnect_text(
                    "The active server was updated. Reconnect with the new configuration?".into(),
                );
            }
        }
    });

    editor.on_validate_fields(|address, port, uuid, protocol| {
        if address.trim().is_empty() {
            return "Server address is required".into();
        }
        if port.parse::<u16>().ok().filter(|n| *n > 0).is_none() {
            return "Port must be between 1 and 65535".into();
        }
        if uuid.trim().is_empty()
            || (protocol != "trojan" && !crate::model::is_valid_uuid(uuid.as_str()))
        {
            return "Enter a valid UUID or authentication password".into();
        }
        "".into()
    });
    let weak_cancel = editor.as_weak();
    editor.on_cancel_clicked(move || {
        if let Some(editor) = weak_cancel.upgrade() {
            request_editor_close(&editor);
        }
    });
    let weak_close = editor.as_weak();
    editor.window().on_close_requested(move || {
        if let Some(editor) = weak_close.upgrade() {
            request_editor_close(&editor);
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
    let weak_discard = editor.as_weak();
    editor.on_discard_clicked(move || {
        if let Some(editor) = weak_discard.upgrade() {
            editor.set_ed_confirm_close(false);
            let _ = editor.hide();
        }
    });
    let weak_keep = editor.as_weak();
    editor.on_keep_editing_clicked(move || {
        if let Some(editor) = weak_keep.upgrade() {
            editor.set_ed_confirm_close(false);
        }
    });

    let weak_test = editor.as_weak();
    let rt_test = rt.clone();
    let logger_test = logger.clone();
    editor.on_test_clicked(move || {
        let Some(editor) = weak_test.upgrade() else {
            return;
        };
        let fields = read_editor(&editor);
        let base = ServerProfile {
            id: fields.id.clone(),
            ..Default::default()
        };
        let profile = match fields.to_profile(&base) {
            Ok(p) => p,
            Err(e) => {
                editor.set_ed_status(e.into());
                return;
            }
        };
        editor.set_ed_status("testing...".into());
        let weak_editor = editor.as_weak();
        let log = logger_test.clone();
        rt_test.spawn(async move {
            let ep = resolve_endpoint(&profile, None);
            let d = netcheck::diagnose(&ep, None, engine::TEST_URL, Duration::from_secs(10)).await;
            let text = d.summary();
            log.info(format!("test of {}:\n{}", profile.name, d.report()));
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(editor) = weak_editor.upgrade() {
                    editor.set_ed_status(text.into());
                }
            });
        });
    });
}

/// Run the diagnostic ladder for a server and record the result.
fn run_server_test(
    ui: &AppWindow,
    cfg: &Arc<Mutex<AppConfig>>,
    state: &Arc<UiState>,
    rt: tokio::runtime::Handle,
    logger: Logger,
    profile: ServerProfile,
) {
    let id = profile.id.clone();
    let name = profile.name.clone();
    let weak = ui.as_weak();
    let cfg_bg = cfg.clone();
    let state_bg = state.clone();
    rt.spawn(async move {
        let ep = resolve_endpoint(&profile, None);
        let d = netcheck::diagnose(&ep, None, engine::TEST_URL, Duration::from_secs(10)).await;
        let latency = ep_latency(&ep).await;
        let ok = d.first_failure().is_none();
        logger.info(format!("server '{name}' test:\n{}", d.report()));
        {
            let mut c = cfg_bg.lock().unwrap();
            if let Some(s) = c.servers.iter_mut().find(|s| s.id == id) {
                s.latency_ms = latency;
                s.last_test = if ok { "ok".into() } else { "failed".into() };
            }
            let _ = store::save(&c);
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_detail(format!("{name}: {} ({} ms)", d.summary(), latency).into());
            refresh_servers(&ui, &cfg_bg, &state_bg);
        }
    });
}

/// Measure the TCP round trip to the server (milliseconds).
async fn ep_latency(ep: &crate::model::Endpoint) -> u32 {
    match netcheck::tcp_connect(&ep.address, ep.port, Duration::from_secs(10)).await {
        Ok((_, ms)) => ms as u32,
        Err(_) => 0,
    }
}

fn import_profile(cfg: &Arc<Mutex<AppConfig>>, text: &str) -> Result<String> {
    let imported = vmess::parse_any(text)?;
    if imported.is_empty() {
        anyhow::bail!("no server profiles found in the input");
    }
    let mut c = cfg.lock().unwrap();
    let mut updated = c.clone();
    let messages = merge_imported(&mut updated, imported);
    store::save(&updated)?;
    *c = updated;
    Ok(messages.join("; "))
}

/// Compare connection settings independently of labels and measured state.
fn same_imported_connection(a: &ServerProfile, b: &ServerProfile) -> bool {
    let normalize = |p: &ServerProfile| {
        let mut p = p.clone();
        p.id.clear();
        p.name.clear();
        p.remark.clear();
        p.latency_ms = 0;
        p.last_test.clear();
        p
    };
    normalize(a) == normalize(b)
}

/// Merge freshly-parsed profiles into the config, skipping identical connections.
/// Pure (no I/O) so it can be unit-tested.
fn merge_imported(c: &mut AppConfig, imported: Vec<ServerProfile>) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    for mut p in imported {
        if p.id.is_empty() {
            p.id = uuid::Uuid::new_v4().to_string();
        }
        if p.name.trim().is_empty() {
            p.name = format!("{}:{}", p.address, p.port);
        }
        if c.servers.iter().any(|s| same_imported_connection(s, &p)) {
            messages.push(format!("skipped duplicate {}:{}", p.address, p.port));
            continue;
        }
        if c.servers.iter().any(|s| s.id == p.id) {
            p.id = uuid::Uuid::new_v4().to_string();
        }
        if c.selected_server.is_none() {
            c.selected_server = Some(p.id.clone());
        }
        messages.push(format!("added {}", p.name));
        c.servers.push(p);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srv(addr: &str, port: u16, net: &str) -> ServerProfile {
        ServerProfile {
            address: addr.into(),
            port,
            network: net.into(),
            ..Default::default()
        }
    }

    #[test]
    fn import_adds_and_selects_first() {
        let mut c = AppConfig::default();
        let msgs = merge_imported(&mut c, vec![srv("a.example", 443, "ws")]);
        assert_eq!(c.servers.len(), 1);
        assert_eq!(c.selected_server, Some(c.servers[0].id.clone()));
        assert!(msgs[0].starts_with("added"));
    }

    #[test]
    fn import_skips_identical_connections() {
        let mut c = AppConfig::default();
        merge_imported(&mut c, vec![srv("a.example", 443, "ws")]);
        let msgs = merge_imported(&mut c, vec![srv("a.example", 443, "ws")]);
        assert_eq!(c.servers.len(), 1);
        assert!(msgs[0].starts_with("skipped duplicate"));
    }

    #[test]
    fn import_keeps_distinct_accounts_and_connection_settings() {
        let mut c = AppConfig::default();
        let base = srv("a.example", 443, "ws");
        let mut profiles = vec![base.clone()];
        for change in 0..5 {
            let mut p = base.clone();
            match change {
                0 => p.uuid = "other-account".into(),
                1 => p.sni = "other.example".into(),
                2 => p.ws_path = "/other".into(),
                3 => p.ws_host = "host.example".into(),
                _ => p.protocol = "vless".into(),
            }
            profiles.push(p);
        }
        merge_imported(&mut c, profiles);
        assert_eq!(c.servers.len(), 6);
        let ids: std::collections::HashSet<_> = c.servers.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), 6, "distinct imported profiles need distinct IDs");
    }

    #[test]
    fn import_duplicate_ignores_labels_and_test_results() {
        let mut c = AppConfig::default();
        let base = srv("a.example", 443, "ws");
        merge_imported(&mut c, vec![base.clone()]);
        let mut renamed = base;
        renamed.id = "new-id".into();
        renamed.name = "new label".into();
        renamed.remark = "new remark".into();
        renamed.latency_ms = 80;
        renamed.last_test = "recent".into();
        merge_imported(&mut c, vec![renamed]);
        assert_eq!(c.servers.len(), 1);
    }

    #[test]
    fn import_does_not_select_if_already_selected() {
        let mut c = AppConfig {
            selected_server: Some("keep-me".into()),
            ..Default::default()
        };
        merge_imported(&mut c, vec![srv("a.example", 443, "ws")]);
        assert_eq!(c.selected_server.as_deref(), Some("keep-me"));
    }
}

fn request_editor_close(editor: &ServerEditor) {
    let dirty = EDIT_BASE.with(|base| {
        base.borrow()
            .as_ref()
            .is_some_and(|fields| *fields != read_editor(editor))
    });
    if dirty {
        editor.set_ed_confirm_close(true);
    } else {
        let _ = editor.hide();
    }
}
