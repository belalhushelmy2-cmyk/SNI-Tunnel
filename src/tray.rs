//! System-tray icon (Windows) running on a dedicated thread with its own
//! message pump. Menu / click events are delivered through muda's global
//! channels and drained from the UI thread via [`poll`].

use anyhow::{Context, Result};
use std::sync::mpsc;
use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const MENU_SHOW: &str = "sni-tunnel.show";
pub const MENU_CONNECT: &str = "sni-tunnel.connect";
pub const MENU_DISCONNECT: &str = "sni-tunnel.disconnect";
pub const MENU_TEST: &str = "sni-tunnel.test";
pub const MENU_QUIT: &str = "sni-tunnel.quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Connect,
    Disconnect,
    Test,
    Quit,
}

/// Create the tray icon on a background thread and start pumping messages.
/// Blocks until the icon has been created so startup errors surface here.
pub fn spawn() -> Result<()> {
    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("sni-tunnel-tray".into())
        .spawn(move || match build_tray() {
            Ok(tray) => {
                let _ = tx.send(Ok(()));
                pump_messages();
                drop(tray);
            }
            Err(e) => {
                let _ = tx.send(Err(format!("{e:#}")));
            }
        })
        .context("spawn tray thread")?;

    match rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => anyhow::bail!("{e}"),
        Err(_) => anyhow::bail!("tray thread exited during startup"),
    }
}

fn build_tray() -> Result<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::with_id(MENU_SHOW, "Show window", true, None);
    let connect = MenuItem::with_id(MENU_CONNECT, "Connect", true, None);
    let disconnect = MenuItem::with_id(MENU_DISCONNECT, "Disconnect", true, None);
    let test = MenuItem::with_id(MENU_TEST, "Test connection", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);

    let items: Vec<&dyn IsMenuItem> = vec![
        &show as &dyn IsMenuItem,
        &connect as &dyn IsMenuItem,
        &disconnect as &dyn IsMenuItem,
        &test as &dyn IsMenuItem,
        &separator as &dyn IsMenuItem,
        &quit as &dyn IsMenuItem,
    ];
    menu.append_items(&items).context("build tray menu")?;

    let (w, h, rgba) = icon_rgba();
    let icon = tray_icon::Icon::from_rgba(rgba, w, h).context("create tray icon image")?;

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("SNI Tunnel")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()
        .context("create tray icon")
}

/// Drain any pending tray menu / click events. Called on the UI thread.
pub fn poll() -> Vec<TrayCommand> {
    let mut out = Vec::new();

    while let Ok(event) = MenuEvent::receiver().try_recv() {
        let cmd = match event.id().as_ref() {
            MENU_SHOW => Some(TrayCommand::Show),
            MENU_CONNECT => Some(TrayCommand::Connect),
            MENU_DISCONNECT => Some(TrayCommand::Disconnect),
            MENU_TEST => Some(TrayCommand::Test),
            MENU_QUIT => Some(TrayCommand::Quit),
            _ => None,
        };
        if let Some(c) = cmd {
            out.push(c);
        }
    }

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        match event {
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => out.push(TrayCommand::Show),
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => out.push(TrayCommand::Show),
            _ => {}
        }
    }

    out
}

/// The tray icon: 32x32 raw RGBA, pre-rendered from `assets/icon.png` by
/// `scripts/make-icon.ps1` (so we do not need an image decoder at runtime).
fn icon_rgba() -> (u32, u32, Vec<u8>) {
    const S: u32 = 32;
    const RAW: &[u8] = include_bytes!("../assets/icon-32.rgba");
    debug_assert_eq!(RAW.len(), (S * S * 4) as usize);
    (S, S, RAW.to_vec())
}

#[cfg(windows)]
fn pump_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG,
    };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if ret <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(windows))]
fn pump_messages() {}
