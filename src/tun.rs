//! Stage 6: TUN mode, built on Xray-core's native `tun` inbound (wintun).
//!
//! `xray.exe` itself creates the wintun adapter and processes its packets, so
//! there is no tun2socks dependency. Traffic path:
//!   system -> wintun adapter -> Xray tun inbound -> outbound (VMess/direct)
//! The outbound is pinned to the physical uplink with `sockopt.interface`
//! (IP_UNICAST_IF), which keeps Xray's own traffic out of the TUN — otherwise
//! it would be routed back into the tunnel and loop.
//!
//! Requires Administrator. All pure logic (config/route builders, parsers) is
//! unit-tested; the live orchestration is never exercised by `cargo test`.

use crate::corebin::{self, xray_path, XRAY_PACKAGE};
use crate::logging::Logger;
use crate::model::{Endpoint, Settings};
use crate::util;
use crate::xray::{self, XrayProcess};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Name of the wintun adapter created by Xray's tun inbound.
pub const DEVICE_NAME: &str = "sni-tunnel";
/// Address assigned to the TUN adapter.
pub const TUN_IP: &str = "198.18.0.1";
/// IPv6 address of the TUN adapter (so IPv6 can be tunnelled as well).
pub const TUN_IPV6: &str = "fc00::1";
/// Prefix length of the TUN subnet.
pub const TUN_PREFIX: u8 = 24;
/// Prefix length of the TUN IPv6 subnet.
pub const TUN_PREFIX_V6: u8 = 64;
/// Link MTU of the TUN adapter.
pub const TUN_MTU: u16 = 1500;
/// URL used to prove the TUN datapath (raw IP so routing is unambiguous).
pub const VERIFY_URL: &str = "https://1.1.1.1/cdn-cgi/trace";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// There is only one live tunnel per process (guarded by the TUN ownership lock).
static ACTIVE_TAP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub fn active_tun_ip() -> &'static str {
    if ACTIVE_TAP.lock().unwrap().is_some() {
        crate::tap_compat::IP
    } else {
        TUN_IP
    }
}
fn active_tun_gateway() -> &'static str {
    if ACTIVE_TAP.lock().unwrap().is_some() {
        crate::tap_compat::GATEWAY
    } else {
        TUN_IP
    }
}
pub fn active_device() -> String {
    ACTIVE_TAP
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| DEVICE_NAME.into())
}

// ---------------------------------------------------------------------------
// Pure builders (unit-tested)
// ---------------------------------------------------------------------------

/// The Xray `tun` inbound document for our adapter.
pub fn tun_inbound(device: &str, mtu: u16) -> Value {
    json!({
        "tag": "tun-in",
        "protocol": "tun",
        "settings": { "name": device, "mtu": mtu }
    })
}

/// Prepend `sockopt.interface` to an outbound so Xray forces its traffic onto
/// a specific interface (IP_UNICAST_IF) instead of following the route table.
pub fn bind_outbound_interface(mut outbound: Value, iface: &str) -> Value {
    if outbound.get("streamSettings").is_none() {
        outbound["streamSettings"] = json!({});
    }
    outbound["streamSettings"]["sockopt"] = json!({ "interface": iface });
    outbound
}

/// Full Xray document for TUN mode: one tun inbound, one outbound.
pub fn tun_config(device: &str, mtu: u16, outbound: Value, loglevel: &str) -> Value {
    json!({
        "log": { "loglevel": loglevel },
        "inbounds": [tun_inbound(device, mtu)],
        "outbounds": [outbound]
    })
}

/// `netsh interface ipv4 set address name=... source=static addr=... mask=...`.
///
/// The keyed `source=`/`addr=`/`mask=` form is required: the positional form
/// silently no-ops on a wintun adapter.
pub fn netsh_set_address(device: &str, ip: &str, mask: &str) -> Vec<String> {
    vec![
        "interface".into(),
        "ipv4".into(),
        "set".into(),
        "address".into(),
        format!("name={device}"),
        "source=static".into(),
        format!("addr={ip}"),
        format!("mask={mask}"),
        "store=active".into(),
    ]
}

/// `netsh interface ipv6 set address name=... source=static addr=... prefix=...`.
pub fn netsh_set_address_v6(device: &str, ip: &str, prefix: u8) -> Vec<String> {
    vec![
        "interface".into(),
        "ipv6".into(),
        "set".into(),
        "address".into(),
        format!("name={device}"),
        "source=static".into(),
        format!("addr={ip}"),
        format!("prefix={prefix}"),
        "store=active".into(),
    ]
}

/// Disable duplicate-address detection so the address becomes usable at once.
pub fn netsh_disable_dad(device: &str) -> Vec<String> {
    vec![
        "interface".into(),
        "ipv4".into(),
        "set".into(),
        "interface".into(),
        format!("interface={device}"),
        "dadtransmits=0".into(),
        "store=active".into(),
    ]
}

/// `netsh interface ipv4 add route`. The interface must be named explicitly —
/// `route.exe add` picks the wrong interface for a wintun next hop.
pub fn netsh_add_route(
    device: &str,
    dest: &str,
    prefix: u8,
    nexthop: Option<&str>,
    ipv6: bool,
) -> Vec<String> {
    let family = if ipv6 { "ipv6" } else { "ipv4" };
    let mut args = vec![
        "interface".into(),
        family.into(),
        "add".into(),
        "route".into(),
        format!("{dest}/{prefix}"),
        format!("interface={device}"),
    ];
    if let Some(h) = nexthop {
        args.push(format!("nexthop={h}"));
    }
    args.push("metric=1".into());
    args.push("store=active".into());
    args
}

/// `netsh interface ipv4 delete route`, scoped to the interface we used.
pub fn netsh_delete_route(device: &str, dest: &str, prefix: u8, ipv6: bool) -> Vec<String> {
    let family = if ipv6 { "ipv6" } else { "ipv4" };
    vec![
        "interface".into(),
        family.into(),
        "delete".into(),
        "route".into(),
        format!("{dest}/{prefix}"),
        format!("interface={device}"),
        "store=active".into(),
    ]
}

/// The interface address of the route to `dest` in `route print -4` output.
///
/// The route table prints the interface *address* (not its name) in the fourth
/// column, which is enough to tell a physical route apart from one that goes
/// back into the TUN adapter.
pub fn route_interface_for(dest: &str, prefix: u8, route_print: &str) -> Option<String> {
    let mask = mask_from_prefix(prefix)?;
    for line in route_print.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() >= 4 && t[0] == dest && t[1] == mask {
            return Some(t[3].to_string());
        }
    }
    None
}

/// An IPv4 address as the number the routing table uses (`a.b.c.d` becomes
/// `0xAABBCCDD`).
pub fn ipv4_value(s: &str) -> Option<u32> {
    let mut v = 0u32;
    let mut n = 0;
    for part in s.split('.') {
        let b: u8 = part.parse().ok()?;
        v = (v << 8) | b as u32;
        n += 1;
    }
    if n == 4 {
        Some(v)
    } else {
        None
    }
}

/// The route the system would use for `ip`: `(gateway, interface_address)`.
///
/// The longest matching prefix wins, the lowest metric breaks a tie - the same
/// rules the routing table itself uses. The gateway is the TUN address when the
/// route leads into the tunnel (Windows prints "On-link" in the gateway column
/// when the next hop is the address of the adapter itself, so the interface
/// column has to be looked at as well).
pub fn best_route_for(ip: &str, route_print: &str) -> Option<(String, String)> {
    let target = ipv4_value(ip)?;
    let mut best: Option<(u32, u32, String, String)> = None;
    for line in route_print.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 || t[0] == "Network" || t[0] == "Active" || t[0] == "Persistent" {
            continue;
        }
        let (Some(dest), Some(mask)) = (ipv4_value(t[0]), ipv4_value(t[1])) else {
            continue;
        };
        if target & mask != dest {
            continue;
        }
        let prefix = mask.count_ones();
        let metric = t
            .get(4)
            .and_then(|m| m.parse::<u32>().ok())
            .unwrap_or(u32::MAX);
        let better = match &best {
            None => true,
            Some((p, m, _, _)) => prefix > *p || (prefix == *p && metric < *m),
        };
        if better {
            best = Some((prefix, metric, t[2].to_string(), t[3].to_string()));
        }
    }
    best.map(|(_, _, gw, iface)| (gw, iface))
}

/// Dotted-quad netmask for a CIDR prefix length.
pub fn mask_from_prefix(prefix: u8) -> Option<String> {
    if prefix > 32 {
        return None;
    }
    let m: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Some(format!(
        "{}.{}.{}.{}",
        (m >> 24) & 0xff,
        (m >> 16) & 0xff,
        (m >> 8) & 0xff,
        m & 0xff
    ))
}

/// One route to install; `via_gateway` is omitted for on-link routes.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteSpec {
    pub dest: String,
    pub prefix: u8,
    pub via_gateway: Option<String>,
    pub on_interface: String,
    /// IPv6 route (`netsh interface ipv6`) instead of IPv4.
    pub ipv6: bool,
}

/// Two /1 routes that shadow the default route without deleting it. This keeps
/// the original default route intact so a crash restores connectivity.
///
/// The same is done for IPv6, so IPv6 traffic cannot leak around the tunnel.
pub fn split_default_routes(device: &str) -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            dest: "0.0.0.0".into(),
            prefix: 1,
            via_gateway: Some(TUN_IP.into()),
            on_interface: device.into(),
            ipv6: false,
        },
        RouteSpec {
            dest: "128.0.0.0".into(),
            prefix: 1,
            via_gateway: Some(TUN_IP.into()),
            on_interface: device.into(),
            ipv6: false,
        },
        RouteSpec {
            dest: "::".into(),
            prefix: 1,
            via_gateway: Some(TUN_IPV6.into()),
            on_interface: device.into(),
            ipv6: true,
        },
        RouteSpec {
            dest: "8000::".into(),
            prefix: 1,
            via_gateway: Some(TUN_IPV6.into()),
            on_interface: device.into(),
            ipv6: true,
        },
    ]
}

