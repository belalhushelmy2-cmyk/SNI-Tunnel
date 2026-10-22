# SNI Tunnel V2 validation — 2026-10-22

## Result and replacement status

V2 is built separately from the previous application. Automated regression tests,
real-data migration and Linux local-proxy HTTPS forwarding passed. This is a
candidate for platform testing, not approval to replace the installed client.
No installation, uninstallation, driver installation, system-proxy change or
system-wide TUN test was performed during this V2 validation.

## Executed checks

| Check | Result | Scope |
| --- | --- | --- |
| Rust regression suite | 121 passed; 5 opt-in tests ignored | `cargo test --all --locked`: 111 unit (including GUI import), 3 persistence/migration integration, 7 workflow tests |
| Python suite | 12 passed | Existing Linux profile/editor regressions and shared network-helper recovery/DNS tests; includes a legacy Tk X11 editor test, not native Slint input automation |
| Native Linux GUI build | Passed | Release software-rendered Slint GUI, built with glibc 2.30 target |
| Windows x64 GUI build | Passed | Cross-compiled GNU release executable; Windows runtime has not been executed here |
| Linux package | Passed | DEB and tar.gz built; DEB inspected and extracted without installation; package identity, payload permissions, required core/helper and absence of user stores checked |
| Windows installer | Built | NSIS compilation; payload extraction and application hash verification recorded with delivery artifacts |
| Slint geometry and scrolling | Passed | Six pages; dashboard at 800x600; editor at 660x420; wheel events and fixed-footer assertion at 100% and 150% scale |
| Formatting and scripts | Passed | Changed Rust files checked with rustfmt; Python compilation and shell syntax checked |
| Real profile migration | Passed | Both real legacy profiles migrated using the extracted packaged Linux app; saved credentials, address/port, effective TLS/SNI, WS Host/path, ALPN, fingerprint and MUX compared; all original backup-manifest hashes unchanged |
| Non-destructive migration | Passed | Refuses existing V2 stores, checks round-trip persistence, retains original-byte backups; regression covers blank SNI overrides and preservation of TLS |
| Linux VMess HTTPS forwarding | Passed | Packaged app, copied real profile, `check --mode none`; real proxied HTTP 200; Xray started and stopped |
| Sustained local-proxy sample | Passed | 12 of 12 HTTPS requests over 63.58 seconds; graceful disconnect exit 0; no system proxy/routes/DNS changes |

The standalone Rust TLS diagnostic failed during the successful VMess forwarding
check. DNS and TCP diagnostics passed, and the bundled Xray successfully carried
HTTPS using the preserved profile. The diagnostic result is recorded separately;
it is not presented as a successful standalone TLS probe.

## UI and performance

The offline harness uses the real Slint software renderer to construct all six
pages and the server editor without an engine, user store or network. Synthetic
sample values contain no real profile information. Exact 800x600 dashboard and
660x420 editor layouts are tested with wheel events; the editor footer is checked
to remain unchanged when scrolling. Native desktop previews also ran, but the
local tiling compositor can constrain a window below its minimum size, so exact
geometry checks use an isolated software window. Layout snapshots and injected
scroll events do not establish all desktop keyboard/mouse interactions.

An early like-for-like local debug-GUI idle sample showed about 127 MiB RSS and
10% of one CPU core before changes, versus about 67 MiB RSS and 0.33% after the
Linux software-rendering change. Each sample used a three-second warmup and a
three-second observation with no tunnel. These are short observations, not a
sustained benchmark. The final extracted release GUI measured 37,232 KiB RSS
(about 36.4 MiB) and no additional CPU ticks in its three-second sample; this
release result should not be treated as a controlled comparison to debug builds
or a guarantee of zero CPU use. Network throughput and cold-start improvements
have not been established by measurements.

## Outstanding platform and replacement gates

- Real Windows 10/11 startup, installer/fresh-install/upgrade/uninstall and driver
  behavior; Windows proxy/TUN forwarding and crash cleanup.
- Debian/Ubuntu/Mint/Zorin fresh install and package-manager lifecycle; a glibc
  floor check alone does not establish distribution compatibility.
- Live Linux full-device TUN, NetworkManager DNS/routes, privilege prompts,
  route restoration, Wi-Fi changes and suspend/resume recovery.
- Native Slint keyboard/mouse/touchpad editing on both X11 and Wayland; screenshot
  layout checks do not replace interaction testing.
