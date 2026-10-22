//! Explicit experiment using the installed NetMod TAP/tun2socks datapath.
use crate::{compat, logging::Logger, util};
use anyhow::{Context, Result};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

pub const IP: &str = "10.4.2.1";
pub const GATEWAY: &str = "10.4.2.2";
fn journal() -> PathBuf {
    util::app_dir().join("tap-adapter-backup.json")
}

// Use the interface GUID in the journal, never an interpolated shell command.
const RESTORE: &str = r#"
$ErrorActionPreference='Stop'
if (!(Test-Path -LiteralPath $env:SNI_TAP_JOURNAL)) { exit 0 }
$s=Get-Content -LiteralPath $env:SNI_TAP_JOURNAL -Raw | ConvertFrom-Json
$a=Get-NetAdapter | Where-Object { $_.InterfaceGuid.ToString() -eq $s.Guid }
if (!$a) { throw 'Saved TAP adapter is missing; retaining recovery journal' }
$idx=$a.ifIndex
if ($s.V6Added) { Get-NetIPAddress -InterfaceIndex $idx -IPAddress 'fc00::1' -ErrorAction SilentlyContinue | Remove-NetIPAddress -PolicyStore ActiveStore -Confirm:$false }
Get-NetIPAddress -InterfaceIndex $idx -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.IPAddress -eq '10.4.2.1' } | Remove-NetIPAddress -PolicyStore ActiveStore -Confirm:$false
Set-NetIPInterface -PolicyStore ActiveStore -InterfaceIndex $idx -AddressFamily IPv4 -Dhcp $s.Dhcp -NlMtuBytes $s.Mtu
if ($s.Dhcp -eq 'Disabled') {
 foreach($ip in $s.Addresses) {
  if (!(Get-NetIPAddress -InterfaceIndex $idx -IPAddress $ip.IPAddress -ErrorAction SilentlyContinue)) {
   New-NetIPAddress -PolicyStore ActiveStore -InterfaceIndex $idx -IPAddress $ip.IPAddress -PrefixLength $ip.PrefixLength | Out-Null
  }
 }
}
if ($s.StaticDns) { Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses @($s.Dns) }
else { Set-DnsClientServerAddress -InterfaceIndex $idx -ResetServerAddresses }
Remove-Item -LiteralPath $env:SNI_TAP_JOURNAL
"#;

const CHECK_OWNER: &str = r#"
$ErrorActionPreference='Stop'
$own=$env:SNI_TUN2SOCKS_EXE
$procs=@(Get-CimInstance Win32_Process -Filter "Name='badvpn-tun2socks.exe'" -ErrorAction SilentlyContinue)
foreach($p in $procs) {
 $image=$p.ExecutablePath
 if ($own -and $image -and [IO.Path]::GetFullPath($image).Equals([IO.Path]::GetFullPath($own),[StringComparison]::OrdinalIgnoreCase)) {
  Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
 } else {
  throw 'Another application owns the TAP forwarding process; close that VPN before connecting'
 }
}
"#;