/// /32 routes for specific hosts through the TUN device (self-test mode).
pub fn host_routes(hosts: &[String], device: &str) -> Vec<RouteSpec> {
    hosts
        .iter()
        .map(|h| RouteSpec {
            dest: h.clone(),
            prefix: 32,
            via_gateway: Some(TUN_IP.into()),
            on_interface: device.into(),
            ipv6: false,
        })
        .collect()
}

/// Pull the IPv4 default route (gateway, interface address) out of
/// `route print -4` output. Returns `gateway=None` when it is on-link.
pub fn parse_default_route(route_print: &str) -> Option<(Option<String>, String)> {
    for line in route_print.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() >= 4 && t[0] == "0.0.0.0" && t[1] == "0.0.0.0" {
            let gw = if t[2] == "On-link" {
                None
            } else {
                Some(t[2].to_string())
            };
            return Some((gw, t[3].to_string()));
        }
    }
    None
}

/// Find the interface alias from `netsh interface ipv4 show addresses` output
/// that owns the given IP.
pub fn parse_alias_for_ip(addresses: &str, ip: &str) -> Option<String> {
    let mut alias = None;
    for line in addresses.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("Configuration for interface \"") {
            if let Some(end) = rest.strip_suffix('"') {
                alias = Some(end.to_string());
            }
        } else if let Some(rest) = l.strip_prefix("IP Address:") {
            if rest.trim().eq_ignore_ascii_case(ip) {
                if let Some(a) = alias.clone() {
                    return Some(a);
                }
            }
        }
    }
    None
}

/// Pull the reported egress IP out of a Cloudflare `/cdn-cgi/trace` body.
pub fn parse_trace_ip(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("ip="))
        .map(|s| s.trim().to_string())
}

/// Names of the connected adapters from `netsh interface show interface`.
pub fn parse_connected_interfaces(show_interface: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in show_interface.lines() {
        let mut it = line.split_whitespace();
        let admin = it.next().unwrap_or("");
        let state = it.next().unwrap_or("");
        let typ = it.next().unwrap_or("");
        if admin != "Enabled" || state != "Connected" || typ.is_empty() {
            continue;
        }
        // The interface name is the remainder of the line and may contain
        // spaces ("Ethernet 4"), so it cannot be split on whitespace.
        if let Some(name) = remainder_after_third_token(line) {
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out
}

/// Text after the third whitespace-separated token of a line.
fn remainder_after_third_token(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut pos = 0usize;
    for _ in 0..3 {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if start == pos {
            return None;
        }
    }
    Some(line[pos..].trim().to_string())
}

/// Does this adapter look like a VPN client that will fight the TUN?
///
/// Cloudflare WARP and similar clients install a WFP filter driver that
/// rejects connections originating from a foreign TUN adapter
/// (`WSAEACCES / 10013`), and they also hijack the whole IPv4 route table.
pub fn looks_like_vpn_client(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // Our own adapter is not a conflict.
    if n == DEVICE_NAME || n.starts_with("sni-") {
        return false;
    }
    const MARKERS: &[&str] = &[
        "warp",
        "cloudflare",
        "wireguard",
        "openvpn",
        "tunnel",
        "vpn",
        "nord",
        "mullvad",
        "proton",
        "tailscale",
        "zerotier",
        "tunsafe",
        "twingate",
        "expressvpn",
        "surfshark",
        "hidemyass",
    ];
    MARKERS.iter().any(|m| n.contains(m))
}

// ---------------------------------------------------------------------------
// Elevation
// ---------------------------------------------------------------------------

/// True if the current process token is elevated.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut ret_len: u32 = 0;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            );
            CloseHandle(token);
            ok != 0 && elevation.TokenIsElevated != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Route mode
// ---------------------------------------------------------------------------

/// How the TUN interface should be routed.
#[derive(Clone, Debug)]
pub enum RouteMode {
    /// Only these /32 hosts go through the TUN (safe; used for self-tests).
    Hosts(Vec<String>),
    /// The whole internet goes through the TUN. `exclusions` are the addresses
    /// that must still use the physical network (the VPN server).
    DefaultWithExclusions(Vec<String>),
}

// ---------------------------------------------------------------------------
// System tool invocation
// ---------------------------------------------------------------------------

fn system32(name: &str) -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

async fn run_tool(exe: &Path, args: &[String], logger: &Logger) -> Result<(i32, String)> {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd
        .output()
        .await
        .with_context(|| format!("run {}", exe.display()))?;
    let code = out.status.code().unwrap_or(-1);
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let text = text.trim().to_string();
    logger.debug(format!(
        "{} {} -> ({}) {}",
        exe.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        args.join(" "),
        code,
        text.replace('\n', " ")
    ));
    Ok((code, text))
}

/// Connected VPN/tunnel clients that are likely to block TUN traffic.
pub async fn conflicting_vpn_clients(logger: &Logger) -> Vec<String> {
    let netsh = system32("netsh.exe");
    let (_, text) = match run_tool(
        &netsh,
        &["interface".into(), "show".into(), "interface".into()],
        logger,
    )
    .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    parse_connected_interfaces(&text)
        .into_iter()
        .filter(|n| looks_like_vpn_client(n))
        .collect()
}

/// Discover the machine's primary uplink: `(adapter alias, gateway, local IP)`.
async fn discover_uplink(logger: &Logger) -> Result<(String, Option<String>, String)> {
    let route_exe = system32("route.exe");
    let netsh = system32("netsh.exe");

    let (code, routes_txt) = run_tool(&route_exe, &["print".into(), "-4".into()], logger).await?;
    if code != 0 {
        bail!("route print failed");
    }
    let (gw, if_ip) = parse_default_route(&routes_txt).context("no IPv4 default route found")?;

    let (_, addr_txt) = run_tool(
        &netsh,
        &[
            "interface".into(),
            "ipv4".into(),
            "show".into(),
            "addresses".into(),
        ],
        logger,
    )
    .await?;
    let alias = parse_alias_for_ip(&addr_txt, &if_ip)
        .with_context(|| format!("no adapter owns the default-route address {if_ip}"))?;
    Ok((alias, gw, if_ip))
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

pub struct TunReport {
    pub status: u16,
    pub ms: u128,
    pub egress_ip: Option<String>,
}

/// Perform an HTTPS GET whose source address is bound to the TUN interface.
/// Retried a few times because the address can be briefly unusable while the
/// interface settles.
pub async fn verify_through_tun(url: &str, timeout: Duration) -> Result<TunReport> {
    tokio::time::timeout(timeout, async {
        let per_attempt = timeout.min(Duration::from_secs(8));
        let mut last = None;
        for attempt in 0..6 {
            match verify_once(url, per_attempt).await {
                Ok(r) => return Ok(r),
                Err(e) => last = Some(e),
            }
            if attempt < 5 {
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
        }
        Err(last.expect("at least one attempt"))
    })
    .await
    .context("TUN forwarding check exceeded its total timeout")?
}

async fn verify_once(url: &str, timeout: Duration) -> Result<TunReport> {
    let ip: std::net::IpAddr = active_tun_ip().parse().unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .local_address(ip)
        .timeout(timeout)
        .user_agent("SniTunnel/0.1")
        .build()
        .context("build verify client")?;
    let t = Instant::now();
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url} via TUN failed"))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    Ok(TunReport {
        status,
        ms: t.elapsed().as_millis(),
        egress_ip: parse_trace_ip(&body),
    })
}

// ---------------------------------------------------------------------------
// Live TUN session
// ---------------------------------------------------------------------------

enum Outbound {
    Vmess(Box<Endpoint>),
    Netmod(Box<Endpoint>),
    Direct,
}

/// The stages of the TUN setup, in order. Used for precise reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunStage {
    Config,
    Core,
    Adapter,
    Address,
    Dns,
    Routes,
    Forwarding,
}

impl TunStage {
    pub fn label(self) -> &'static str {
        match self {
            TunStage::Config => "Xray configuration",
            TunStage::Core => "Xray process",
            TunStage::Adapter => "TUN adapter",
            TunStage::Address => "TUN addressing",
            TunStage::Dns => "DNS configuration",
            TunStage::Routes => "routing configuration",
            TunStage::Forwarding => "traffic forwarding",
        }
    }
}

/// A specific reason why the TUN tunnel could not be brought up.
#[derive(Debug, Clone)]
pub enum TunFailure {
    /// The process is not elevated.
    NotElevated,
    /// `wintun.dll` is missing next to `xray.exe`, or has the wrong CPU
    /// architecture.
    WintunMissing,
    /// Xray refused the generated configuration.
    ConfigRejected(String),
    /// Xray could not create the wintun adapter.
    AdapterFailed(String),
    /// The adapter could not be addressed.
    AddressFailed(String),
    /// The DNS servers could not be configured.
    DnsFailed(String),
    /// The routes could not be installed.
    RouteFailed(String),
    /// Another VPN client is active and conflicts with the TUN interface.
    VpnConflict(String),
    /// The tunnel is up but no traffic is forwarded.
    ForwardingFailed(String),
    /// Anything else.
    Other(String),
}

impl TunFailure {
    /// A message for the status line / the log.
    pub fn message(&self) -> String {
        match self {
            TunFailure::NotElevated => {
                "TUN mode needs Administrator privileges - start SNI Tunnel as Administrator \
                 (or use the Connect button, which asks for elevation)"
                    .to_string()
            }
            TunFailure::WintunMissing => {
                "the wintun driver (wintun.dll) is missing or does not match the \
                 architecture of xray.exe - it must be in the same folder as xray.exe"
                    .to_string()
            }
            TunFailure::ConfigRejected(m) => format!("Xray rejected the configuration: {m}"),
            TunFailure::AdapterFailed(m) => {
                format!("the TUN adapter could not be created: {m}")
            }
            TunFailure::AddressFailed(m) => {
                format!("the TUN adapter could not be addressed: {m}")
            }
            TunFailure::DnsFailed(m) => {
                format!("the DNS servers could not be configured: {m}")
            }
            TunFailure::RouteFailed(m) => {
                format!("the routes could not be configured: {m}")
            }
            TunFailure::VpnConflict(m) => {
                format!("another VPN client conflicts with the TUN interface: {m}")
            }
            TunFailure::ForwardingFailed(m) => {
                format!("the tunnel is up but no traffic is forwarded: {m}")
            }
            TunFailure::Other(m) => m.clone(),
        }
    }
}

