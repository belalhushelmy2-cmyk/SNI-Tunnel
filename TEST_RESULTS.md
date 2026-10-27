# Linux desktop 2.1.6 app usage — 2026-10-28

- All 134 Rust library tests passed, including new baseline/delta accounting,
  pause/resume/reset, duplicate socket protection and socket parser coverage.
- A real loopback TCP transfer was resolved to the test application's executable
  by PID. Its 32 KiB upload and 16 KiB download were counted exactly once, with
  the server/relay side excluded.
- Native Slint software-renderer interaction checks passed at 100% and 150%
  scaling: seven first-click sidebar entries, existing pages/actions and scrolling
  at 800×600. The new App usage list was checked at 1366×768 and 800×600 with
  long labels, fixed numeric columns, scrolling, Pause and Reset controls.
- The headless build check passed. Networking/core/configuration code is unchanged.
- The optimized Linux build passed the packaged local HTTP proxy/Xray round trip
  using an isolated temporary store and a local fixture. Installation retains the
  previous package as a rollback and leaves the running desktop process untouched.
- This is opt-in, session-only, unprivileged Linux device TCP sampling through ss,
  not exhaustive per-app VPN accounting. Direct traffic is included; UDP/QUIC,
  inaccessible processes and connections entirely between samples are omitted.
  Windows/macOS/FreeBSD and Android/TV byte attribution is not implemented.

# FreeBSD native proxy preview — 2026-10-27

- FreeBSD 14.3 amd64 VM: native optimized Rust GUI build and 129 library tests
  passed. The published build uses release opt-level s with LTO disabled to
  bound linker memory; application/networking behavior is unchanged.
- Native pkg creation, installation, dependency/file-integrity checks and ELF
  loader resolution passed. The package declares its FreeBSD:14:amd64 ABI and
  includes the GUI, SHA-256 pinned Xray 26.3.27, launcher, icon and licenses.
- The installed application passed native X11/software GUI startup, a controlled
  HTTP proxy round trip and VMess over WebSocket/TLS with a custom SNI and explicit
  certificate pin. Wrong pins, strict untrusted certificates and unverified
  certificate discovery were rejected. No real server credentials were used.
- Full-device TUN, kill switch and automatic system proxy management are not
  implemented. Physical FreeBSD desktop/GPU behavior and real VPN servers were
  not tested. Final package inspection confirmed all files are under /usr/local.

# macOS native proxy previews — 2026-10-27

- Native Apple Silicon and Intel Rust builds and library test gates passed on
  macOS 14.8.9 arm64 and macOS 15.7.9 x86_64. Deployment target: macOS 12.
- Final DMGs bundle architecture-matched, SHA-256 pinned Xray 26.3.27. App/helper
  ad-hoc signatures passed deep strict verification; disk images passed hdiutil
  verification and were mounted read-only for the runtime tests.
- Both architectures passed a Mach-O loader audit: GUI and Xray dependencies
  refer only to Apple system libraries/frameworks, with no Homebrew runtime paths.
- Both packaged apps passed an HTTP round trip through the bundled core, native
  GUI startup, and VMess over WebSocket/TLS with an explicit certificate pin and
  custom SNI. Wrong pins, strict untrusted certificates and unverified discovery
  were rejected. Only generated fixture identities/keys and loopback servers were used.
- These are HTTP/SOCKS proxy previews. TUN, kill switch, automatic system proxy
  management and a macOS menu-bar tray are not implemented. There is no Developer
  ID signature/notarization. Real servers, physical macOS 12 machines and full
  application routing were not tested.

# Solus and AerynOS package validation — 2026-10-27

- Native Solus `.eopkg` built with ypkg in the official Solus build image.
  eopkg resolved runtime dependencies from the Solus Polaris repository,
  installed/reinstalled the package, retained an application-data marker, and
  passed the package integrity check.
  The desktop entry passed desktop-file-validate. A basic font dependency was
  added after a bare build-root test exposed the lack of system fonts.
- AerynOS `.stone` built with the unmodified upstream os-tools v0.26.1 stone
  writer, inspected/indexed by moss and dependency-resolved against the AerynOS
  unstable repository. A clean ephemeral installation passed with all required
  library/tool providers and font dependencies resolved.
- Native AerynOS moss 0.26.6 completed normal stateful installation, removal
  and reinstallation from the published GitHub repository. Three real states
  were recorded and the application-data marker survived all operations.
- Both isolated distro runtimes opened a native xdg_shell Wayland window with
  the software renderer and empty temporary stores. A real HTTP round trip
  through the packaged Xray and local proxy passed against a loopback fixture.
- The native package GUI/core/helper payload is drawn from the SHA-256 pinned
  published v2.1.5 Linux portable archive. No user servers or keys were used.
