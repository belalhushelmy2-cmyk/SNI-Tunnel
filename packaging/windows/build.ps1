param([switch]$SkipRelease)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$env:RUST_MIN_STACK='8388608'
if (!$SkipRelease) {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
}
# Stage official modern Xray using the existing pinned archive, and copy the
# user's tested compatibility binaries/driver. Never collect profiles/logs.
$compat=Join-Path ${env:ProgramFiles(x86)} 'NetMod/bin'
# A clean build machine can recover unchanged compatibility runtime assets from
# the pinned previous release; never use its application executable as V2.
if (!(Test-Path packaging/staging/windows/compat/v2ray.exe) -and !(Test-Path (Join-Path $compat 'v2ray.exe'))) {
    $previous=Join-Path $root 'target/previous-runtime-installer.exe'
    Invoke-WebRequest 'https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/download/v0.4.3/SNI-Tunnel-Setup-Windows-x64.exe' -OutFile $previous
    if ((Get-FileHash $previous).Hash.ToLower() -ne 'e63a12a086ae29a1369be99c37b321b36fe4eb966144c2d318a31c549dadd2f8') { throw 'Previous runtime archive checksum mismatch' }
    $seven=Join-Path $env:ProgramFiles '7-Zip/7z.exe'
    if (!(Test-Path $seven)) { throw 'Install 7-Zip to extract the verified previous runtime assets' }
    & $seven x -y $previous '-opackaging/staging/windows' core compat driver
    if ($LASTEXITCODE -ne 0) { throw 'Runtime asset extraction failed' }
}

New-Item -ItemType Directory -Force packaging/staging/windows/compat,packaging/staging/windows/driver,packaging/staging/windows/core | Out-Null
if (!(Test-Path packaging/staging/windows/core/xray.exe)) {
    $archive=Join-Path $root 'target/xray-windows-64.zip'
    Invoke-WebRequest 'https://github.com/XTLS/Xray-core/releases/download/v26.3.27/Xray-windows-64.zip' -OutFile $archive
    if ((Get-FileHash $archive).Hash.ToLower() -ne 'd004c39288ce9ada487c6f398c7c545f7d749e44bdfdd59dbc9f865afba4e1ad') { throw 'Xray checksum mismatch' }
    Expand-Archive $archive packaging/staging/windows/core -Force
}
foreach ($name in @('v2ray.exe','badvpn-tun2socks.exe','vcruntime140.dll')) {
    if (!(Test-Path "packaging/staging/windows/compat/$name")) {
        Copy-Item (Join-Path $compat $name) packaging/staging/windows/compat
    }
}
if (!(Test-Path packaging/staging/windows/driver/tap0901.sys)) {
    Copy-Item (Join-Path $compat 'TAP-driver/x64/*') packaging/staging/windows/driver
}
New-Item -ItemType Directory -Force releases/v2/windows | Out-Null
makensis /V2 packaging/windows/SNI-Tunnel.nsi
if ($LASTEXITCODE -ne 0) { throw 'NSIS installer build failed' }
Write-Host 'Installer: releases/v2/windows/SNI-Tunnel-V2-Setup-x64.exe'
# Portable distribution carries exactly the same runtimes as the installer.
$portable=Join-Path $root 'target/windows-portable'
New-Item -ItemType Directory -Force $portable | Out-Null
Copy-Item target/release/sni-tunnel.exe $portable
Copy-Item packaging/README-INSTALL.txt,THIRD-PARTY-NOTICES.md,packaging/windows/install-tap.ps1 $portable
foreach ($folder in @('core','compat','driver')) {
    Copy-Item -Recurse -Force "packaging/staging/windows/$folder" $portable
}
Copy-Item -Recurse -Force packaging/licenses $portable
$version=(Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
Compress-Archive -Force -Path "$portable/*" -DestinationPath "releases/v2/windows/SNI-Tunnel-V2-$version-windows-x64-portable.zip"