/// An error that knows *which* stage failed and *why*.
#[derive(Debug, Clone)]
pub struct TunSetupError {
    pub stage: TunStage,
    pub failure: TunFailure,
}

impl TunSetupError {
    pub fn new(stage: TunStage, failure: TunFailure) -> Self {
        Self { stage, failure }
    }
}

impl std::fmt::Display for TunSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage.label(), self.failure.message())
    }
}

impl std::error::Error for TunSetupError {}

/// A live TUN-mode tunnel. Dropping without [`TunSession::stop`] relies on
/// `kill_on_drop` for the child; routes are best-effort removed first.
pub struct TunSession {
    compatibility: Option<crate::compat::CompatibilityCore>,
    core: Option<XrayProcess>,
    tap: Option<crate::tap_compat::TapFrontend>,
    routes: Vec<RouteSpec>,
    /// The name of the wintun adapter that was actually created.
    pub device: String,
    /// The DNS configuration that was replaced when the tunnel went up.
    dns_backups: Vec<DnsBackup>,
    /// The addresses of the server the tunnel connects to. The traffic that
    /// leaves through the tunnel has the server's address as its exit address.
    pub server_addrs: Vec<String>,
    /// Counts the connections the tunnel currently carries.
    pub conns: Arc<crate::stats::ConnCounter>,
}

impl TunSession {
    /// Start a TUN tunnel whose outbound is the given VMess endpoint.
    pub async fn start(
        mut ep: Endpoint,
        settings: &Settings,
        mode: RouteMode,
        verify_hash: bool,
        logger: &Logger,
    ) -> Result<Self> {
        ep.validate()
            .map_err(|e| anyhow::anyhow!("invalid endpoint: {e}"))?;
        if ep.dns.trim().is_empty() {
            ep.dns = settings.custom_dns.clone();
        }
        Self::spawn(
            if settings.netmod_compatibility {
                Outbound::Netmod(Box::new(ep))
            } else {
                Outbound::Vmess(Box::new(ep))
            },
            xray::core_log_level(settings),
            mode,
            verify_hash,
            logger,
        )
        .await
    }

    /// Start a TUN tunnel whose outbound is `freedom` (direct). Used by the
    /// self-test to validate the datapath without a real server.
    pub async fn start_direct(
        settings: &Settings,
        mode: RouteMode,
        verify_hash: bool,
        logger: &Logger,
    ) -> Result<Self> {
        Self::spawn(
            Outbound::Direct,
            xray::normalize_level(&settings.log_level),
            mode,
            verify_hash,
            logger,
        )
        .await
    }

    async fn spawn(
        outbound: Outbound,
        loglevel: String,
        mode: RouteMode,
        verify_hash: bool,
        logger: &Logger,
    ) -> Result<Self> {
        // The TUN setup uses Windows specific tooling (wintun, netsh, route).
        #[cfg(not(windows))]
        {
            return Err(TunSetupError::new(
                TunStage::Adapter,
                TunFailure::Other(
                    "TUN mode is currently implemented on Windows only - use the \
                     system proxy mode on this platform"
                        .into(),
                ),
            )
            .into());
        }

        if !is_elevated() {
            return Err(TunSetupError::new(TunStage::Adapter, TunFailure::NotElevated).into());
        }

        if !matches!(outbound, Outbound::Netmod(_)) {
            corebin::ensure_package(&XRAY_PACKAGE, verify_hash, logger).await?;
        }

        // Only one instance may own the wintun adapter.
        acquire_tun_lock(logger)?;

        // From here on the lock is held: every error path has to release it,
        // otherwise a failed setup would block the next start.
        match Self::spawn_locked(outbound, loglevel, mode, logger).await {
            Ok(s) => Ok(s),
            Err(e) => {
                release_tun_lock();
                Err(e)
            }
        }
    }

