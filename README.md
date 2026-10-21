# SNI Tunnel

Windows x64 installer, Linux amd64 packages, and source code.

## Downloads

Open [Releases](https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/latest) for:

- Windows installer with bundled Xray, Wintun, compatibility core, tun2socks runtime and TAP driver.
- Linux DEB, RPM and tar.gz packages. Linux uses the kernel TUN device.
- Complete source ZIP and Linux build scripts.
- Installation instructions and SHA-256 checksums.

## Installation

Close SNI Tunnel before installing or upgrading Windows. NetMod is not required at runtime. The installer reuses an existing TAP V9 adapter.

Debian/Ubuntu: `sudo apt install ./sni-tunnel-linux-amd64.deb`

Fedora: `sudo dnf install ./sni-tunnel-linux-amd64.rpm`

Portable Linux: extract the tar.gz and run `./start.sh`. Requires x86_64, glibc 2.30+, Python 3.8 with Tk, iproute2, polkit/pkexec, CA certificates and TUN support.

## Build on Linux

Download and extract **sni-tunnel-source.zip** from the release, then run `sh packaging/linux/build.sh`. Install Rust stable, a C/C++ compiler, CMake and Python first. The first build downloads Rust dependencies.

## Validation and notices

Linux packages were cross-compiled and their contents checked on Windows. Linux desktop/kernel behavior and ISP quota accounting still require testing on a Linux machine.

The Windows compatibility binaries were copied unchanged from the requesting user's existing NetMod installation. See THIRD-PARTY-NOTICES.md inside the downloads for provenance and licensing. Saved profiles, credentials and connection logs are excluded.