- The updater compatibility preview was also run as an unprivileged user in
  the Solus runtime: it detected the installed eopkg and selected the native
  release artifact and dependency commands.
- 19 terminal installer tests passed, including eopkg transaction selection,
  installed-package detection and refusal to continue after database failure.
  Existing moss files are protected from unmanaged replacement.
- These are package/runtime tests sharing the host kernel; they do not establish
  a complete Solus/AerynOS boot, physical polkit-agent behavior, full-device TUN
  routing, IPv6 protection or real-server connectivity. Isolated roots have no
  running systemd/udev daemons; related usysconf reload hooks reported this.

# Fedora RPM validation — 2026-10-27

- Built sni-tunnel-v2-2.1.5-1.x86_64.rpm with the native desktop GUI, Linux helper,
  Xray, BadVPN, launcher/icon and licenses. Runtime dependencies include the
  actual glibc floor, graphics/X11/Wayland libraries, Python, PolicyKit/pkexec,
  iproute, nftables, notifications and clipboard utilities.
- Official Fedora 44 x86_64 image: DNF resolved and installed all dependencies;
  installation and reinstallation passed, rpm digest/file verification passed,
  the desktop entry validated, and GUI/core CLI smoke checks passed.
- Reinstallation retained the application-data marker. A native Wayland window
  opened using the Fedora runtime with Slint's software renderer. The test used
  an isolated rootless filesystem and an empty profile store.
- All 16 installer regressions passed, including existing-RPM detection and a
  verified DNF update without creating a duplicate unmanaged installation.
- Fedora CI installation/reinstallation is now required for future publication.
  Physical Fedora SELinux/TUN authorization and actual VPN forwarding were not
  exercised by these package checks. Desktop networking code is unchanged.

# Android 1.0.6 update to release 2.1.5 — 2026-10-27

- Signed phone and TV updates: 33 JVM tests per flavor, zero lint errors, existing
  signing identity and application IDs retained; versionCode 7.
- Both flavors held a partial CPU wake lock with the screen asleep while a
  separate-UID app verified the complete controlled 2 MiB transfer. Both released
  the lock after Disconnect; failed TV startup also released it.
- Details and scope: android/TEST_RESULTS.md and android/test-support/README.md.
  Physical OEM standby behavior remains unverified.
- Desktop binaries and their prior validation remain unchanged.

# 2.1.5 interface validation — 2026-10-24

- Native Slint rendering covers six pages at 1366×768, 1536×1024, 1920×1080,
  980×720 and 800×600. The compact overview fits its quick actions at 1366×768.
- Pointer/keyboard regressions passed at 100% and 150% scale: fixed metric/health
  geometry under changing values, real-sample traffic bars/toggle, latency-tab
  trace rendering, Connect/Disconnect/Cancel, Verify, IP refresh, server selection,
  six first-click sidebar actions, Space/Enter, Ctrl+K, five normal/compact server
  rows, dashboard scrolling and editor scrolling with a fixed footer.
- All 130 Rust library tests passed locally. CI runs the broader cross-platform
  gates before publishing. Existing unrelated formatting warnings are retained;
  the changed Rust preview harness passes rustfmt.
- Signed Android phone/TV 1.0.5 builds: 33 JVM tests per flavor, zero lint errors.
  See android/TEST_RESULTS.md for emulator and signing checks.
- No src/ networking/core modules or Android service/profile-store implementation
  changed. No real server credentials or user profiles were used in tests.

## Earlier validation

# 2.1.4 validation

Local checks cover the real native UI at 100%/150% scaling, equal/stable metric
geometry with changing numerical text, measured bar rendering, and existing
navigation/action/scrolling regressions. Latency regression covers the 60-probe
bound, failed-probe gaps and real zero-millisecond samples. No new networking
probes or desktop network implementation changes are introduced.

Linux/Windows/Arch/Android CI gates must pass before publication. Android 1.0.4
APKs are reused byte-for-byte with pinned SHA-256 values and their existing
signing identity; see android/TEST_RESULTS.md for their controlled evidence and
physical-device limitations.

## Earlier validation

# 2.1.3 local validation — 2026-10-24

- Rust: 141 tests passed; five privileged/environment-dependent tests remain
  explicitly ignored locally. No real profiles or remote server credentials used.
- Native Slint software rendering: all six pages at 1366×768, 1536×1024,
  1920×1080, 980×720 and 800×600. Inspected card geometry, labels and scrolling.
- Pointer/keyboard harness: first-click sidebar actions, Space/Enter, Ctrl+K,
  five independently clickable server rows in normal/compact layouts, dashboard
  scrolling and the editor's scrollable fields/fixed footer; 100% and 150% scale.
- Dashboard regressions ensure traffic comes only from actual engine statistics,
  history is bounded to 60 samples, disconnected history clears, and proxy-only
  success never appears as verified full-device or IPv6 forwarding.