    async fn spawn_locked(
        outbound: Outbound,
        loglevel: String,
        mode: RouteMode,
        logger: &Logger,
    ) -> Result<Self> {
        if let Outbound::Netmod(ep) = &outbound {
            return Self::spawn_tap_locked(ep, &loglevel, &mode, logger).await;
        }

        // The wintun driver must sit next to xray.exe and match its
        // architecture, otherwise the core cannot create the adapter.
        check_wintun(&xray_path(), logger)?;

        // Other VPN clients (Cloudflare WARP and friends) install a filter
        // driver that rejects traffic from a foreign TUN adapter. Warn early
        // instead of letting the user stare at a timeout.
        let conflicts = conflicting_vpn_clients(logger).await;
        if !conflicts.is_empty() {
            logger.warn(format!(
                "other VPN clients are active ({}); they can block TUN traffic - stop them first",
                conflicts.join(", ")
            ));
        }

        // The physical uplink is needed to pin the outbound to it (preventing
        // a loop) and to keep the VPN server outside the tunnel.
        let (uplink_iface, uplink_gw, uplink_ip) = discover_uplink(logger).await?;
        logger.info(format!(
            "Physical uplink adapter '{}' (gateway {})",
            uplink_iface,
            uplink_gw.as_deref().unwrap_or("on-link")
        ));

        // Find out what the core understands (XUDP) before the configuration
        // is written, so the configuration only uses fields it knows.
        xray::ensure_core_caps(&xray_path(), logger);

        let compatibility = match &outbound {
            Outbound::Netmod(ep) => {
                // Resolve before DNS is redirected into TUN. The legacy core
                // must never need its own tunnel to resolve the tunnel server.
                let mut dial = (**ep).clone();
                let addresses = match &mode {
                    RouteMode::DefaultWithExclusions(addresses) => addresses.clone(),
                    RouteMode::Hosts(_) => crate::netcheck::resolve(&ep.address, ep.port)
                        .await?
                        .addrs
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                };
                dial.address = addresses
                    .into_iter()
                    .find(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
                    .context(
                        "NetMod compatibility needs an IPv4 server address excluded from TUN",
                    )?;
                Some(
                    crate::compat::CompatibilityCore::start(&dial, &uplink_ip, &loglevel, logger)
                        .await?,
                )
            }
            _ => None,
        };
        let dns_config = match &outbound {
            Outbound::Vmess(ep) => xray::build_config(ep, &Settings::default(), 10800)
                .get("dns")
                .cloned(),
            Outbound::Direct | Outbound::Netmod(_) => None,
        };
        let outbound = match &outbound {
            Outbound::Netmod(_) => {
                crate::compat::bridge_outbound(compatibility.as_ref().unwrap().port)
            }
            Outbound::Vmess(ep) => bind_outbound_interface(xray::vmess_outbound(ep), &uplink_iface),
            Outbound::Direct => bind_outbound_interface(
                json!({ "tag": "proxy", "protocol": "freedom" }),
                &uplink_iface,
            ),
        };

        // A wintun adapter whose name is still registered from a crashed run
        // cannot be re-created ("initialization has already been completed").
        // Such an adapter cannot be removed reliably, so we simply use a
        // fresh name - the old one is then cleaned up by the core.
        let mut last_error: Option<TunSetupError> = None;
        let mut started = None;
        let conns = Arc::new(crate::stats::ConnCounter::new());
        for attempt in 0..4u32 {
            let device = if attempt == 0 {
                DEVICE_NAME.to_string()
            } else {
                format!("{DEVICE_NAME}-{attempt}")
            };
            let mut cfg = tun_config(&device, TUN_MTU, outbound.clone(), &loglevel);
            if let Some(dns) = &dns_config {
                cfg["dns"] = dns.clone();
            }

            // 1. Validate the configuration *before* touching the network, so a
            //    broken config never leaves an adapter or a route behind.
            //    The check uses a throw-away SOCKS inbound: `xray run -test`
            //    builds the inbound handlers, and a `tun` inbound would create
            //    the wintun adapter as a side effect.
            let cfg_path = util::runtime_config_path();
            xray::write_config(&cfg, &cfg_path)?;
            logger.info(format!("Wrote TUN runtime config {}", cfg_path.display()));
            let check_cfg = Self::validation_config(&cfg);
            let check_path = cfg_path.with_extension("check.json");
            if let Err(e) = xray::write_config(&check_cfg, &check_path)
                .and_then(|_| xray::validate_config(&xray_path(), &check_path))
            {
                let _ = std::fs::remove_file(&check_path);
                return Err(TunSetupError::new(
                    TunStage::Config,
                    TunFailure::ConfigRejected(root_cause(&e)),
                )
                .into());
            }
            let _ = std::fs::remove_file(&check_path);
            logger.info(format!("[{}] ok", TunStage::Config.label()));

            // 2. Start Xray: it creates the wintun adapter.
            logger.event("Starting Xray-core");
            let mut core = match XrayProcess::start_with_counter(
                &xray_path(),
                cfg_path,
                logger,
                Some(conns.clone()),
            )
            .await
            {
                Ok(c) => c,
                Err(e) => {
                    return Err(TunSetupError::new(
                        TunStage::Core,
                        TunFailure::Other(root_cause(&e)),
                    )
                    .into())
                }
            };
            logger.event("Xray-core started successfully");

            // 3. Give the adapter an address (Xray's `gateway` setting is
            //    unreliable here, so we always do it ourselves).
            let mask = mask_from_prefix(TUN_PREFIX).unwrap();
            match set_address_with_retry(&device, TUN_IP, &mask, &mut core, logger).await {
                Ok(()) => {
                    started = Some((device, core));
                    break;
                }
                Err(e) => {
                    logger.warn(format!(
                        "the TUN adapter '{device}' could not be initialised ({}) - trying another name",
                        e.failure.message()
                    ));
                    core.stop(logger).await;
                    last_error = Some(e);
                }
            }
        }
        let (device, mut core) = match started {
            Some(v) => v,
            None => {
                let e = last_error.unwrap_or_else(|| {
                    TunSetupError::new(
                        TunStage::Adapter,
                        TunFailure::AdapterFailed("no usable wintun adapter name".into()),
                    )
                });
                return Err(e.into());
            }
        };
        logger.info(format!(
            "[{}] ok - adapter '{device}'",
            TunStage::Adapter.label()
        ));
        logger.event(format!("TUN adapter initialized ({device})"));

        // 4. Point the adapter at a public resolver so DNS travels through
        //    the tunnel as well.
        if let Err(e) = configure_dns(&device, logger).await {
            logger.warn(format!("[{}] failed: {e}", TunStage::Dns.label()));
        } else {
            logger.event("Custom DNS configured (the tunnel's resolver is in use)");
        }

        // 5. Install routes, cleaning them up on any failure. Only the routes
        //    that were really created are kept for the cleanup - a route that
        //    already existed belongs to another program and stays untouched.
        logger.event("Preparing VPN routes");
        let wanted = build_routes(&mode, &device, &uplink_iface, uplink_gw.as_deref());
        let routes = match apply_routes(&wanted, logger).await {
            Ok(r) => r,
            Err(e) => {
                core.stop(logger).await;
                return Err(TunSetupError::new(
                    TunStage::Routes,
                    TunFailure::RouteFailed(e.to_string()),
                )
                .into());
            }
        };
        logger.event(format!("VPN routes installed ({} route(s))", routes.len()));

        // Remember the routes so a crash cannot leave the machine without
        // connectivity: the next start removes them again.
        write_route_journal(&routes);

        // 6. Send the DNS of the physical adapters into the tunnel as well:
        //    Windows asks every configured resolver in parallel, so the
        //    physical one would answer directly and the DNS traffic would
        //    leave the machine outside the tunnel.
        let dns_backups = hijack_dns(logger);
        write_dns_journal(&dns_backups);

        Ok(TunSession {
            compatibility,
            core: Some(core),
            tap: None,
            routes,
            device,
            dns_backups,
            server_addrs: match &mode {
                RouteMode::DefaultWithExclusions(a) => a.clone(),
                RouteMode::Hosts(_) => Vec::new(),
            },
            conns,
        })
    }
    async fn spawn_tap_locked(
        ep: &Endpoint,
        level: &str,
        mode: &RouteMode,
        logger: &Logger,
    ) -> Result<Self> {
        let (uplink, gateway, uplink_ip) = discover_uplink(logger).await?;
        let addresses = match mode {
            RouteMode::DefaultWithExclusions(a) => a.clone(),
            RouteMode::Hosts(_) => crate::netcheck::resolve(&ep.address, ep.port)
                .await?
                .addrs
                .iter()
                .map(ToString::to_string)
                .collect(),
        };
        let mut dial = ep.clone();
        dial.address = addresses
            .iter()
            .find(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
            .context("TAP requires an IPv4 server")?
            .clone();
        let mut backend =
            crate::compat::CompatibilityCore::start(&dial, &uplink_ip, level, logger).await?;
        let mut tap = crate::tap_compat::TapFrontend::start(backend.port, logger).await?;
        let mut wanted = build_routes(mode, &tap.device, &uplink, gateway.as_deref());
        for r in &mut wanted {
            if r.on_interface == tap.device && !r.ipv6 {
                r.via_gateway = Some(crate::tap_compat::GATEWAY.into());
            }
        }
        // Store a truthful backend snapshot, not the previous modern TUN frontend.
        let mut runtime = crate::compat::config(&dial, backend.port, &uplink_ip, level)?;
        runtime["sniTunnelFrontend"] =
            json!({"type":"netmod-tap", "device":tap.device, "dns":["8.8.8.8","8.8.4.4"]});
        xray::write_config(&runtime, &util::runtime_config_path())?;
        let routes = match apply_routes(&wanted, logger).await {
            Ok(r) => r,
            Err(e) => {
                tap.stop().await;
                backend.stop(logger).await;
                return Err(e);
            }
        };
        write_route_journal(&routes);
        *ACTIVE_TAP.lock().unwrap() = Some(tap.device.clone());
        logger.event(format!(
            "VPN routes installed ({} route(s)); NetMod TAP datapath",
            routes.len()
        ));
        logger.info("TAP experiment preserves physical-adapter DNS like NetMod; active connection count is unavailable");
        Ok(Self {
            compatibility: Some(backend),
            core: None,
            device: tap.device.clone(),
            tap: Some(tap),
            routes,
            dns_backups: Vec::new(),
            server_addrs: addresses,
            conns: Arc::new(crate::stats::ConnCounter::new()),
        })
    }
    /// A copy of the TUN configuration with the `tun` inbound replaced by a
    /// throw-away SOCKS inbound, so it can be validated without creating the
    /// wintun adapter.
    pub fn validation_config(cfg: &Value) -> Value {
        let mut out = cfg.clone();
        if let Some(inbounds) = out.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for ib in inbounds.iter_mut() {
                if ib.get("protocol").and_then(|p| p.as_str()) == Some("tun") {
                    *ib = json!({
                        "tag": "check-socks",
                        "protocol": "socks",
                        "listen": "127.0.0.1",
                        "port": 0,
                        "settings": { "auth": "noauth", "udp": false }
                    });
                }
            }
        }
        out
    }

    /// A human readable report of the setup stages that succeeded.
    pub fn stage_report(&self) -> String {
        let lines = [
            format!("[{}] ok", TunStage::Config.label()),
            format!("[{}] ok", TunStage::Core.label()),
            format!("[{}] ok", TunStage::Adapter.label()),
            format!("[{}] ok", TunStage::Address.label()),
            format!("[{}] ok", TunStage::Dns.label()),
            format!("[{}] ok", TunStage::Routes.label()),
        ];
        lines.join("\n")
    }

    pub fn core_exit_code(&mut self) -> Option<i32> {
        self.core
            .as_mut()
            .and_then(|c| c.try_exit_code())
            .or_else(|| self.tap.as_mut().and_then(|t| t.exit_code()))
            .or_else(|| self.compatibility.as_mut().and_then(|c| c.exit_code()))
    }

    /// Remove routes *before* killing Xray so no traffic is directed at a
    /// vanishing interface.
    ///
    /// With `keep_routes` (the kill switch) the routes stay in place: traffic
    /// then has no path out of the machine instead of escaping the tunnel,
    /// while the server's own /32 route still allows a reconnect.
    pub async fn stop(&mut self, logger: &Logger, keep_routes: bool) {
        if !keep_routes {
            let _ = remove_routes(&self.routes, logger).await;
            // The journal has served its purpose; a clean stop must not leave
            // it behind (it would trigger a pointless recovery on the next
            // start).
            let _ = std::fs::remove_file(route_journal_path());
        } else {
            logger.warn(
                "kill switch: retaining routes; blocking is not guaranteed if the adapter disappears",
            );
        }
        // The DNS of the physical adapters is restored in any case.
        restore_dns(&self.dns_backups, logger);
        let _ = std::fs::remove_file(dns_journal_path());
        if let Some(core) = self.core.as_mut() {
            core.stop(logger).await;
        }
        if let Some(mut tap) = self.tap.take() {
            tap.stop().await;
        }
        *ACTIVE_TAP.lock().unwrap() = None;
        if let Some(mut backend) = self.compatibility.take() {
            backend.stop(logger).await;
        }
        release_tun_lock();
    }
}

fn build_routes(
    mode: &RouteMode,
    device: &str,
    uplink_iface: &str,
    uplink_gw: Option<&str>,
) -> Vec<RouteSpec> {
    match mode {
        RouteMode::Hosts(hosts) => host_routes(hosts, device),
        RouteMode::DefaultWithExclusions(exclusions) => {
            let mut r = split_default_routes(device);
            r.extend(exclusions.iter().map(|dest| RouteSpec {
                dest: dest.clone(),
                prefix: 32,
                via_gateway: uplink_gw.map(|g| g.to_string()),
                on_interface: uplink_iface.to_string(),
                ipv6: false,
            }));
            r
        }
    }
}

async fn set_address_with_retry(
    device: &str,
    ip: &str,
    mask: &str,
    core: &mut XrayProcess,
    logger: &Logger,
) -> std::result::Result<(), TunSetupError> {
    let netsh = system32("netsh.exe");
    let deadline = Instant::now() + Duration::from_secs(20);

    // 1. Disable DAD so the address is not left in the "tentative" state.
    let mut dad_ok = false;
    while Instant::now() < deadline {
        let (code, _) = run_tool(&netsh, &netsh_disable_dad(device), logger)
            .await
            .map_err(|e| {
                TunSetupError::new(TunStage::Address, TunFailure::AddressFailed(root_cause(&e)))
            })?;
        if code == 0 {
            dad_ok = true;
            break;
        }
        if let Some(e) = core_failure(core) {
            return Err(e);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    if !dad_ok {
        logger.warn("could not disable duplicate-address detection; continuing anyway");
    }

    // 2. Assign the static address; retry while the adapter is appearing.
    let args = netsh_set_address(device, ip, mask);
    loop {
        let (code, text) = run_tool(&netsh, &args, logger).await.map_err(|e| {
            TunSetupError::new(TunStage::Address, TunFailure::AddressFailed(root_cause(&e)))
        })?;
        if code == 0 {
            logger.info(format!(
                "[{}] ok - {device} = {ip}/{mask}",
                TunStage::Address.label()
            ));
            // Give the adapter an IPv6 address as well, so IPv6 traffic can
            // be tunnelled instead of leaking.
            let v6 = netsh_set_address_v6(device, TUN_IPV6, TUN_PREFIX_V6);
            let (code6, _) = run_tool(&netsh, &v6, logger)
                .await
                .unwrap_or((-1, String::new()));
            if code6 == 0 {
                logger.info(format!(
                    "[{}] ok - {device} = {TUN_IPV6}/64",
                    TunStage::Address.label()
                ));
            } else {
                logger.warn(format!(
                    "could not assign the IPv6 address to {device}; IPv6 will not be tunnelled"
                ));
            }
            return Ok(());
        }
        if let Some(e) = core_failure(core) {
            return Err(e);
        }
        if Instant::now() >= deadline {
            return Err(TunSetupError::new(
                TunStage::Address,
                TunFailure::AddressFailed(text),
            ));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Turn a dead core into a precise error.
fn core_failure(core: &mut XrayProcess) -> Option<TunSetupError> {
    let code = core.try_exit_code()?;
    let failure = match core.failure() {
        Some(f) => match &f {
            xray::CoreFailure::ConfigRejected(m) => TunFailure::ConfigRejected(m.clone()),
            xray::CoreFailure::WintunMissing => TunFailure::WintunMissing,
            xray::CoreFailure::Other(m) => {
                if m.to_ascii_lowercase().contains("adapter") {
                    TunFailure::AdapterFailed(m.clone())
                } else {
                    TunFailure::Other(f.message())
                }
            }
        },
        None => TunFailure::Other(format!("Xray exited with code {code}")),
    };
    Some(TunSetupError::new(TunStage::Core, failure))
}

/// The inner cause of an error (strip our own prefix).
fn root_cause(err: &anyhow::Error) -> String {
    let text = format!("{err:#}");
    match text.split_once(": ").map(|(_, r)| r) {
        Some(rest) if !rest.trim().is_empty() => rest.trim().to_string(),
        _ => text,
    }
}

/// The CPU architecture of a PE executable, read from its header.
///
/// `0x8664` = x86-64, `0x14c` = i386, `0xaa64` = ARM64. The wintun driver
/// must match the architecture of `xray.exe`, otherwise the core cannot load
/// it and the adapter is never created.
pub fn pe_machine(path: &Path) -> Option<u16> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 0x40 || &data[0..2] != b"MZ" {
        return None;
    }
    let pe_off = u32::from_le_bytes(data[0x3c..0x40].try_into().ok()?) as usize;
    if pe_off + 6 > data.len() || &data[pe_off..pe_off + 4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(
        data[pe_off + 4..pe_off + 6].try_into().ok()?,
    ))
}

fn arch_name(machine: u16) -> &'static str {
    match machine {
        0x8664 => "x86-64",
        0x14c => "i386",
        0xaa64 => "ARM64",
        _ => "unknown",
    }
}

/// Check that the wintun driver is present next to `xray.exe` and that it
/// matches the executable's architecture.
fn check_wintun(xray_exe: &Path, logger: &Logger) -> Result<()> {
    let dir = xray_exe.parent().unwrap_or(Path::new("."));
    let dll = dir.join("wintun.dll");
    if !dll.is_file() {
        logger.error(format!(
            "wintun.dll is missing next to xray.exe ({})",
            dir.display()
        ));
        return Err(TunSetupError::new(TunStage::Adapter, TunFailure::WintunMissing).into());
    }
    let exe_arch = pe_machine(xray_exe);
    let dll_arch = pe_machine(&dll);
    if let (Some(e), Some(d)) = (exe_arch, dll_arch) {
        if e != d {
            logger.error(format!(
                "wintun.dll is {} but xray.exe is {} - the driver must match the core",
                arch_name(d),
                arch_name(e)
            ));
            return Err(TunSetupError::new(TunStage::Adapter, TunFailure::WintunMissing).into());
        }
        logger.info(format!(
            "wintun driver found ({} bytes, {})",
            std::fs::metadata(&dll).map(|m| m.len()).unwrap_or(0),
            arch_name(d)
        ));
    } else {
        logger.info("wintun driver found");
    }
    Ok(())
}

/// Point the TUN adapter at public resolvers so DNS is tunnelled too.
async fn configure_dns(device: &str, logger: &Logger) -> Result<()> {
    let netsh = system32("netsh.exe");
    let args = vec![
        "interface".into(),
        "ipv4".into(),
        "set".into(),
        "dnsservers".into(),
        format!("name={device}"),
        "source=static".into(),
        "address=1.1.1.1".into(),
        "validate=no".into(),
    ];
    let (code, text) = run_tool(&netsh, &args, logger).await?;
    if code == 0 {
        logger.info(format!(
            "[{}] ok - {device} -> 1.1.1.1",
            TunStage::Dns.label()
        ));
        Ok(())
    } else {
        Err(TunSetupError::new(TunStage::Dns, TunFailure::DnsFailed(text)).into())
    }
}

/// The journal of routes we installed, so a crash cannot leave the machine
/// without connectivity.
fn route_journal_path() -> PathBuf {
    util::runtime_config_path().with_file_name("tun-routes.json")
}

/// Persist the routes we installed.
fn write_route_journal(routes: &[RouteSpec]) {
    let entries: Vec<Value> = routes
        .iter()
        .map(|r| {
            json!({
                "interface": r.on_interface,
                "prefix": format!("{}/{}", r.dest, r.prefix),
            })
        })
        .collect();
    if let Ok(text) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(route_journal_path(), text);
    }
}

/// Remove the routes recorded in the journal (crash recovery).
pub async fn clean_stale_routes(logger: &Logger) {
    // A second app launch must never run crash recovery against a tunnel that
    // is still owned by the first process. The journal is shared by both.
    if live_tun_owner(logger) {
        return;
    }
    let path = route_journal_path();
    let mut removed = 0;
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(entries) = serde_json::from_str::<Vec<Value>>(&text) {
            let netsh = system32("netsh.exe");
            for entry in entries {
                let (Some(iface), Some(prefix)) = (
                    entry.get("interface").and_then(|v| v.as_str()),
                    entry.get("prefix").and_then(|v| v.as_str()),
                ) else {
                    continue;
                };
                let (dest, plen) = match prefix.split_once('/') {
                    Some((d, p)) => (d, p.parse::<u8>().unwrap_or(32)),
                    None => (prefix, 32),
                };
                let args = vec![
                    "interface".into(),
                    if dest.contains(':') { "ipv6" } else { "ipv4" }.into(),
                    "delete".into(),
                    "route".into(),
                    format!("{dest}/{plen}"),
                    format!("interface={iface}"),
                    "store=active".into(),
                ];
                if let Ok((code, _)) = run_tool(&netsh, &args, logger).await {
                    if code == 0 {
                        removed += 1;
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_file(&path);
    if removed > 0 {
        logger.warn(format!(
            "removed {removed} TUN route(s) left behind by a previous run"
        ));
    }
    // A leftover *address* on our adapter makes the next Xray start fail
    // (the wintun stack cannot be initialised twice), so it is cleared too.
    clear_stale_address(DEVICE_NAME, logger).await;
}

/// Remove the addresses we assigned to our adapter (crash recovery).
pub async fn clear_stale_address(device: &str, logger: &Logger) {
    let netsh = system32("netsh.exe");
    for (family, addr) in [("ipv4", TUN_IP), ("ipv6", TUN_IPV6)] {
        let args = vec![
            "interface".into(),
            family.into(),
            "delete".into(),
            "address".into(),
            format!("name={device}"),
            format!("addr={addr}"),
            "store=active".into(),
        ];
        if let Ok((code, _)) = run_tool(&netsh, &args, logger).await {
            if code == 0 {
                logger.warn(format!(
                    "cleared the stale {family} address {addr} on '{device}' from a previous run"
                ));
            }
        }
    }
}

/// How many of our routes are currently installed: `(ipv4, ipv6)`.
///
/// Used by the full-tunnel verification to prove that the system routing table
/// really points into the TUN. `None` on platforms without a Windows routing
/// table (TUN mode is a Windows feature).
#[cfg(windows)]
pub async fn installed_routes() -> Option<(usize, usize)> {
    let route_exe = system32("route.exe");
    let logger = Logger::new();
    let mut v4 = 0;
    let mut v6 = 0;
    for (flag, gateway, counter) in [
        ("-4", active_tun_gateway(), &mut v4),
        ("-6", TUN_IPV6, &mut v6),
    ] {
        let args = vec!["print".into(), flag.into()];
        if let Ok((code, text)) = run_tool(&route_exe, &args, &logger).await {
            if code == 0 {
                // Count the split routes that point at our TUN gateway.
                *counter = if flag == "-6" {
                    split_routes_into_tun_v6(&text, gateway)
                } else {
                    split_routes_into_tun(&text, gateway)
                };
            }
        }
    }
    Some((v4, v6))
}

/// The routing table is a Windows feature.
#[cfg(not(windows))]
pub async fn installed_routes() -> Option<(usize, usize)> {
    None
}

/// Does the system route to `ip` lead through the TUN adapter?
///
/// This is the deterministic half of the full-tunnel proof: it shows that the
/// host the measurement uses is reached through the tunnel, no matter what the
/// other programs on the machine route.
///
/// `None` means the platform does not support the check (TUN mode is a Windows
/// feature), `Some(false)` means the route does *not* lead into the TUN.
#[cfg(windows)]
pub async fn route_leads_into_tun(ip: &str) -> Option<bool> {
    let route_exe = system32("route.exe");
    let logger = Logger::new();
    let args = vec!["print".into(), "-4".into()];
    if let Ok((code, text)) = run_tool(&route_exe, &args, &logger).await {
        if code == 0 {
            if let Some((gw, iface)) = best_route_for(ip, &text) {
                // A route into the TUN either names the TUN address as its
                // gateway or is "on-link" on the TUN adapter.
                return Some(
                    gw.eq_ignore_ascii_case(active_tun_gateway())
                        || iface.eq_ignore_ascii_case(active_tun_ip()),
                );
            }
        }
    }
    None
}

/// The check needs the Windows routing table.
#[cfg(not(windows))]
pub async fn route_leads_into_tun(_ip: &str) -> Option<bool> {
    None
}

/// The split routes (the /1 pairs) that point into the TUN adapter.
///
/// The count is what the dashboard shows as "TUN routes installed": the system
/// traffic can only reach the tunnel if those routes are there.
pub fn split_routes_into_tun(route_print: &str, tun_ip: &str) -> usize {
    route_print
        .lines()
        .filter(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            if t.len() < 4 {
                return false;
            }
            let into_tun = t[2].eq_ignore_ascii_case(tun_ip) || t[3].eq_ignore_ascii_case(tun_ip);
            if !into_tun {
                return false;
            }
            // Only the split routes count, not the on-link routes of the
            // adapter itself.
            (t[0] == "0.0.0.0" && t[1] == "128.0.0.0")
                || (t[0] == "128.0.0.0" && t[1] == "128.0.0.0")
                || t[0].ends_with("/1")
        })
        .count()
}

/// The IPv6 split routes that point into the TUN adapter.
///
/// `route print -6` uses a different layout than the IPv4 table: the columns
/// are `interface index, metric, destination, gateway`.
pub fn split_routes_into_tun_v6(route_print: &str, tun_ip: &str) -> usize {
    route_print
        .lines()
        .filter(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            if t.len() < 4 {
                return false;
            }
            t[2].ends_with("/1") && t[3].eq_ignore_ascii_case(tun_ip)
        })
        .count()
}

/// The lock file that keeps two TUN instances from fighting over the adapter.
fn tun_lock_path() -> PathBuf {
    util::app_dir().join("tun.lock")
}

/// Is the process with this id an instance of *this* program?
///
/// A PID on its own is not enough: after a crash the PID can be reused by an
/// unrelated program, which would make a stale lock look like a live one.
#[cfg(windows)]
fn process_is_our_instance(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    let want = current.to_string_lossy().to_ascii_lowercase();

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if !h.is_null() {
                        let mut buf = [0u16; 260];
                        let mut len = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
                            let path = String::from_utf16_lossy(&buf[..len as usize]);
                            found = path.to_ascii_lowercase() == want;
                        }
                        CloseHandle(h);
                    }
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(not(windows))]
fn process_is_our_instance(pid: u32) -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    match std::fs::read_link(format!("/proc/{pid}/exe")) {
        Ok(path) => path == current,
        Err(_) => false,
    }
}

/// Terminate the cores of a previous, crashed run (they would keep the wintun
/// adapter locked and the next start would fail).
fn kill_stale_cores(logger: &Logger) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };

        let exe = xray_path();
        let want = exe.to_string_lossy().to_ascii_lowercase();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut killed = 0;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    let mut buf = [0u16; 260];
                    let h = OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                        0,
                        entry.th32ProcessID,
                    );
                    if !h.is_null() {
                        let mut len = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
                            let path = String::from_utf16_lossy(&buf[..len as usize]);
                            if path.to_ascii_lowercase() == want {
                                logger.warn(format!(
                                    "terminating a leftover core from a previous run (pid {})",
                                    entry.th32ProcessID
                                ));
                                TerminateProcess(h, 1);
                                killed += 1;
                            }
                        }
                        CloseHandle(h);
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            if killed > 0 {
                logger.warn(format!("cleaned up {killed} leftover core process(es)"));
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = logger;
    }
}

