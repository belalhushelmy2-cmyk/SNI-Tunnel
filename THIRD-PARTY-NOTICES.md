# Bundled third-party components

This local build includes connection components. No user profiles, credentials,
logs or imported NetMod configurations are included.

* Xray-core v26.3.27, Windows x64: official XTLS release, MPL 2.0.
  Source: https://github.com/XTLS/Xray-core/tree/v26.3.27
* Xray-core v1.6.1, Linux amd64: official XTLS release, MPL 2.0.
  Source: https://github.com/XTLS/Xray-core/tree/v1.6.1
  The source bundle includes the release archive and corresponding source.
* Windows compatibility v2ray.exe and badvpn-tun2socks.exe are copied unchanged
  from this user's existing NetMod installation. The core identifies as Xray
  1.6.1; its exact vendor modifications/source were not supplied by NetMod.
  These files reproduce the user's tested local installation. This is not an
  assertion that this vendor binary matches an unmodified XTLS source build.
* BadVPN Linux tun2socks: BSD license, upstream commit
  07268f02706e78e282e19641b5d1d41e8e89bf31. Source archive is included.
  https://github.com/ambrop72/badvpn
* Wintun: the original prebuilt binary license is included verbatim under
  licenses/Wintun-prebuilt-license.txt. It is not described as BSD licensed.
  https://www.wintun.net/
* TAP-Windows Adapter V9: signed OpenVPN driver copied unchanged from this
  user's installed NetMod TAP-driver/x64 directory, DriverVer 04/21/2016,
  9.00.00.21. GPL v2 license is included. Upstream source:
  https://github.com/OpenVPN/tap-windows6
* VCRUNTIME140.dll: Microsoft runtime copied from this user's existing NetMod
  directory alongside the dependent 32-bit tun2socks helper; Windows 10/11
  supplies the Universal CRT. https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
* Rust dependencies retain their individual licenses. Cargo.lock pins them;
  Cargo downloads their source/license files when rebuilding.

The repository has no explicit license grant for SNI Tunnel itself. This
source snapshot is supplied to its requesting owner for local rebuilding;
RPM uses LicenseRef-SNI-Tunnel without inventing a project license.
