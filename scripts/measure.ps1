# Measures the resource usage of the application (idle RAM, CPU, startup time).
#
# Usage:  powershell -ExecutionPolicy Bypass -File scripts\measure.ps1

$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\tools\w64devkit\bin;$env:PATH"

$exe = if (Test-Path "target\release\sni-tunnel.exe") { "target\release\sni-tunnel.exe" } else { "target\debug\sni-tunnel.exe" }
Write-Host "measuring: $exe"

function Measure-Process($proc, $seconds) {
    $cpu0 = $proc.CPU
    $t0 = Get-Date
    Start-Sleep -Seconds $seconds
    $proc.Refresh()
    $cpu1 = $proc.CPU
    $elapsed = ((Get-Date) - $t0).TotalSeconds
    $rss = [int]($proc.WorkingSet64 / 1MB)
    [pscustomobject]@{
        RssMB   = $rss
        CpuSec  = [math]::Round($cpu1 - $cpu0, 3)
        CpuPct  = [math]::Round((($cpu1 - $cpu0) / $elapsed) * 100, 2)
    }
}

# ---------------------------------------------------------------- GUI, idle
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $exe -ArgumentList 'gui' -WorkingDirectory (Get-Location) -PassThru -WindowStyle Hidden
$sw.Stop()
$startupMs = $sw.ElapsedMilliseconds
Start-Sleep -Seconds 6
$p.Refresh()
Write-Host ""
Write-Host "== GUI =="
Write-Host "startup (process spawn)      : $startupMs ms"
$idle = Measure-Process $p 10
Write-Host "idle RSS                     : $($idle.RssMB) MB"
Write-Host "idle CPU over 10 s           : $($idle.CpuSec) s ($($idle.CpuPct) %)"
$threads = $p.Threads.Count
Write-Host "threads                      : $threads"
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# ---------------------------------------------------------------- headless CLI
Write-Host ""
Write-Host "== headless CLI =="
$sw = [Diagnostics.Stopwatch]::StartNew()
$out = & $exe list 2>&1 | Out-String
$sw.Stop()
Write-Host "'list' round trip            : $($sw.ElapsedMilliseconds) ms"
Write-Host "binary size                  : $([math]::Round((Get-Item $exe).Length / 1MB, 2)) MB"

# ---------------------------------------------------------------- tunnel
Write-Host ""
Write-Host "== tunnel (TUN) =="
$p = Start-Process -FilePath $exe -ArgumentList 'connect' -WorkingDirectory (Get-Location) -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 14
$busy = Measure-Process $p 10
Write-Host "tunnel RSS                   : $($busy.RssMB) MB (app only)"
Write-Host "tunnel CPU over 10 s         : $($busy.CpuSec) s ($($busy.CpuPct) %)"
$xray = Get-Process -Name xray -ErrorAction SilentlyContinue | Where-Object { -not $_.HasExited } | Select-Object -First 1
if ($xray) {
    $x = Measure-Process $xray 10
    Write-Host "xray-core RSS                : $([int]($xray.WorkingSet64 / 1MB)) MB"
    Write-Host "xray-core CPU over 10 s      : $($x.CpuSec) s ($($x.CpuPct) %)"
}
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

Write-Host ""
Write-Host "== summary =="
Write-Host "GUI idle RSS                 : $($idle.RssMB) MB"
Write-Host "GUI idle CPU                 : $($idle.CpuPct) %"
Write-Host "GUI startup                  : $startupMs ms (spawn)"
Write-Host "CLI 'list'                   : $($sw.ElapsedMilliseconds) ms"