- Longer sessions, fault injection during real forwarding, and repeated reconnects
  across network changes. The one-minute sample does not establish long-term
  server reliability or ISP quota accounting.
- Identify the installed old Linux client's supported uninstall mechanism. Initial
  inspection found no pacman/dpkg ownership for `/usr/lib/sni-tunnel`.
- Obtain explicit user approval at the replacement stage after these gates are
  resolved, as required by the request. Do not delete installation directories as
  a substitute for an official uninstaller.

## Functional platform limits

Linux currently retains the previous bundled Xray 1.6.1/BadVPN transport behavior:
VMess WebSocket plus TLS; unsupported XUDP/certificate-pin combinations fail
explicitly. Linux system-proxy integration supports GNOME/Cinnamon-compatible
gsettings desktops. Linux tray, Windows NetMod compatibility and Windows TUN kill
switch controls are disabled with platform labels. Legacy unknown JSON fields
remain in permanent backups; only native-model fields are active in V2. Existing
profiles' TLS, SNI and transport choices are not weakened to improve speed.

Raw real-profile diagnostics, keys and configuration backups remain private and
are excluded from Git, source archives and release artifacts.

## AppImage follow-up — 2026-10-22

The x86_64 AppImage packages the native V2 binary with an AppRun launcher, bundled
Xray/BadVPN/helper, desktop entry, icon and licenses. It excludes the old Tk app
and all profile/key stores. appimagetool 1.9.1 and type2-runtime 20251108 were
fetched from official versioned releases and checked against pinned SHA-256s.
The Linux glibc-2.30 release rebuilt successfully after the AppImage-specific
autostart change. Earlier DEB/Windows packages are preserved.

Executed against the packaged image in isolated directories:
- Normal FUSE runtime `--help` and `--appimage-extract-and-run --help`: passed.
- A filename and directory containing spaces: passed.
- Autostart on/off, with Exec pointing to the persistent AppImage rather than a
  mount/cache path: passed.
- Payload binary hash matches the built V2 client; new UI strings present; old
  `app.py` and user profile/key files absent: passed.
- Modified executable-cache content is rejected before execution: passed.
- Native GUI remains alive after a four-second launch smoke test: passed.
- Two concurrent first launches sharing an empty cache: passed.

This adds AppImage launch/packaging evidence. It does not add a fresh live TUN,
DNS/route restoration or sustained-network test. The previous Tk application
was connected during validation and was not stopped or replaced. Full desktop
interaction/distribution gates from the earlier report remain open. Host
requirements and cache behavior are documented in packaging/linux/APPIMAGE.md.

## Installed UI and scale follow-up — 2026-10-22

The user explicitly requested applying V2 to the existing installation. The
original installation was backed up and retained; the launcher was switched to
an adjacent V2 payload. The old helper handled SIGTERM and exited; its routing
journal and sni0 routes were absent before the old window was closed. Both
migrated profiles were checked against the legacy settings before opening the
installed native GUI. Original legacy profile bytes were unchanged.

At the user's request, installed and AppImage launchers default to 100% scale.
The installed GUI was restarted with SLINT_SCALE_FACTOR=1 verified in its
process environment. Profile-file hashes were unchanged across that restart.
The AppImage GUI smoke check also verifies its effective default scale is 1.
This is a UI/launch check, not another live TUN/recovery test.

## Sidebar first-click and alignment follow-up — 2026-10-22

The native Slint input harness reproduced the user's first-click failure before
the fix: clicking Servers left the active tab on Overview. FocusScope was
accepting the initial pointer press before the navigation TouchArea.

The keyboard focus target now has no pointer hit area. All six menu items passed
first-click activation using icon, label and row-padding coordinates, at both
100% and 150% scale. Space/Enter activation, all six page renders, small-window
dashboard/editor scrolling and the editor's fixed-footer check also passed.
Sidebar icons/labels/markers are vertically centered in fixed 44px rows; focus
and selection retain constant border geometry. The software-rendered sidebar
preview was visually inspected. These checks do not add a new network test.

## Server import follow-up

Import and clipboard results are now visible directly on the Servers page and
are independent of engine status updates. Different credentials, protocols, SNI,
Host and paths on one host/port are retained; only identical connection settings
are skipped. Imported ID collisions are reassigned, search is cleared after a
successful import, and failed saves leave the in-memory configuration unchanged.
The default-feature Rust regression suite passed all 121 tests (5 ignored),
including distinct-connection and duplicate-metadata cases.
