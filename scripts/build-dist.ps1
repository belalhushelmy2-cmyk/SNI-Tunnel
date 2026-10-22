# Builds a release binary and packages a distributable zip under dist\.
#
# Usage:  powershell -ExecutionPolicy Bypass -File scripts\build-dist.ps1
#         powershell -ExecutionPolicy Bypass -File scripts\build-dist.ps1 -Linux
#
# The resulting zip contains sni-tunnel.exe and README.md. The networking
# binaries (Xray-core, which also provides the wintun driver) are downloaded
# and SHA-256 verified on first run into %LOCALAPPDATA%\SniTunnel\bin, so they
# are intentionally not bundled here.
#
# -Linux additionally cross-compiles the headless Linux CLI (x86_64) with
# cargo-zigbuild; the GUI needs a Linux sysroot (fontconfig) and is therefore
# not built here.

param(
    [switch]$Linux
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\tools\w64devkit\bin;$env:PATH"
# rustc dies with STATUS_STACK_BUFFER_OVERRUN (0xc0000409) on some crates with
# the default stack; 16 MB is enough for the whole dependency tree.
if (-not $env:RUST_MIN_STACK) { $env:RUST_MIN_STACK = "16777216" }

Write-Host "==> cargo build --release"
# -j 1: the release build is memory hungry and the intermittent rustc crash
# above happens more often with parallel codegen.
cargo build --release -j 1
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$name = "sni-tunnel-$version-windows-x64"
$out = Join-Path $root "dist\$name"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out -Force | Out-Null

Copy-Item "target\release\sni-tunnel.exe" (Join-Path $out "sni-tunnel.exe")
Copy-Item "README.md" (Join-Path $out "README.md")
if (Test-Path "THIRD-PARTY-NOTICES.md") { Copy-Item "THIRD-PARTY-NOTICES.md" $out }

$zip = Join-Path $root "dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $out "*") -DestinationPath $zip

Write-Host ""
Write-Host "Packaged: $zip"
Get-ChildItem $out | Select-Object Name, Length | Format-Table

# ---------------------------------------------------------------------------
# Linux (headless CLI)
# ---------------------------------------------------------------------------
if ($Linux) {
    Write-Host ""
    Write-Host "==> cargo zigbuild --release --no-default-features --target x86_64-unknown-linux-gnu"
    $zig = Get-Command zig -ErrorAction SilentlyContinue
    if (-not $zig) {
        $candidates = Get-ChildItem "$env:TEMP\opencode\zigroot" -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName "zig.exe" } | Where-Object { Test-Path $_ }
        if ($candidates) { $env:PATH = "$(Split-Path $candidates[0]);$env:PATH" }
    }
    if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {
        Write-Warning "zig was not found - skipping the Linux build"
    } else {
        # A separate target directory: mixing the host and the Linux target in
        # one directory confuses cargo's metadata cache.
        $env:CARGO_TARGET_DIR = Join-Path $root "target\linux"
        cargo zigbuild --release --no-default-features --target x86_64-unknown-linux-gnu
        if ($LASTEXITCODE -ne 0) { throw "the Linux build failed" }
        $env:CARGO_TARGET_DIR = Join-Path $root "target\linux"
        $linuxDir = Join-Path $root "dist\sni-tunnel-$version-linux-x86_64"
        if (Test-Path $linuxDir) { Remove-Item -Recurse -Force $linuxDir }
        New-Item -ItemType Directory -Path $linuxDir -Force | Out-Null
        Copy-Item (Join-Path $env:CARGO_TARGET_DIR "x86_64-unknown-linux-gnu\release\sni-tunnel") (Join-Path $linuxDir "sni-tunnel")
        Copy-Item "README.md" $linuxDir
        if (Test-Path "THIRD-PARTY-NOTICES.md") { Copy-Item "THIRD-PARTY-NOTICES.md" $linuxDir }
        $tar = Join-Path $root "dist\sni-tunnel-$version-linux-x86_64.tar.gz"
        if (Test-Path $tar) { Remove-Item -Force $tar }
        tar -czf $tar -C (Join-Path $root "dist") "sni-tunnel-$version-linux-x86_64"
        Write-Host "Packaged: $tar"
        Get-ChildItem $linuxDir | Select-Object Name, Length | Format-Table
    }
}
