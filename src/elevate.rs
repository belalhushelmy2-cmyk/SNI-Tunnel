//! Self-elevation via the UAC "runas" verb.
//!
//! TUN mode needs Administrator (creating the wintun adapter and changing
//! routes). When the user asks for a TUN connection from the unelevated GUI we
//! relaunch ourselves elevated instead of showing a dead end.

use anyhow::{bail, Context, Result};
use std::os::windows::ffi::OsStrExt;

/// Relaunch the current executable with the given arguments as Administrator.
/// Returns `Err` when the user declines the UAC prompt.
pub fn relaunch_elevated(args: &[String]) -> Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe().context("cannot locate our own executable")?;
    let params = args.join(" ");

    unsafe {
        let file: Vec<u16> = exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

        let res = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        ) as isize;
        if res <= 32 {
            bail!("the Administrator request was declined");
        }
    }
    Ok(())
}