- Desktop changes are confined to presentation, assets and packaging metadata;
  desktop Xray, transport, routing, DNS, MUX and recovery modules are unchanged.
- Android results and physical-device limitations: [android/TEST_RESULTS.md](android/TEST_RESULTS.md).

Publication is gated by the Linux, Windows, Arch/offline-package and Android CI
jobs. Local rendering does not establish native Windows runtime behavior or
exhaustive physical HiDPI/desktop coverage. A real-user WAN/phone/TV connection
and a multi-hour soak have not been used as evidence for this release.

## Earlier validation

# 2.1.0 validation

**Published and verified:** [v2.1.0](https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/tag/v2.1.0).
Final [Actions run 39961207877](https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/actions/runs/39961207877) passed every required job, including native Windows WFP enforcement/recovery, isolated Linux IPv4/IPv6 packet tests, EXE version/payload, DEB payload, AppImage smoke tests and Arch/Ubuntu offline bundles. All 16 release assets are present; the published SHA256SUMS matches GitHub's per-asset digests.

Executed locally on Linux:
- Rust: 129 tests passed (118 library + 11 integration); 5 existing live/opt-in tests ignored.
- Python Linux regression suite: 22 tests passed; offline installer regression: 1 test passed.
- Isolated Linux firewall packet test: IPv4/IPv6 fallback blocked, loopback/TUN allowed, adapter deletion still blocked, tampering detected, cleanup restored access and preserved an unrelated nftables table.
- UI checks at 100% and 150%: first-click navigation, five independent server rows, compact layout and scrolling passed.
- Mako Freedesktop notification service responds to GetServerInformation; visual delivery not asserted.
- Windows GNU library/binary/tests cross-check compiled successfully. This is not Windows runtime verification.

Required publication gates (enforced by the release workflow):
- Final portable/GUI package checks and native package payload verification.
- Windows Actions installer build and payload verification. Native regression and live WFP fallback/endpoint exception/replacement/cleanup tests passed on validation commit 7e9d26e; final source will repeat these gates.
- Linux portable/DEB/AppImage build, offline dependency packaging and checksum verification.

Runtime limitations: real VMess sessions, suspend/resume across physical hardware, multi-hour soak, Windows notification presentation, and all distribution/desktop combinations have not been verified by these local tests. Existing user connections were not disrupted for testing.

## Previous release results

# 2.0.2 dependency-bundle validation

The release workflow gates publication on the Rust regressions, Linux GUI checks, Windows executable/installer checks, Debian payload verification, AppImage smoke test, and successful creation of both offline dependency bundles.

New local verification: self-extracting installer checksum validation, extraction into paths with spaces, inner file verification, and rejection of modified outer/inner payloads. Bundled Arch dependencies are checked against repository SHA256 values and official package signatures. Ubuntu dependency downloads use apt repository authentication.

This release does not claim a fresh-device TUN connection test on Omarchy or Ubuntu, a Windows driver/network lifecycle test, or Android support. Packaging a dependency does not prove that a remote server's credentials, TLS/SNI settings, or network work. Existing version 2.0.1 behavior and test details follow.

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
| Rust regression suite | 122 passed; 5 opt-in tests ignored | `cargo test --all --locked`: 111 unit, 4 persistence/migration integration, 7 workflow tests |
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

## Server import, tray and latency follow-up

Imports now append every parsed profile with a fresh ID and a distinct display
name, including repeated imports. Neither address/port nor imported IDs select
an existing row for replacement. All entries in a batch are checked before
saving; a malformed link rejects the batch rather than silently omitting it.
Multiline entry, saved counts and results are visible directly on Servers.
CLI imports use the same append path; mutating CLI commands cannot race a running
GUI. GUI launch with --vmess merges into the checked saved store rather than
saving an ephemeral single-profile config. Editor saves locate the original
profile by stable ID, so deleting another row cannot redirect the save.

The default-feature Rust suite passed 120 tests (5 ignored), including repeat
imports across process restarts, immutable original entries, same-ID imports,
multiple links on one line, invalid batch rejection and wrapped subscriptions.
The latency test connects to an actual loopback listener and reports failure
after that listener is closed. The reader samples server TCP setup every three
seconds, bounds each sample at two seconds, ignores results for stale targets,
and stops with the app. Samples are not written to the profile store.

Linux tray support uses StatusNotifierItem over session D-Bus, including menu
commands and single-click restore. Closing hides only when a host is available;
loss of the host restores the window. Without a host Minimize uses the taskbar.
Native Waybar/D-Bus checks passed: registration, close-to-tray while the process
remained alive, click-to-restore, menu labels, graceful Quit and deregistration.
Three loopback probes arrived at intervals of 3.0 and 3.0 seconds. A GUI launched
with --vmess preserved two already-stored profiles and appended the third.
Output validation artifacts record these checks. A session tray host is required; plain X11 legacy tray
protocols are not implemented.

