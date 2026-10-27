# SNI Tunnel 2.1.5 — a clearer desktop, phone and TV

- Restore the compact desktop overview: slim navigation, teal connection panel,
  one server/public-IP row, fixed health tiles and equal download/upload/session cards.
- Keep full-width traffic bars from actual statistics. Switch to the measured
  DNS + TCP latency history without crowding the dashboard; failed probes leave gaps.
- Refine every page with consistent navy/teal colors, readable controls and native
  focus/hover states. All existing pages, configuration fields and actions remain.
- Android phone and TV 1.0.6 retain a clearer connection action, grouped server
  controls, fixed metric cards, compact phone menu and visible TV focus states.
  System-bar/cutout spacing, Allow insecure editing and stored profiles are retained.
- Android 1.0.6 adds a partial CPU wake lock for VPN startup, forwarding and recovery
  while the screen is off. Disconnect, startup failure, VPN revocation and service
  destruction release it. Both flavors passed screen-off 2 MiB transfer and lock
  lifecycle checks in the emulator.
- Desktop networking/core implementations are unchanged.

Fedora RPM: `sni-tunnel-v2-2.1.5-1.x86_64.rpm`. Install or update with
`sudo dnf install ./sni-tunnel-v2-2.1.5-1.x86_64.rpm`. DNF resolves Fedora runtime
dependencies; the package includes the GUI, networking helpers and launcher.
Fedora 44 installation/reinstallation and native Wayland launch checks passed.

Solus and AerynOS: native `.eopkg` and `.stone` packages include the published
Linux desktop, Xray and BadVPN, with launcher integration and distro-managed
runtime dependencies. Both passed isolated native installation, Wayland startup
and controlled local-proxy round trips. See INSTALL.md for eopkg/moss commands.
The terminal updater now recognizes existing Solus packages and uses eopkg.

macOS proxy previews: native Apple Silicon and Intel DMGs bundle Xray 26.3.27.
Both passed native tests, mounted-app startup and custom-SNI VMess/WS/TLS tests
with explicit certificate pins. They support local HTTP/SOCKS proxy mode, with
no full-device TUN, kill switch or automatic system proxy management. Untrusted
certificates need a verified pin in the server editor. Apps are ad-hoc signed,
not Apple Developer ID signed or notarized. See INSTALL.md before installing.

FreeBSD proxy preview: `sni-tunnel-v2-2.1.5.pkg` targets FreeBSD 14 amd64 and
bundles the native Slint GUI with Xray 26.3.27. Native build/unit tests, package
installation/integrity checks and custom-SNI VMess/WS/TLS pin tests passed on
FreeBSD 14.3. Install with `sudo pkg install ./sni-tunnel-v2-2.1.5.pkg`.
As on macOS, use the local HTTP/SOCKS proxy; full-device TUN is not implemented.

Includes Linux portable/archive, AppImage, DEB, RPM, Arch binary package, Windows
portable/installer and signed Android phone/TV APKs. Android retains its application
IDs and signing identity for upgrades. The Linux install/update command selects
Latest and preserves existing profiles.

Native UI rendering and emulator checks cover presentation and interaction;
physical phone/TV networks and the user's server are not inferred from those tests.