/// Take the TUN lock. A live lock means another instance owns the adapter.
fn acquire_tun_lock(logger: &Logger) -> Result<()> {
    let path = tun_lock_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(pid) = text.trim().parse::<u32>() {
            // Only a *live instance of this program* owns the adapter; a
            // reused PID must not block the start.
            if pid != std::process::id() && process_is_our_instance(pid) {
                bail!(
                    "another SNI Tunnel instance already owns the TUN adapter (pid {pid}) - \
                     stop it before starting another tunnel"
                );
            }
        }
        // The lock is stale: a previous run crashed. Clean up its leftovers:
        // the core processes that keep the wintun device locked and the DNS
        // configuration that the crashed run had hijacked.
        kill_stale_cores(logger);
        restore_dns_from_journal(logger);
        let _ = std::fs::remove_file(&path);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, std::process::id().to_string()).context("write the TUN lock file")?;
    Ok(())
}

/// Release the TUN lock.
fn release_tun_lock() {
    let _ = std::fs::remove_file(tun_lock_path());
}

/// The DNS configuration of an adapter.
#[derive(Clone, Debug, Default)]
pub struct DnsBackup {
    pub interface: String,
    pub servers: Vec<String>,
    /// The servers came from DHCP (and are restored that way).
    pub dhcp: bool,
}