## Five-profile list regression — 2.0.1

Reproduced five saved profiles occupying a single screen row. The repeated root's
visible binding caused Slint to insert a zero-height visibility wrapper; all row
geometry overlapped. The pre-fix pointer test clicked the first row and received
the last profile's source index (4 instead of 0). Removed root visibility and
filtered the model in Rust. All five Use buttons are tested independently, in
normal and compact layouts at 100% and 150% scale. Filtering and sorting tests
retain original source indices for row actions. Profiles are not rewritten.


## Online Linux installer validation — 2026-10-23

Added install.sh and scripts/install.py to install/update the latest stable GitHub
Linux archive. Per-user version directories, checksum validation, safe archive
extraction, atomic current selection, process/installer guards and retained
profiles/previous versions. Dependency mappings: apt, pacman (full upgrade), dnf,
zypper and glibc Void/xbps; other compatible glibc systems can use --no-deps.
Unsupported architectures, musl, old glibc and missing standard ELF loader fail
before installation. This is installer tooling for existing release assets; no
new app version or replacement of published release assets is needed.

Validation: 10 isolated installer regressions passed; Bash syntax and Python 3.8
syntax checks passed. Actual published v2.1.0 Linux archive downloaded over HTTPS,
verified against release SHA256SUMS, installed to a temporary home and launched
with --help successfully. No host packages, user installation, routes or profiles
were modified by that smoke check. Package-manager transactions on all mapped
distributions remain unexecuted; mappings are not an all-distro validation claim.
A focused installer CI workflow is included for subsequent changes.


## Existing-installation and launcher correction — 2026-10-23

Default target detection now updates installed Arch/DEB packages through the
package manager, and recognized unowned /usr/lib manual installations in place
with a backup. User installs remain the fallback; --target user explicitly opts
into a separate installation. Canonical sni-tunnel.desktop replaces stale hidden
overrides, previous installer IDs are hidden to avoid duplicates, desktop database
is refreshed, and launcher permissions are 0644. The local user launcher/updater
were repaired without restarting the running tunnel. Fourteen isolated tests
passed, including native Arch/DEB transaction selection, retained user data and
replacement of a hidden launcher. desktop-file-validate passed for the repaired
local entry. Native package transactions were simulated, not run on the host.


## Android phone and TV 1.0.0 — 2026-10-23

Added a separate native Android project with phone and TV flavors, VMess imports,
independent SNI/Host configuration, Keystore AES-GCM profile storage, foreground
VpnService, official SHA256-pinned Xray/Hev runtimes, real HTTPS health checks and
manual disconnect cleanup. Android 10+ / arm64-v8a and x86_64; TV Leanback entry,
landscape UI and D-pad focus. Signed APKs built locally with a retained private
key; no secrets committed. Both release lint gates passed; eight unit tests per
flavor passed. API 35 x86_64 emulator: both installs/launches, import, VPN consent,
synthetic VMess TCP forwarding, health verification and disconnect passed; TV
UDP DNS and phone process-restart profile retention passed. Physical devices,
IPv6/handoff/soak and Play Store distribution remain untested. See
android/TEST_RESULTS.md for exact scope. Desktop release assets are unchanged.


## Stability and matching Android workflow — 2.1.1 / Android 1.0.1

Fixed Linux socket timeouts accidentally divided by six, introduced a 60-second
sustained diagnostic-failure window and 15-second event/retry spacing, retained
connected controls during degraded health, refreshed a changing egress baseline
only after current verification, and kept route/firewall integrity repair prompt.
A portable local hotfix was staged without restarting the user's active session.
131 Rust tests passed (5 opt-in tests ignored), plus 21 helper tests. Release
version metadata/packages advance to 2.1.1; previous release assets are preserved.

Android 1.0.1 adds the six desktop menu sections, individual server actions,
separate SNI profiles, complete editor, diagnostics/events and saved monitoring/
recovery preferences. Startup verifies forwarding before creating device VPN.
Two-provider bounded probes, explicit verification name / trusted leaf pin,
credential-redacted core logs, independent stats and bounded retained-VPN
recovery replace the single-page generic-failure workflow. Fifteen unit tests
per flavor and both lint gates passed. API 35 emulator checks passed for both
menus, append/search/Use, proxy + ordinary TCP forwarding and disconnect;
untrusted custom-SNI TLS failed before VPN capture and an explicitly pinned
VMess/WebSocket/TLS test forwarded correctly. APK signatures match 1.0.0.
See android/TEST_RESULTS.md for exact scope and physical-device limitations.

Release CI also gates Android unit/lint/build checks and validates the locally
tested, signed APKs against android/release-assets.json before publication.
Publication is pending the workflow's successful final gates at this checkpoint.