const PREPARE: &str = r#"
$ErrorActionPreference='Stop'
$a=@(Get-NetAdapter | Where-Object { $_.InterfaceDescription -like 'TAP-Windows Adapter V9*' })
if ($a.Count -ne 1) { throw 'TAP test requires exactly one installed NetMod TAP-Windows Adapter V9' }
$a=$a[0]; $idx=$a.ifIndex
if (Get-NetRoute -InterfaceIndex $idx -ErrorAction SilentlyContinue | Where-Object { $_.DestinationPrefix -in @('0.0.0.0/1','128.0.0.0/1') }) { throw 'TAP adapter still has VPN routes; disconnect its owner first' }
$ipif=Get-NetIPInterface -InterfaceIndex $idx -AddressFamily IPv4
$ips=@(Get-NetIPAddress -InterfaceIndex $idx -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.PrefixOrigin -eq 'Manual' } | Select-Object IPAddress,PrefixLength)
$dns=(Get-DnsClientServerAddress -InterfaceIndex $idx -AddressFamily IPv4).ServerAddresses
$reg=Get-ItemProperty -LiteralPath ('HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\'+([guid]$a.InterfaceGuid).ToString('B'))
@{V6Added=!(Get-NetIPAddress -InterfaceIndex $idx -IPAddress 'fc00::1' -ErrorAction SilentlyContinue);Guid=$a.InterfaceGuid.ToString();Dhcp=$ipif.Dhcp.ToString();Mtu=$ipif.NlMtu;Addresses=$ips;Dns=@($dns);StaticDns=!!$reg.NameServer} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $env:SNI_TAP_JOURNAL
Set-NetIPInterface -PolicyStore ActiveStore -InterfaceIndex $idx -AddressFamily IPv4 -Dhcp Disabled -NlMtuBytes 1500
foreach($ip in $ips) { Remove-NetIPAddress -PolicyStore ActiveStore -InterfaceIndex $idx -IPAddress $ip.IPAddress -Confirm:$false }
New-NetIPAddress -PolicyStore ActiveStore -InterfaceIndex $idx -IPAddress '10.4.2.1' -PrefixLength 24 | Out-Null
if (!(Get-NetIPAddress -InterfaceIndex $idx -IPAddress 'fc00::1' -ErrorAction SilentlyContinue)) { New-NetIPAddress -PolicyStore ActiveStore -InterfaceIndex $idx -IPAddress 'fc00::1' -PrefixLength 64 | Out-Null }
Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses @('8.8.8.8','8.8.4.4')
$a.Name
"#;

fn command(script: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("SNI_TAP_JOURNAL", journal())
        .env(
            "SNI_TUN2SOCKS_EXE",
            bundled_tun2socks().unwrap_or_default(),
        )
        .kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    c
}

fn bundled_tun2socks() -> Option<PathBuf> {
    let app_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let bundled = app_dir.join("compat").join("badvpn-tun2socks.exe");
    let selected = compat::executable().ok()?.with_file_name("badvpn-tun2socks.exe");
    selected
        .to_string_lossy()
        .eq_ignore_ascii_case(&bundled.to_string_lossy())
        .then_some(bundled)
}
async fn run(script: &str) -> Result<String> {
    let out = tokio::time::timeout(Duration::from_secs(30), command(script).output()).await??;
    anyhow::ensure!(
        out.status.success(),
        "TAP configuration: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
pub fn arguments(device: &str, port: u16) -> Vec<String> {
    [
        "--tundev".into(),
        format!("tap0901:{device}:{IP}:10.4.2.0:255.255.255.0"),
        "--netif-ipaddr".into(),
        GATEWAY.into(),
        "--netif-netmask".into(),
        "255.255.255.0".into(),
        "--socks-server-addr".into(),
        format!("127.0.0.1:{port}"),
        "--udpgw-connection-buffer-size".into(),
        "8".into(),
        "--socks5-udp".into(),
        "--loglevel".into(),
        "0".into(),
    ]
    .to_vec()
}
pub struct TapFrontend {
    child: Option<tokio::process::Child>,
    pub device: String,
    logger: Logger,
}
impl TapFrontend {
    pub async fn start(port: u16, logger: &Logger) -> Result<Self> {
        let exe = compat::executable()?.with_file_name("badvpn-tun2socks.exe");
        anyhow::ensure!(exe.is_file(), "NetMod tun2socks executable is missing");
        // Identify an active foreign TAP owner before touching its adapter.
        // This only terminates a stale helper copied with our own app.
        run(CHECK_OWNER).await?;
        // Restore state left by an interrupted session, then prepare the TAP.
        run(RESTORE).await?;
        let mut guard = Self {
            child: None,
            device: String::new(),
            logger: logger.clone(),
        };
        guard.device = run(PREPARE).await?;
        let mut c = tokio::process::Command::new(exe);
        c.args(arguments(&guard.device, port))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        c.creation_flags(0x08000000);
        let mut child = c.spawn().context("start NetMod tun2socks")?;
        #[cfg(windows)]
        crate::child_job::attach(child.id().context("tun2socks exited before supervision")?)?;
        for pipe in [
            child
                .stdout
                .take()
                .map(|p| Box::pin(p) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>),
            child
                .stderr
                .take()
                .map(|p| Box::pin(p) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>),
        ]
        .into_iter()
        .flatten()
        {
            let log = logger.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log.info(format!("[tun2socks] {line}"));
                }
            });
        }
        guard.child = Some(child);
        tokio::time::sleep(Duration::from_millis(800)).await;
        anyhow::ensure!(
            guard.exit_code().is_none(),
            "NetMod tun2socks exited during adapter startup"
        );
        logger.event("NetMod TAP forwarding active: tun2socks + SOCKS5 UDP; DNS 8.8.8.8, 8.8.4.4");
        Ok(guard)
    }
    pub fn exit_code(&mut self) -> Option<i32> {
        self.child
            .as_mut()?
            .try_wait()
            .ok()
            .flatten()
            .map(|s| s.code().unwrap_or(-1))
    }
    pub async fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill().await;
            let _ = c.wait().await;
        }
        if let Err(e) = run(RESTORE).await {
            self.logger.error(format!("TAP restoration failed: {e:#}"));
        }
    }
}
impl Drop for TapFrontend {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.start_kill();
        }
        if journal().exists() {
            let mut c = std::process::Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command", RESTORE])
                .env("SNI_TAP_JOURNAL", journal());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x08000000);
            }
            if let Ok(out) = c.output() {
                if !out.status.success() {
                    self.logger
                        .error("TAP restoration failed; recovery journal retained");
                }
            }
        }
    }
}