/// Read the DNS servers of an adapter (`netsh interface ipv4 show dnsservers`).
///
/// The output has two shapes:
///   "DNS servers configured through DHCP:  192.168.1.1" (the addresses
///   continue on the following lines) or the older
///   "Configure DNS Server: DHCP" / "IPv4 Address: 192.168.1.1".
fn read_dns_servers(iface: &str) -> DnsBackup {
    let mut backup = DnsBackup {
        interface: iface.to_string(),
        ..Default::default()
    };
    let netsh = system32("netsh.exe");
    let args = vec![
        "interface".into(),
        "ipv4".into(),
        "show".into(),
        "dnsservers".into(),
        format!("name={iface}"),
    ];
    let Ok((code, text)) = run_tool_blocking(&netsh, &args) else {
        return backup;
    };
    if code != 0 {
        return backup;
    }
    let mut collecting = false;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("Configuration for interface") {
            continue;
        }
        if t.contains("configured through DHCP") {
            backup.dhcp = true;
            collecting = true;
            collect_ips(t, &mut backup.servers);
            continue;
        }
        if t.contains("Statically Configured DNS Servers") {
            backup.dhcp = false;
            collecting = true;
            collect_ips(t, &mut backup.servers);
            continue;
        }
        if t.contains("Configure DNS Server:") {
            backup.dhcp = t.contains("DHCP");
            collecting = false;
            continue;
        }
        if let Some(rest) = t.strip_prefix("IPv4 Address:") {
            let ip = rest.trim();
            if !ip.is_empty() && ip != "None" {
                backup.servers.push(ip.to_string());
            }
            continue;
        }
        if collecting && is_ip(t) {
            backup.servers.push(t.to_string());
        }
    }
    backup
}

/// Extract the addresses that follow a header line.
fn collect_ips(text: &str, out: &mut Vec<String>) {
    let Some((_, tail)) = text.split_once(':') else {
        return;
    };
    for part in tail.split(',') {
        let p = part.trim();
        if is_ip(p) {
            out.push(p.to_string());
        }
    }
}

/// Is this an IPv4 address?
fn is_ip(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Set the DNS servers of an adapter.
fn write_dns_servers(iface: &str, servers: &[String], dhcp: bool) -> bool {
    let netsh = system32("netsh.exe");
    let mut args = vec![
        "interface".into(),
        "ipv4".into(),
        "set".into(),
        "dnsservers".into(),
        format!("name={iface}"),
    ];
    if dhcp {
        args.push("source=dhcp".into());
    } else {
        args.push("source=static".into());
        args.push(format!("address={}", servers.join(",")));
        args.push("validate=no".into());
    }
    run_tool_blocking(&netsh, &args)
        .map(|(code, _)| code == 0)
        .unwrap_or(false)
}

/// Run a command synchronously (used for the DNS handling, which runs before
/// the tunnel is up).
fn run_tool_blocking(exe: &Path, args: &[String]) -> Result<(i32, String)> {
    let out = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("run {}", exe.display()))?;
    let code = out.status.code().unwrap_or(-1);
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((code, text.trim().to_string()))
}

/// The names of the adapters that are connected (the TUN one is skipped).
fn connected_adapters() -> Vec<String> {
    let netsh = system32("netsh.exe");
    let args = vec![
        "interface".into(),
        "ipv4".into(),
        "show".into(),
        "interfaces".into(),
    ];
    let Ok((code, text)) = run_tool_blocking(&netsh, &args) else {
        return Vec::new();
    };
    if code != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.contains('=') {
            continue;
        }
        // "15   50   1500  connected  Wi-Fi" - the state column position
        // depends on whether an MTU column is present, so look for the word.
        let cols: Vec<&str> = t.split_whitespace().collect();
        let Some(pos) = cols
            .iter()
            .position(|c| c.eq_ignore_ascii_case("connected"))
        else {
            continue;
        };
        let name = cols[pos + 1..].join(" ");
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// Point the DNS of the physical adapters into the tunnel.
///
/// Windows resolves a name by asking *all* the configured resolvers in
/// parallel, so the physical resolver would answer directly and the DNS
/// traffic would leave the machine outside the tunnel. The previous setting is
/// recorded so it can be restored.
fn hijack_dns(logger: &Logger) -> Vec<DnsBackup> {
    let mut backups = Vec::new();
    let adapters = connected_adapters();
    logger.debug(format!("DNS hijack: connected adapters = {adapters:?}"));
    for iface in adapters {
        if iface == DEVICE_NAME {
            continue;
        }
        let backup = read_dns_servers(&iface);
        logger.info(format!(
            "DNS of '{iface}': servers={:?} dhcp={}",
            backup.servers, backup.dhcp
        ));
        if backup.servers.is_empty() {
            continue;
        }
        // A resolver that is already the tunnel's own does not need to change.
        if backup.servers.iter().all(|s| s == "1.1.1.1") {
            continue;
        }
        if write_dns_servers(&iface, &["1.1.1.1".to_string()], false) {
            logger.info(format!(
                "DNS of '{iface}' now goes through the tunnel (was {})",
                backup.servers.join(", ")
            ));
            backups.push(backup);
        }
    }
    backups
}

/// Restore the DNS configuration that `hijack_dns` replaced.
fn restore_dns(backups: &[DnsBackup], logger: &Logger) {
    for b in backups {
        let ok = if b.dhcp {
            write_dns_servers(&b.interface, &[], true)
        } else {
            write_dns_servers(&b.interface, &b.servers, false)
        };
        if ok {
            logger.info(format!("DNS of '{}' restored", b.interface));
        }
    }
}

/// The journal of the DNS configuration we replaced.
fn dns_journal_path() -> PathBuf {
    util::runtime_config_path().with_file_name("tun-dns.json")
}

/// Remember the DNS configuration that was replaced.
fn write_dns_journal(backups: &[DnsBackup]) {
    let entries: Vec<Value> = backups
        .iter()
        .map(|b| {
            json!({
                "interface": b.interface,
                "servers": b.servers,
                "dhcp": b.dhcp,
            })
        })
        .collect();
    if let Ok(text) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(dns_journal_path(), text);
    }
}

