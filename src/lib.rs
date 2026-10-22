//! SNI Tunnel — a lightweight VMess/VLESS/Trojan client with an independent
//! custom TLS SNI, built on Xray-core.
//!
//! The crate is split into a library (this file) and a thin binary
//! (`src/main.rs`) so that the behaviour can be tested with integration tests.

pub mod clipboard;
pub mod compat;
pub mod console;
pub mod corebin;
pub mod diag;
pub mod editor;
pub mod effective_diag;
pub mod engine;
pub mod localproxy;
pub mod logging;
pub mod model;
pub mod netcheck;
pub mod session;
pub mod stats;
pub mod store;
pub mod storecli;
pub mod tap_compat;
pub mod tun;
pub mod util;
pub mod vmess;
pub mod xray;

#[cfg(feature = "gui")]
pub mod gui;

// ---------------------------------------------------------------------------
// Platform specific implementations
// ---------------------------------------------------------------------------
// Windows has the native versions; every other platform uses the portable
// ones. The aliases keep the rest of the code platform independent.
#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod dpapi;
#[cfg(windows)]
pub mod elevate;
#[cfg(windows)]
pub mod sysproxy;
#[cfg(all(windows, feature = "gui"))]
pub mod tray;

#[cfg(not(windows))]
pub mod autostart_linux;
#[cfg(not(windows))]
pub use autostart_linux as autostart;
#[cfg(not(windows))]
pub mod dpapi_linux;
#[cfg(not(windows))]
pub use dpapi_linux as dpapi;
#[cfg(not(windows))]
pub mod elevate_linux;
#[cfg(not(windows))]
pub use elevate_linux as elevate;
#[cfg(not(windows))]
pub mod sysproxy_linux;
#[cfg(not(windows))]
pub use sysproxy_linux as sysproxy;
#[cfg(all(not(windows), feature = "gui"))]
pub mod tray_linux;
#[cfg(all(not(windows), feature = "gui"))]
pub use tray_linux as tray;

#[cfg(windows)]
pub mod child_job;

#[cfg(target_os = "linux")]
pub mod linux_tunnel;
pub mod migration;
