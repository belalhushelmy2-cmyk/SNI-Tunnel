use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn app_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SNI_TUNNEL_DATA_DIR") {
        let p = PathBuf::from(path);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    #[cfg(windows)]
    let p = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()))
        .join("SniTunnelV2");
    #[cfg(not(windows))]
    let p = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
                .join(".local/share")
        })
        .join("sni-tunnel-v2");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn bin_dir() -> PathBuf {
    let p = app_dir().join("bin");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn runtime_config_path() -> PathBuf {
    app_dir().join("runtime-xray.json")
}

pub fn app_log_path() -> PathBuf {
    app_dir().join("sni-tunnel.log")
}

/// Ask the OS for a currently-free loopback TCP port.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(0)
}

pub fn format_bytes(n: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", U[i])
    } else {
        format!("{v:.2} {}", U[i])
    }
}

pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

pub fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// UTC HH:MM:SS timestamp (no external time crate needed).
pub fn now_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

/// A file name stamp: `20261021-153412`.
pub fn now_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// The calendar date of a number of days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Replace any occurrence of a secret (e.g. a UUID) with a mask before it can
/// reach a log sink.
/// A short, non-reversible hint for a secret (used in the diagnostics so a
/// credential can be recognised without ever being printed).
pub fn short_hash(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(secret.as_bytes());
    hex::encode(&digest[..4])
}

pub fn scrub(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "********")
}

/// Scrub a list of secrets from a log line.
pub fn scrub_all(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        out = scrub(&out, s);
    }
    out
}

/// Remove the sensitive data from a log that is about to leave the machine.
///
/// The registered secrets (the UUIDs) are already gone, but a log can hold
/// more: a UUID in a different notation, a password, a token or an id. This
/// scrubs them, so an exported log can be attached to a bug report.
pub fn scrub_sensitive(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        if let Some(len) = uuid_len_at(bytes, i) {
            out.push_str("********");
            i += len;
            continue;
        }
        // Copy the whole character.
        let ch = match text[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        out.push(ch);
        i += ch.len_utf8();
    }
    let out = replace_json_value(&out, "\"id\"");
    let out = replace_json_value(&out, "\"uuid\"");
    let out = replace_json_value(&out, "\"password\"");
    let out = replace_json_value(&out, "\"pass\"");
    let out = replace_json_value(&out, "\"token\"");
    let out = replace_json_value(&out, "\"secret\"");
    // URI query strings and authentication headers in free-form core errors
    // may carry arbitrary credentials; conceal the rest of that line.
    out.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let cut = [
                "?",
                "authorization:",
                "cookie:",
                "token=",
                "password=",
                "secret=",
            ]
            .iter()
            .filter_map(|marker| lower.find(marker))
            .min();
            match cut {
                Some(i) => format!("{}[redacted]", &line[..i]),
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The length of a UUID that starts at `i`, if there is one.
///
/// Both the dashed (8-4-4-4-12) and the undashed notation are recognised.
fn uuid_len_at(bytes: &[u8], i: usize) -> Option<usize> {
    for len in [36, 32] {
        if let Some(candidate) = bytes.get(i..i + len) {
            if let Ok(text) = std::str::from_utf8(candidate) {
                if uuid::Uuid::parse_str(text).is_ok() {
                    return Some(len);
                }
            }
        }
    }
    None
}

/// Replace the value of a JSON field with a mask.
fn replace_json_value(text: &str, key: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find(key) {
        let after = &rest[pos + key.len()..];
        let Some(value) = after.trim_start().strip_prefix(':') else {
            out.push_str(&rest[..pos + key.len()]);
            rest = after;
            continue;
        };
        let Some(quoted) = value.trim_start().strip_prefix('"') else {
            out.push_str(&rest[..pos + key.len()]);
            rest = after;
            continue;
        };
        let prefix_len = rest.len() - quoted.len();
        out.push_str(&rest[..prefix_len]);
        out.push_str("********\"");
        let mut escaped = false;
        let end = quoted.char_indices().find_map(|(i, c)| {
            if escaped {
                escaped = false;
                return None;
            }
            if c == '\\' {
                escaped = true;
                return None;
            }
            if c == '"' {
                Some(i)
            } else {
                None
            }
        });
        rest = end.map(|i| &quoted[i + 1..]).unwrap_or("");
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_credentials_and_url_queries_are_hidden() {
        let text = r#"{"password":"first\"secret-tail"}"#;
        assert!(!scrub_sensitive(text).contains("secret-tail"));
        assert!(!scrub_sensitive("failed /ws?ticket=private-ticket").contains("private-ticket"));
        assert!(!scrub_sensitive("Authorization: Bearer private-token").contains("private-token"));
        assert_eq!(scrub("password abc", "abc"), "password ********");
    }
    #[test]
    fn uuids_are_scrubbed() {
        let line =
            r#"from tcp:1.2.3.4 accepted [inbound] email: b831381d-6324-4d53-ad4f-8cda48b30811"#;
        let out = scrub_sensitive(line);
        assert!(!out.contains("b831381d"), "the UUID is still in: {out}");
        assert!(out.contains("********"));
    }

    #[test]
    fn an_undashed_uuid_is_scrubbed() {
        let out = scrub_sensitive("id b831381d63244d53ad4f8cda48b30811 end");
        assert!(!out.contains("b831381d"), "the UUID is still in: {out}");
    }

    #[test]
    fn credentials_are_scrubbed() {
        let out = scrub_sensitive(
            r#"{"id": "b831381d-6324-4d53-ad4f-8cda48b30811", "password": "secret-value"}"#,
        );
        assert!(
            !out.contains("secret-value"),
            "the password is still in: {out}"
        );
        assert!(!out.contains("b831381d"), "the id is still in: {out}");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let line = "[15:23:41] Xray-core started successfully on 127.0.0.1:2080";
        assert_eq!(scrub_sensitive(line), line);
    }
}

/// OS-backed lock; released on crash without stale PID files.
pub fn lock_instance() -> anyhow::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(app_dir().join("instance.lock"))?;
    file.try_lock().map_err(|e| {
        anyhow::anyhow!(
            "SNI Tunnel V2 is already running, or its instance lock is unavailable: {e}"
        )
    })?;
    Ok(file)
}

/// Previous releases do not share the V2 data lock. Refuse a competing Windows
/// client before any proxy/adapter changes; never terminate another process.
pub fn ensure_no_competing_client() -> anyhow::Result<()> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        };
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Process32FirstW(snapshot, &mut entry) != 0;
        while found {
            let end = entry
                .szExeFile
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if entry.th32ProcessID != std::process::id()
                && name.eq_ignore_ascii_case("sni-tunnel.exe")
            {
                CloseHandle(snapshot);
                anyhow::bail!("Another SNI Tunnel client is running. Disconnect and close it before connecting V2.");
            }
            found = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    Ok(())
}