/// Restore the DNS configuration of a crashed run.
pub fn restore_dns_from_journal(logger: &Logger) {
    // Recovery is for crashed sessions only. Never restore DNS underneath a
    // different live SNI Tunnel process.
    if live_tun_owner(logger) {
        return;
    }
    let path = dns_journal_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(entries) = serde_json::from_str::<Vec<Value>>(&text) else {
        let _ = std::fs::remove_file(&path);
        return;
    };
    let mut backups = Vec::new();
    for e in entries {
        let Some(interface) = e.get("interface").and_then(|v| v.as_str()) else {
            continue;
        };
        let servers = e
            .get("servers")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let dhcp = e.get("dhcp").and_then(|v| v.as_bool()).unwrap_or(false);
        backups.push(DnsBackup {
            interface: interface.to_string(),
            servers,
            dhcp,
        });
    }
    restore_dns(&backups, logger);
    let _ = std::fs::remove_file(&path);
}

/// Returns true when another live copy of this application owns the shared
/// TUN/DNS journals. A stale PID is safe to recover from; a matching live PID
/// means cleanup would tear down an active session.
fn live_tun_owner(logger: &Logger) -> bool {
    let Ok(text) = std::fs::read_to_string(tun_lock_path()) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    if pid != std::process::id() && process_is_our_instance(pid) {
        logger.info(format!(
            "skipping stale-session recovery because SNI Tunnel is still connected (pid {pid})"
        ));
        true
    } else {
        false
    }
}

/// The traffic counters of the *physical* uplink adapter.
///
/// They are the reference for the leak check: while the tunnel is up, an
/// ordinary request must not grow them.
pub async fn uplink_traffic() -> Option<(u64, u64)> {
    let (alias, _gw, _ip) = discover_uplink(&Logger::new()).await.ok()?;
    adapter_traffic(&alias)
}

/// Read the traffic counters of the TUN adapter by its alias.
///
/// These are the real bytes that went through the tunnel (the adapter is the
/// only path for tunnelled traffic), so the dashboard can show actual speeds
/// instead of zeros.
#[cfg(windows)]
pub fn adapter_traffic(device: &str) -> Option<(u64, u64)> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        ConvertInterfaceAliasToLuid, GetIfEntry2, MIB_IF_ROW2,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;

    unsafe {
        // The alias is resolved to the interface LUID directly: enumerating
        // the interface table does not reliably list wintun adapters.
        let alias: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
        let mut luid: NET_LUID_LH = std::mem::zeroed();
        if ConvertInterfaceAliasToLuid(alias.as_ptr(), &mut luid) != 0 {
            log::info!("adapter_traffic: no interface named '{device}'");
            return None;
        }
        let mut entry: MIB_IF_ROW2 = std::mem::zeroed();
        entry.InterfaceLuid = luid;
        if GetIfEntry2(&mut entry) != 0 {
            log::info!("adapter_traffic: could not read the counters of '{device}'");
            return None;
        }
        Some((entry.InOctets, entry.OutOctets))
    }
}

/// The traffic counters are only available on Windows.
#[cfg(not(windows))]
pub fn adapter_traffic(_device: &str) -> Option<(u64, u64)> {
    None
}

