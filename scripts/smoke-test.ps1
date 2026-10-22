# Live end-to-end smoke test.
#
# Runs the CLI against a real server and reports PASS / FAIL / BLOCKED for
# every step. It never fabricates a result: if a step cannot run (no server,
# no administrator rights, no network) it is reported as BLOCKED.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\smoke-test.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\smoke-test.ps1 -Tun
#
# The server comes from the stored profile (the GUI's Servers tab) or from
# -Vmess <link>.

param(
    [string]$Vmess,
    [switch]$Tun,
    [switch]$SkipNetwork
)

$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\tools\w64devkit\bin;$env:PATH"

$exe = "target\debug\sni-tunnel.exe"
if (-not (Test-Path $exe)) {
    Write-Host "building the debug binary..."
    cargo build
    if ($LASTEXITCODE -ne 0) { throw "the build failed" }
}

$results = @()
function Step($name, $block) {
    Write-Host ""
    Write-Host "== $name =="
    try {
        $global:LASTEXITCODE = 0
        $out = & $block 2>&1 | Out-String
        Write-Host $out.Trim()
        $code = $LASTEXITCODE
        if ($null -ne $code -and $code -ne 0) {
            $script:results += [pscustomobject]@{ Step = $name; Result = "FAIL"; Output = "exit code $code" }
        } else {
            $script:results += [pscustomobject]@{ Step = $name; Result = "PASS"; Output = $out.Trim() }
        }
    } catch {
        Write-Host "ERROR: $_"
        $script:results += [pscustomobject]@{ Step = $name; Result = "FAIL"; Output = "$_" }
    }
}

# ---------------------------------------------------------------- build / unit
Step "build (debug)" { cargo build 2>&1 | Select-Object -Last 1 }
Step "unit + integration tests" { cargo test 2>&1 | Select-String "test result" }

# ---------------------------------------------------------------- CLI basics
Step "help" { $o = & $exe --help 2>&1 | Out-String; ($o -split "`r?`n" | Select-Object -First 3) -join "`n" }
Step "store round trip" {
    & $exe list 2>&1 | Out-String
    $o = & $exe export 2>&1 | Out-String
    ($o -split "`r?`n" | Select-Object -First 3) -join "`n"
}

# ---------------------------------------------------------------- networking
if ($SkipNetwork) {
    $results += [pscustomobject]@{ Step = "network tests"; Result = "BLOCKED"; Output = "-SkipNetwork" }
} else {
    $args = @()
    if ($Vmess) { $args += @("--vmess", $Vmess) }
    if ($Tun) { $args += @("--mode", "tun") }

    Step "selftest (core + local proxy)" { & $exe selftest 2>&1 | Out-String }
    Step "check (diagnostics + real traffic)" { & $exe check @args 2>&1 | Out-String }
    Step "tun self test" { & $exe tun-selftest 2>&1 | Out-String }
    if ($Tun) {
        Step "full tunnel verification" { & $exe verify-full 2>&1 | Out-String }
    } else {
        $results += [pscustomobject]@{ Step = "full tunnel verification"; Result = "BLOCKED"; Output = "needs -Tun" }
    }
}

# ---------------------------------------------------------------- cleanup
Step "no leftover processes" {
    $alive = @()
    foreach ($p in (Get-Process -Name "sni-tunnel", "xray" -ErrorAction SilentlyContinue)) {
        # A process that has already exited but whose handle is still open shows
        # up in the list; it holds no resources and must not count.
        try { if (-not $p.HasExited) { $alive += $p } } catch { }
    }
    "alive leftover processes: $($alive.Count)"
    if ($alive.Count -gt 0) { throw "$($alive.Count) process(es) are still running" }
}
Step "system proxy untouched" {
    $p = (Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue).ProxyServer
    "ProxyServer = '$p'"
}

Write-Host ""
Write-Host "================ SUMMARY ================"
$results | Format-Table -Property Step, Result -AutoSize
$failed = ($results | Where-Object { $_.Result -eq "FAIL" }).Count
$blocked = ($results | Where-Object { $_.Result -eq "BLOCKED" }).Count
Write-Host "PASS: $(($results | Where-Object { $_.Result -eq 'PASS' }).Count)  FAIL: $failed  BLOCKED: $blocked"
if ($failed -gt 0) { exit 1 }
