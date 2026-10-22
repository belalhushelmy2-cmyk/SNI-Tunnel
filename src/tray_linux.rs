//! Native StatusNotifierItem tray for Linux/Wayland and X11 desktops.
use anyhow::{Context, Result};
use ksni::blocking::TrayMethods;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Mutex, OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Connect,
    Disconnect,
    Test,
    Quit,
}
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static EVENTS: OnceLock<(
    mpsc::Sender<TrayCommand>,
    Mutex<mpsc::Receiver<TrayCommand>>,
)> = OnceLock::new();
static HANDLE: Mutex<Option<ksni::blocking::Handle<LinuxTray>>> = Mutex::new(None);
struct LinuxTray {
    tx: mpsc::Sender<TrayCommand>,
}
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "sni-tunnel-v2".into()
    }
    fn title(&self) -> String {
        "SNI Tunnel V2".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut data = include_bytes!("../assets/icon-32.rgba").to_vec();
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data,
        }]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "SNI Tunnel V2".into(),
            description: "Click to show the window".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        let _ = self.tx.send(TrayCommand::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        [
            ("Show window", TrayCommand::Show),
            ("Connect", TrayCommand::Connect),
            ("Disconnect", TrayCommand::Disconnect),
            ("Test connection", TrayCommand::Test),
            ("Quit", TrayCommand::Quit),
        ]
        .into_iter()
        .map(|(label, cmd)| {
            ksni::menu::StandardItem {
                label: label.into(),
                activate: Box::new(move |tray: &mut Self| {
                    let _ = tray.tx.send(cmd);
                }),
                ..Default::default()
            }
            .into()
        })
        .collect()
    }
    fn watcher_online(&self) {
        AVAILABLE.store(true, Ordering::Relaxed);
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        AVAILABLE.store(false, Ordering::Relaxed);
        true
    }
}
pub fn is_available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}
pub fn spawn() -> Result<()> {
    let mut handle = HANDLE.lock().unwrap();
    if handle.is_some() {
        return Ok(());
    }
    let (tx, _) = EVENTS.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        (tx, Mutex::new(rx))
    });
    AVAILABLE.store(true, Ordering::Relaxed);
    match (LinuxTray { tx: tx.clone() })
        .assume_sni_available(true)
        .spawn()
    {
        Ok(tray) => {
            *handle = Some(tray);
            Ok(())
        }
        Err(error) => {
            AVAILABLE.store(false, Ordering::Relaxed);
            Err(error).context("Register Linux tray with the desktop session")
        }
    }
}
pub fn poll() -> Vec<TrayCommand> {
    EVENTS
        .get()
        .map(|(_, rx)| rx.lock().unwrap().try_iter().collect())
        .unwrap_or_default()
}
pub fn shutdown() {
    AVAILABLE.store(false, Ordering::Relaxed);
    if let Some(handle) = HANDLE.lock().unwrap().take() {
        handle.shutdown();
    }
}