/// Install the routes and return the ones that were really created.
///
/// A route that was already there is *not* returned: the app must not remove
/// it again on disconnect, because it belongs to another program.
async fn apply_routes(routes: &[RouteSpec], logger: &Logger) -> Result<Vec<RouteSpec>> {
    let netsh = system32("netsh.exe");
    let route = system32("route.exe");
    let mut installed: Vec<RouteSpec> = Vec::new();
    for spec in routes {
        let args = netsh_add_route(
            &spec.on_interface,
            &spec.dest,
            spec.prefix,
            spec.via_gateway.as_deref(),
            spec.ipv6,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (code, text) = run_tool(&netsh, &args, logger)
                .await
                .unwrap_or((-1, String::new()));
            if code == 0 {
                installed.push(spec.clone());
                break;
            }
            // Something already routes to that destination. Two cases:
            //  * a route that already leads over the physical network - for
            //    the server exclusion that is exactly what is wanted (the
            //    server has to stay reachable outside the tunnel), so it is
            //    kept and it is *not* removed again on disconnect,
            //  * a leftover of an earlier run of this app - it is removed and
            //    the correct route is added again.
            if text.to_ascii_lowercase().contains("already exists") {
                let mut removed_own = false;
                if spec.prefix == 32 && !spec.ipv6 {
                    let print = run_tool_blocking(&route, &["print".into(), "-4".into()])
                        .map(|(_, t)| t)
                        .unwrap_or_default();
                    if let Some(iface) = route_interface_for(&spec.dest, spec.prefix, &print) {
                        if iface != TUN_IP {
                            logger.info(format!(
                                "the route to {}/{} already exists over {iface} - keeping it",
                                spec.dest, spec.prefix
                            ));
                            break;
                        }
                        // A leftover of this app on the TUN adapter: it would
                        // send the server traffic back into the tunnel.
                        let del = netsh_delete_route(DEVICE_NAME, &spec.dest, spec.prefix, false);
                        let _ = run_tool(&netsh, &del, logger).await;
                        removed_own = true;
                    }
                }
                if !removed_own {
                    // Remove the leftover of this app (only on our own
                    // interface, the routes of other programs stay untouched)
                    // and try again.
                    let del =
                        netsh_delete_route(&spec.on_interface, &spec.dest, spec.prefix, spec.ipv6);
                    let _ = run_tool(&netsh, &del, logger).await;
                }
                let (code2, _) = run_tool(&netsh, &args, logger)
                    .await
                    .unwrap_or((-1, String::new()));
                if code2 == 0 {
                    installed.push(spec.clone());
                    break;
                }
            }
            if Instant::now() >= deadline {
                // Do not leave half installed routes behind.
                let _ = remove_routes(&installed, logger).await;
                bail!("route add {}/{} failed: {text}", spec.dest, spec.prefix);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    logger.info(format!("Installed {} TUN route(s)", installed.len()));
    Ok(installed)
}

async fn remove_routes(routes: &[RouteSpec], logger: &Logger) -> Result<()> {
    let netsh = system32("netsh.exe");
    let mut removed = 0;
    for spec in routes {
        let args = netsh_delete_route(&spec.on_interface, &spec.dest, spec.prefix, spec.ipv6);
        if let Ok((code, _)) = run_tool(&netsh, &args, logger).await {
            if code == 0 {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        logger.info(format!("Removed {removed} TUN route(s)"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tun_inbound_shape() {
        let i = tun_inbound("sni-tunnel", 1500);
        assert_eq!(i["tag"], "tun-in");
        assert_eq!(i["protocol"], "tun");
        assert_eq!(i["settings"]["name"], "sni-tunnel");
        assert_eq!(i["settings"]["mtu"], 1500);
    }

    #[test]
    fn tun_config_composes_inbound_and_outbound() {
        let cfg = tun_config(
            "sni-tunnel",
            1500,
            bind_outbound_interface(json!({ "tag": "proxy", "protocol": "freedom" }), "Wi-Fi"),
            "warning",
        );
        assert_eq!(cfg["log"]["loglevel"], "warning");
        assert_eq!(cfg["inbounds"][0]["protocol"], "tun");
        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["tag"], "proxy");
        assert_eq!(ob["protocol"], "freedom");
        assert_eq!(ob["streamSettings"]["sockopt"]["interface"], "Wi-Fi");
    }

    #[test]
    fn binding_merges_with_existing_stream_settings() {
        let ob = bind_outbound_interface(json!({ "tag": "proxy" }), "Ethernet");
        assert_eq!(ob["streamSettings"]["sockopt"]["interface"], "Ethernet");

        let ob = bind_outbound_interface(
            json!({ "streamSettings": { "security": "tls" } }),
            "Ethernet",
        );
        assert_eq!(ob["streamSettings"]["security"], "tls");
        assert_eq!(ob["streamSettings"]["sockopt"]["interface"], "Ethernet");
    }

    #[test]
    fn netsh_commands() {
        assert_eq!(
            netsh_set_address("sni-tunnel", "198.18.0.1", "255.255.255.0"),
            vec![
                "interface",
                "ipv4",
                "set",
                "address",
                "name=sni-tunnel",
                "source=static",
                "addr=198.18.0.1",
                "mask=255.255.255.0",
                "store=active"
            ]
        );
        assert_eq!(
            netsh_disable_dad("sni-tunnel"),
            vec![
                "interface",
                "ipv4",
                "set",
                "interface",
                "interface=sni-tunnel",
                "dadtransmits=0",
                "store=active"
            ]
        );
    }

    #[test]
    fn route_commands() {
        assert_eq!(
            netsh_add_route("sni-tunnel", "0.0.0.0", 1, Some("198.18.0.1"), false),
            vec![
                "interface",
                "ipv4",
                "add",
                "route",
                "0.0.0.0/1",
                "interface=sni-tunnel",
                "nexthop=198.18.0.1",
                "metric=1",
                "store=active"
            ]
        );
        assert_eq!(
            netsh_delete_route("sni-tunnel", "1.1.1.1", 32, false),
            vec![
                "interface",
                "ipv4",
                "delete",
                "route",
                "1.1.1.1/32",
                "interface=sni-tunnel",
                "store=active"
            ]
        );
    }

    /// A route table as `route print -4` prints it while the tunnel is up.
    const ROUTE_TABLE: &str = "\
===========================================================================
Interface List
 16...a8 93 4a 2b 19 9f ......MediaTek Wi-Fi 6 MT7921 Wireless LAN Card
 11...........................Xray Tunnel
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1     192.168.1.17     50
          0.0.0.0        128.0.0.0         On-link        198.18.0.1      6
     51.91.248.209  255.255.255.255      192.168.1.1     192.168.1.17     50
        128.0.0.0        128.0.0.0         On-link        198.18.0.1      6
       198.18.0.0    255.255.255.0         On-link        198.18.0.1    261
       198.18.0.1  255.255.255.255         On-link        198.18.0.1    261
===========================================================================
Persistent Routes:
  None
";

    /// The IPv6 table as `route print -6` prints it (a different layout).
    const ROUTE_TABLE_V6: &str = "\
IPv6 Route Table
===========================================================================
Active Routes:
 If Metric Network Destination      Gateway
 11      6 ::/1                     fc00::1
  1    331 ::1/128                  On-link
 11      6 8000::/1                 fc00::1
 16     66 fdd4:6ba6:80d7:b400::/64 On-link
===========================================================================
";

    #[test]
    fn the_split_routes_are_counted() {
        assert_eq!(split_routes_into_tun(ROUTE_TABLE, TUN_IP), 2);
        assert_eq!(split_routes_into_tun_v6(ROUTE_TABLE_V6, TUN_IPV6), 2);
    }

    #[test]
    fn the_measured_host_is_routed_into_the_tun() {
        let (gw, iface) = best_route_for("104.26.12.205", ROUTE_TABLE).expect("a route");
        assert!(
            gw.eq_ignore_ascii_case(TUN_IP) || iface.eq_ignore_ascii_case(TUN_IP),
            "the route leads into {gw} / {iface}"
        );
    }

    #[test]
    fn the_server_is_not_routed_into_the_tun() {
        let (gw, iface) = best_route_for("51.91.248.209", ROUTE_TABLE).expect("a route");
        assert_eq!(iface, "192.168.1.17");
        assert_eq!(gw, "192.168.1.1");
    }

    #[test]
    fn an_existing_route_is_recognised() {
        assert_eq!(
            route_interface_for("51.91.248.209", 32, ROUTE_TABLE).as_deref(),
            Some("192.168.1.17")
        );
        assert_eq!(route_interface_for("1.2.3.4", 32, ROUTE_TABLE), None);
    }

    #[test]
    fn the_longest_prefix_wins() {
        // 10.0.0.0/8 (a more specific route) has to beat the default route.
        let table = "\
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1     192.168.1.17     50
         10.0.0.0        255.0.0.0       10.4.2.2         10.4.2.1     49
";
        let (gw, iface) = best_route_for("10.1.2.3", table).expect("a route");
        assert_eq!(gw, "10.4.2.2");
        assert_eq!(iface, "10.4.2.1");
    }

    #[test]
    fn mask_conversion() {
        assert_eq!(mask_from_prefix(24).unwrap(), "255.255.255.0");
        assert_eq!(mask_from_prefix(32).unwrap(), "255.255.255.255");
        assert_eq!(mask_from_prefix(1).unwrap(), "128.0.0.0");
        assert_eq!(mask_from_prefix(0).unwrap(), "0.0.0.0");
        assert!(mask_from_prefix(33).is_none());
    }

    #[test]
    fn route_specs_split_and_hosts() {
        let r = split_default_routes("sni-tunnel");
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].dest, "0.0.0.0");
        assert_eq!(r[0].prefix, 1);
        assert_eq!(r[0].via_gateway, Some("198.18.0.1".into()));
        assert_eq!(r[0].on_interface, "sni-tunnel");
        assert!(!r[0].ipv6);
        assert_eq!(r[1].dest, "128.0.0.0");
        assert_eq!(r[1].on_interface, "sni-tunnel");
        assert!(!r[1].ipv6);
        assert_eq!(r[2].dest, "::");
        assert_eq!(r[2].prefix, 1);
        assert_eq!(r[2].on_interface, "sni-tunnel");
        assert!(r[2].ipv6);
        assert_eq!(r[3].dest, "8000::");
        assert_eq!(r[3].prefix, 1);
        assert_eq!(r[3].on_interface, "sni-tunnel");
        assert!(r[3].ipv6);

        let h = host_routes(&["1.1.1.1".into(), "9.9.9.9".into()], "sni-tunnel");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].dest, "1.1.1.1");
        assert_eq!(h[0].prefix, 32);
        assert_eq!(h[1].on_interface, "sni-tunnel");
        assert!(h.iter().all(|r| !r.ipv6));
        let routes = build_routes(
            &RouteMode::DefaultWithExclusions(vec!["9.9.9.9".into()]),
            "sni-tunnel",
            "PdaNet",
            Some("10.1.19.1"),
        );
        assert_eq!(routes.len(), 5);
        let ex = &routes[4];
        assert_eq!(ex.dest, "9.9.9.9");
        assert_eq!(ex.prefix, 32);
        assert_eq!(ex.on_interface, "PdaNet");
        assert_eq!(ex.via_gateway, Some("10.1.19.1".into()));
        assert!(!ex.ipv6);
    }

    #[test]
    fn parses_default_route_row() {
        let sample = "\
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1    192.168.1.50     25
        127.0.0.0        255.0.0.0         On-link         127.0.0.1    331
";
        assert_eq!(
            parse_default_route(sample),
            Some((Some("192.168.1.1".into()), "192.168.1.50".into()))
        );
    }

    #[test]
    fn on_link_default_route_has_no_gateway() {
        let sample = "          0.0.0.0          0.0.0.0         On-link     10.0.0.5     25\n";
        assert_eq!(parse_default_route(sample), Some((None, "10.0.0.5".into())));
        assert!(parse_default_route("nothing here").is_none());
    }

    #[test]
    fn parses_alias_for_ip() {
        let sample = "Configuration for interface \"Ethernet 5\"\n    DHCP enabled: Yes\n    IP Address: 192.168.1.3\n\nConfiguration for interface \"PdaNet Broadband Connection\"\n    IP Address: 10.1.19.2\n";
        assert_eq!(
            parse_alias_for_ip(sample, "10.1.19.2"),
            Some("PdaNet Broadband Connection".to_string())
        );
        assert_eq!(
            parse_alias_for_ip(sample, "192.168.1.3"),
            Some("Ethernet 5".to_string())
        );
        assert!(parse_alias_for_ip(sample, "8.8.8.8").is_none());
    }

    #[test]
    fn parses_trace_egress_ip() {
        let body = "fl=1\nh=1.1.1.1\nip=203.0.113.7\nts=123\n";
        assert_eq!(parse_trace_ip(body), Some("203.0.113.7".to_string()));
        assert!(parse_trace_ip("no ip line").is_none());
    }

    #[test]
    fn parses_connected_interfaces() {
        let sample = "\
Admin State    State          Type             Interface Name
-------------------------------------------------------------------------
Enabled        Disconnected   Dedicated        Ethernet 3
Enabled        Connected      Dedicated        Ethernet 4
Enabled        Disconnected   Dedicated        PdaNet Broadband Connection
Enabled        Connected      Dedicated        Wi-Fi
Enabled        Connected      Dedicated        CloudflareWARP
";
        assert_eq!(
            parse_connected_interfaces(sample),
            vec![
                "Ethernet 4".to_string(),
                "Wi-Fi".to_string(),
                "CloudflareWARP".to_string()
            ]
        );
        assert!(parse_connected_interfaces("").is_empty());
    }

    #[test]
    fn detects_vpn_client_adapters() {
        assert!(looks_like_vpn_client("CloudflareWARP"));
        assert!(looks_like_vpn_client("WireGuard Tunnel"));
        assert!(looks_like_vpn_client("NordLynx Tunnel"));
        assert!(!looks_like_vpn_client("Wi-Fi"));
        assert!(!looks_like_vpn_client("Ethernet 4"));
        assert!(!looks_like_vpn_client("sni-tunnel"));
    }

    #[test]
    fn pe_machine_reads_the_architecture() {
        // A minimal PE header is enough for the reader.
        let mut data = vec![0u8; 0x50];
        data[0..2].copy_from_slice(b"MZ");
        data[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        data[0x40..0x44].copy_from_slice(b"PE\0\0");
        data[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
        let path = std::env::temp_dir().join("sni-tunnel-pe-test.exe");
        std::fs::write(&path, &data).unwrap();
        assert_eq!(pe_machine(&path), Some(0x8664));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pe_machine_rejects_non_pe_files() {
        let path = std::env::temp_dir().join("sni-tunnel-pe-test.bin");
        std::fs::write(&path, b"not a pe file at all").unwrap();
        assert_eq!(pe_machine(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_failure_has_a_specific_message() {
        let cases = [
            (TunFailure::NotElevated, "Administrator"),
            (TunFailure::WintunMissing, "wintun"),
            (TunFailure::ConfigRejected("bad json".into()), "bad json"),
            (TunFailure::AdapterFailed("no driver".into()), "adapter"),
            (TunFailure::AddressFailed("no address".into()), "address"),
            (TunFailure::DnsFailed("no dns".into()), "DNS"),
            (TunFailure::RouteFailed("no route".into()), "route"),
            (TunFailure::VpnConflict("WARP".into()), "WARP"),
            (
                TunFailure::ForwardingFailed("no traffic".into()),
                "no traffic",
            ),
            (TunFailure::Other("something".into()), "something"),
        ];
        for (f, needle) in cases {
            let msg = f.message();
            assert!(msg.contains(needle), "{msg} should mention {needle}");
        }
    }

    #[test]
    fn setup_error_names_the_stage() {
        let e = TunSetupError::new(TunStage::Routes, TunFailure::RouteFailed("busy".into()));
        assert_eq!(e.stage.label(), "routing configuration");
        assert!(e.to_string().contains("busy"));
    }

    #[test]
    fn stage_labels_cover_the_whole_setup() {
        let labels: Vec<&str> = [
            TunStage::Config,
            TunStage::Core,
            TunStage::Adapter,
            TunStage::Address,
            TunStage::Dns,
            TunStage::Routes,
            TunStage::Forwarding,
        ]
        .iter()
        .map(|s| s.label())
        .collect();
        assert_eq!(labels.len(), 7);
        assert!(labels.iter().all(|l| !l.is_empty()));
    }
}

#[cfg(test)]
mod timeout_regression_tests {
    #[tokio::test]
    async fn forwarding_retry_timeout_bounds_the_entire_operation() {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::verify_through_tun("http://127.0.0.1:9", std::time::Duration::from_millis(20)),
        )
        .await
        .expect("retries must not multiply the caller's timeout");
        assert!(result.err().unwrap().to_string().contains("total timeout"));
    }
}
