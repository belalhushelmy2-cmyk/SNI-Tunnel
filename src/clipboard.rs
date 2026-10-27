//! Clipboard access without extra dependencies.
//!
//! * Windows: the Win32 clipboard API.
//! * Linux: `wl-paste` (Wayland) or `xclip`/`xsel` (X11) as a subprocess.
//! * Other platforms: unsupported (the functions report a clear error).

/// Read text from the clipboard.
pub fn read_text() -> anyhow::Result<String> {
    #[cfg(windows)]
    {
        read_text_windows()
    }
    #[cfg(not(windows))]
    {
        read_text_unix()
    }
}

/// Write text to the clipboard.
pub fn write_text(text: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        write_text_windows(text)
    }
    #[cfg(not(windows))]
    {
        write_text_unix(text)
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn read_text_windows() -> anyhow::Result<String> {
    use anyhow::bail;
    use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::{CF_TEXT, CF_UNICODETEXT};

    let text: String = unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            bail!("could not open the clipboard (error {})", GetLastError());
        }
        let mut handle: HANDLE = std::ptr::null_mut();
        let mut unicode = false;
        for (fmt, is_unicode) in [(CF_UNICODETEXT, true), (CF_TEXT, false)] {
            handle = GetClipboardData(fmt as u32);
            if !handle.is_null() {
                unicode = is_unicode;
                break;
            }
        }
        if handle.is_null() {
            CloseClipboard();
            return Ok(String::new());
        }
        let locked = GlobalLock(handle);
        let text = if locked.is_null() {
            String::new()
        } else if unicode {
            let ptr = locked as *const u16;
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        } else {
            let ptr = locked as *const u8;
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
        };
        if !locked.is_null() {
            GlobalUnlock(handle);
        }
        CloseClipboard();
        text
    };
    Ok(text)
}

#[cfg(windows)]
fn write_text_windows(text: &str) -> anyhow::Result<()> {
    use anyhow::bail;
    use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            bail!("could not open the clipboard (error {})", GetLastError());
        }
        EmptyClipboard();

        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * std::mem::size_of::<u16>();
        let handle: HANDLE = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            CloseClipboard();
            bail!("out of memory while copying to the clipboard");
        }
        let locked = GlobalLock(handle);
        if locked.is_null() {
            CloseClipboard();
            bail!("could not lock clipboard memory");
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), locked as *mut u16, wide.len());
        GlobalUnlock(handle);

        if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
            CloseClipboard();
            bail!("could not set the clipboard data");
        }
        CloseClipboard();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unix (Linux)
// ---------------------------------------------------------------------------

#[cfg(all(not(windows), not(target_os = "macos")))]
fn read_text_unix() -> anyhow::Result<String> {
    use anyhow::{Context, Result};
    use std::process::Command;

    for (cmd, args) in [
        ("wl-paste", vec!["--no-newline", "--type", "text/plain"]),
        ("xclip", vec!["-selection", "clipboard", "-o"]),
        ("xsel", vec!["--clipboard", "--output"]),
    ] {
        if let Ok(out) = Command::new(cmd).args(&args).output() {
            if out.status.success() {
                return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
            }
        }
    }
    Err(anyhow::anyhow!(
        "no clipboard tool found (install wl-clipboard or xclip)"
    ))
    .context("read clipboard")
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn write_text_unix(text: &str) -> anyhow::Result<()> {
    use anyhow::{Context, Result};
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (cmd, args) in [
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ] {
        let mut child = match Command::new(cmd)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        drop(child.stdin.take());
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "no clipboard tool found (install wl-clipboard or xclip)"
    ))
    .context("write clipboard")
}

#[cfg(target_os = "macos")]
fn read_text_unix() -> anyhow::Result<String> {
    let output = std::process::Command::new("/usr/bin/pbpaste").output()?;
    anyhow::ensure!(output.status.success(), "Could not read macOS clipboard");
    Ok(String::from_utf8(output.stdout)?)
}
#[cfg(target_os = "macos")]
fn write_text_unix(text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()?;
    let written = child.stdin.take().unwrap().write_all(text.as_bytes());
    let status = child.wait()?;
    written?;
    anyhow::ensure!(status.success(), "Could not write macOS clipboard");
    Ok(())
}
