# Five-profile list hotfix — 2.0.1

Reproduced the user's five-saved/one-visible report. The repeated ListView root
had a visible binding; Slint lowered it to a zero-height wrapper and all five
rows overlapped. Added a pointer regression that fails on the old layout
(first click reaches source index 4 instead of 0), moved search filtering into
the model, and kept stable source indices after sorting/filtering. Added normal
and compact five-row tests at both scales. This patch is released as 2.0.1,
preserving the already-published 2.0.0 and all existing user profile data.

# Append-only import, native tray and live latency — 2026-10-22

Imports now always append with fresh IDs and unique labels across GUI and CLI.
Fixed the GUI --vmess launch path that constructed an ephemeral one-profile
config, stable-ID editor saves, silent partial batch imports and CLI/GUI mutation
races. Added the native Linux StatusNotifierItem tray, safe hide/restore/quit
behavior, and a three-second TCP latency reader with bounded probes and stale
result rejection. Regression suite: 120 passed, 5 ignored. Native Waybar checks passed: icon registration, close/hide, restore, Quit,
deregistration, GUI --vmess profile preservation and 3.0-second TCP cadence.
The portable rebuild and native tray checks passed; 2.0.0 was published successfully. The first GitHub Windows build passed;
Linux tests/build passed but the AppImage startup smoke failed. Added missing
X11 runtime dependencies and startup-log diagnostics before retrying CI.

# Server import follow-up — 2026-10-22

Found that import feedback appeared only in the Overview detail, while an
over-broad host/port/transport duplicate check skipped distinct accounts and
SNI/Host/path variants. Results now stay visible on Servers; empty input and
clipboard failures are explicit, successful imports clear the search filter,
all imports append instead of replacing existing entries, ID collisions are reassigned,
and persistence failure does not mutate the in-memory config. All 121 default-
feature Rust tests passed; 5 opt-in tests remain ignored.

# GitHub release preparation — 2026-10-22

Connected repository access is available. Prepared a checked source import and
an Actions workflow that builds Windows/Linux, tests the native menu and profile
paths, assembles AppImage/DEB/source assets, uploads a draft and then marks it
Latest. The direct connector lacks a release-upload action; the repository's
own workflow performs the authorized publication. Private stores/logs/backups
are excluded. Heavy vendor archives remain on the prior verified release and
are restored by pinned SHA-256s for builds and complete source assets.

# Sidebar interaction follow-up — 2026-10-22

Reproduced the first-click menu failure with native Slint pointer events. Moved
the keyboard focus target out of pointer hit testing, retained Space/Enter and
accessibility activation, and centered icons/text/markers in equal rows with
constant border geometry. All six first-click actions and layout/scroll checks
passed at 100% and 150% scale. The Linux release is rebuilt for the installed
client and AppImage; profile and networking behavior are unchanged.

# Installed UI and scale follow-up — 2026-10-22

At the user's explicit request, updated the existing manual Linux installation's
launcher to native V2. Original installed files remain, and a verified root-owned
backup was created under /var/backups/sni-tunnel-ui-20261022-140839. The old helper
stopped gracefully; its routing journal and sni0 routes were gone before restart.
Both saved profiles were activated in the separate V2 store and verified; old
profile files remain unchanged. V2 opened through /usr/bin/sni-tunnel.

The user then requested a smaller UI. Installed and AppImage launchers now
use 100% scale by default, preserving explicit SLINT_SCALE_FACTOR overrides.
The installed window was restarted disconnected, with profile bytes unchanged.
No new live TUN connection was started during the UI/scale update.

# AppImage follow-up — 2026-10-22

Added a separately packaged Linux x86_64 AppImage containing the native V2 Slint
client, without the legacy Tk launcher. Official versioned build tools/runtime
are hash-pinned. A verified executable cache outside FUSE supports the existing
pkexec helper; Linux autostart selects the persistent AppImage path.

The AppImage was tested using isolated data/config/cache: normal and extraction
fallback launches, V2 binary/payload hashes, paths with spaces, autostart on/off,
cache tamper rejection, concurrent first launch and GUI startup. The installed
old client was already connected; no live network/TUN test was started and its
processes were left alone. Earlier V2 packages remain available separately.

# SNI Tunnel V2 progress — 2026-10-22

V2 is developed in a separate `SNI-Tunnel-V2` working copy. The original source
and outputs remain available. The baseline is tagged
`checkpoint-before-v2-20261022`. The installed old Linux app and its user stores
have not been replaced or modified.

Completed V2 implementation:
- Original dark Slint interface on both platforms: dashboard, real traffic history,
  vector sidebar, server search/sort, settings, logs and scrollable editor.
- Independent V2 data/install/autostart identifiers and concurrent-session guards.
- Linux native GUI bridge to the existing elevated Xray/BadVPN helper, staged
  connection health, bounded recovery, and actual kernel traffic counters.
- Linux proxy capture/restore journal, DNS preference propagation and protection
  of resolver changes made outside the app.
- Credential-safe migration with permanent backups, strict loading, corrupt-store
  overwrite protection, and regression fixes for TLS inheritance from legacy SNI.
- Extended VMess field preservation, editor validation/unsaved-change handling,
  settings-save rollback and reduced hidden-page/rendering work.
- Separate Windows and Linux release builds, original branding and bundled runtime
  dependencies; source and user configuration remain separate.

Validation executed: 116 Rust tests and 12 Python tests passed. Both real saved
Linux profiles migrated through the packaged binary and were compared without
printing secrets; source files remain hash-identical. Packaged local-proxy VMess
HTTPS forwarding passed, followed by 12/12 HTTPS requests over 63.58 seconds and
graceful disconnect. Neither check changed system routes or DNS. Windows was
cross-compiled, not run. See TEST_RESULTS.md for exact evidence and remaining gates.

Replacement is pending platform validation and the user's explicit approval.
No official uninstall mechanism was found for the existing manual Linux install;
removing its directory is not an acceptable substitute. See MIGRATION.md.

The entries below are inherited previous-version history and do not describe
V2 tests executed on this Linux host.

---

# PROGRESS.md — SNI Tunnel

## 2026-10-22 — Configuration editing, recovery and desktop polish

Implemented against the complete v0.4.3 release source. The public GitHub tree
contains only README.md, so the application source was taken from its release
asset sni-tunnel-source.zip. Before modifications, a local Git commit and tag
`checkpoint-before-fixes` captured the pristine source (ea4f9cb).

Confirmed defects and changes:

- Packaged Linux uses Tk, rather than the Rust Slint editor. Added a field editor
  for address, port, UUID, transport, TLS, SNI, Host/path, ALPN, fingerprint,
  MUX and XUDP, plus a collapsible JSON editor for every other imported field.
  Unknown fields and untouched field types survive edits. Mouse-wheel/X11
  buttons, touchpad wheel events, Page Up/Down and focus navigation scroll the
  form; Save, Cancel and Test connection remain outside it. The editor test
  explicitly labels its TCP-only result; a successful TCP test does not claim
  verified VMess forwarding.
- Linux profile writes previously truncated profile.json. Replaced them with
  private temporary files, fsync and same-directory atomic replacement. Invalid
  or unreadable existing settings block saving. Unknown top-level settings are
  retained. Import/Add no longer accidentally targets the active server.
- Rust persistence now uses private unique temporary files, flushes writes and
  directories, and refuses to overwrite unreadable/corrupt/newer configurations
  or credentials that cannot be recovered. Slint editor rolls back its in-memory
  profile when saving fails. Existing imported XUDP settings can be retained
  even when the installed core cannot currently run them.
- Linux secret-key creation previously overwrote a key after a read failure and
  allowed simultaneous first saves to use different keys. Publish a complete
  private key without replacement; refuse missing keys during decryption and
  damaged existing keys. A 32-thread first-save regression test passes.
- Linux periodic verification previously disconnected on one failed probe.
  Mark the tunnel degraded, retry probes, and tear down/rebuild after three
  consecutive failures. Log child names/exit codes and distinguish a missing
  physical route from forwarding failure. Core configuration errors are fatal.
  Recovery uses 3/6/12/24/48-second waits and at most five retries, retaining the
  same temporary profile and options. Manual stop interrupts the wait. Existing
  route/DNS ownership rules and single-helper lock are retained. DNS lookup or
  unavailable routes during recovery also use the bounded retry budget.
- Rust recovery now uses bounded exponential delays and retains the endpoint
  snapshot. Only a stable 120-second session resets its budget; brief successful
  probes cannot sustain an endless reconnect cycle. Disconnect cancels recovery.
- VMess import/export retains explicit MUX/XUDP values. CLI imports previously
  overwrote imported settings and explicit flags with global defaults; removed
  that overwrite. Blank profiles still use the existing global defaults.
- Polished Slint palette, navigation, cards, dashboard profile/SNI context,
  server search, log search, editor content sizing and keyboard scrolling.
  Settings content scrolls with its Save action outside. Packaged Linux also
  receives updated colors, a prominent Connect action, search, severity colors,
  log modes and active-server/SNI metrics. Traffic counters remain measured.
- Linux build.sh runs all Linux regressions and uses LF line endings; the
  release source's CRLF version failed `sh -n`.

Validation actually run:

- `cargo test --locked --no-default-features`, using isolated test data:
  111 passed (102 unit, 2 additional persistence/key integration, 7 workflow).
  Five existing live-core integration tests remain ignored.
- `python3 -m unittest discover -s packaging/linux -p 'test_*.py'`:
  12 passed, including the live X11 Tk editor edit/scroll/save/reopen/restart
  test at 680x700, 480x360 and 1000x900. It checks exact preservation of all
  original fields except the edited address and verifies footer visibility.
- `cargo check --locked` with GUI enabled: passed.
- `cargo build --locked` with GUI enabled: passed on Linux.
- Slint startup under X11 and native Wayland: each remained running for five
  seconds until the smoke test terminated it. Graphics-driver fallback warnings
  were logged; this is only a startup smoke check, not complete interaction QA.
- Separate CLI processes imported, edited and reloaded a dummy profile; verified
  credentials, ea.com SNI, MUX concurrency and 0600 configuration permissions.
- Generated Linux compatibility configuration passed the bundled unchanged
  Xray 1.6.1 `run -test` validation. No live server was contacted for this check.
- Rust formatting check for changed Rust files, Python compilation, shell syntax
  check, and Git whitespace check with CRLF support passed.

Remaining unverified / existing constraints:

- No real VMess server or user credentials were available. Sustained traffic,
  real core crashes, Wi-Fi loss/reconnect, suspend/resume, elevated TUN route/DNS
  installation and cleanup, NetworkManager interaction and quota behavior still
  require live testing. The source defects above were confirmed; they do not
  prove the cause of every disconnection on the user's machine.
- Windows compilation, Windows GUI/DPAPI behavior and EXE installer execution
  were not run: this environment has no Windows Rust target or Windows runtime.
  DEB/RPM/EXE installers were not rebuilt. Existing packaging paths and cores
  were retained; deliverables are source and patch, not replacement installers.
- Packaged Linux retains its legacy Xray 1.6.1 + BadVPN datapath, including its
  existing VMess/WebSocket/TLS restriction. Unsupported XUDP requests produce
  compatibility errors; saved settings are retained rather than silently reset.
- Slint editor interaction on Windows and native Wayland is unverified. The
  shipped Tk editor was exercised through X11/XWayland, not native Wayland Tk.
- Existing Linux-only warnings in clipboard/tun code remain; no dependencies,
  credentials, custom SNI, core binaries or routing policy were replaced.

Legend: [x] done & tested · [~] partial · [ ] todo

## Stage 1 — Headless VMess + custom TLS SNI  ✅ COMPLETE (tested)

Deliverable: a CLI that builds a real VMess tunnel through a pinned Xray-core,
with independent TLS SNI / Host overrides, and **verifies actual forwarded
traffic**. No GUI yet (intentional).

### Done
- [x] Rust GNU toolchain + w64devkit build environment (`AGENTS.md` §1).
- [x] Data model with independent address / SNI / WS-Host fields
      (`src/model.rs`, `resolve_endpoint`).
- [x] `vmess://`, base64 subscription and JSON import (`src/vmess.rs`).
- [x] Xray config generation incl. tls/ws/h2/grpc + custom SNI (`src/xray.rs`).
- [x] Xray subprocess management (`src/xray.rs`, `XrayProcess`).
- [x] Pinned download + SHA-256 verify + unzip (`src/corebin.rs`).
- [x] Local HTTP + SOCKS5 proxy with real byte counting
      (`src/localproxy.rs`, chained into Xray's private SOCKS inbound).
- [x] Diagnostics: DNS, TCP, TLS (with SNI), proxied HTTP (`src/netcheck.rs`).
- [x] DPAPI-sealed config store (`src/store.rs`, `src/dpapi.rs`).
- [x] Secret scrubbing in all logs (`src/logging.rs`, `src/util.rs`).

### Evidence
- `cargo build` → ok (warnings only: unused stage-2/3 helpers).
- `cargo test` → **14 passed, 0 failed**.
- `sni-tunnel.exe selftest` → **SELF-TEST PASSED**
  (`proxied HTTP GET -> 200 (345 ms): 104.28.245.1`) — proves download+verify,
  core start, SOCKS inbound, our proxy, and a live HTTPS round-trip.
- `check` against a bogus server → staged `[fail] TCP/TLS/FORWARD` messages,
  `TUNNEL NOT VERIFIED`, exit code 1 (no fake "connected").

### Relevant files
`src/{main,model,vmess,xray,corebin,session,localproxy,netcheck,stats,logging,store,dpapi,util}.rs`
`Cargo.toml`, `build.rs`, `ui/app.slint` (placeholder, unused this stage),
`.cargo/config.toml`, `AGENTS.md`.

### Errors / caveats
- None blocking. `ui/app.slint` is a leftover placeholder; the GUI stage will
  replace it. `slint` is compiled but not used yet.
- Build machine has no VMess server, so a **real VMess handshake must be tested
  by the user**: `sni-tunnel check --vmess "<link>" --sni <allowed-host>`.
  The plumbing it depends on is validated by `selftest`.

## Stage 2 — Profile store + CLI  ✅ COMPLETE (tested)

Deliverable: manage multiple VMess servers and named custom-SNI profiles from
the CLI, persisted with DPAPI-sealed UUIDs; `check`/`connect`/`proxy` now run
against the *selected* stored profile when `--vmess` is omitted.

### Done
- [x] `src/storecli.rs`: `list`/`profiles`, `add`/`import`, `remove`,
      `use`, `export`, and `sni list|add|use|off|remove`.
- [x] Reference resolution by 1-based index, id, or (substring) name.
- [x] Duplicate detection on address/port/network; `--select` on import.
- [x] Removing a server also removes its SNI profiles and fixes selection.
- [x] `export` strips UUIDs (`store::export_scrubbed`).
- [x] `check`/`connect`/`proxy` fall back to the stored selection when
      `--vmess` is absent (CLI overrides still apply on top).

### Evidence
- `add` → `list` shows `Tokyo-01  1.2.3.4:443 ws/tls sni=cdn.example.com`.
- `config.json` contains `uuid_sealed` and **no** plaintext UUID (checked).
- `sni add --name cdn-front --sni front.example.net --ws-host front.example.net`
  → `sni use 1` → `check` (no `--vmess`) resolved dial `1.2.3.4` with
  `SNI=front.example.net, Host=front.example.net` (independence proven from disk).
- `proxy` (no `--vmess`) used stored ports 2080/1080, set and restored the
  system proxy (registry `enable=0 server=http://127.0.0.1:12334` restored).
- `sni off`, `use`, `remove` verified; `cargo test` → 16 passed.

### Relevant files
`src/storecli.rs`, `src/main.rs` (dispatch + stored-config fallback),
`src/store.rs`, `src/dpapi.rs`.

### Errors / caveats
- Fixed during development: option keys kept their `--` prefix in the parser.
- Test data (fake `1.2.3.4` server) was removed afterwards; config is clean.

## Stage 3 — Engine + system proxy  ✅ COMPLETE (tested)

Deliverable: an actor-style engine that owns the tunnel lifecycle, sets/restores
the Windows system proxy, reports rich status, detects core death and can
auto-reconnect.

### Done
- [x] `src/sysproxy.rs`: capture/apply/restore WinINet proxy
      (`ProxyEnable/ProxyServer/ProxyOverride/AutoConfigURL/AutoDetect`) +
      `InternetSetOptionW` refresh. Crash-safe on-disk snapshot.
- [x] Crash recovery: stale snapshot restored on engine start and via
      `sni-tunnel proxy-off`.
- [x] `src/engine.rs`: `Phase { Disconnected, Preparing, CoreReady, Verifying,
      Forwarding, Disconnecting, Error }`; **CoreReady ≠ Forwarding** — forwarding
      is only claimed after a real proxied HTTP 200.
- [x] Commands: Connect / Disconnect / TestConnection / RefreshIp / Shutdown.
- [x] Stats events every second (real byte counters + per-second speeds).
- [x] Core-death detection + optional auto-reconnect (max 5 tries, 3 s backoff).
- [x] CLI: `sni-tunnel proxy`, `sni-tunnel proxy-off`, `--mode`, `--auto-reconnect`.

### Evidence
- `cargo test` → **16 passed, 0 failed**.
- Runtime (Win11, PowerShell): started `proxy` on ports 2099/1099 →
  registry `ProxyEnable=1`, `ProxyServer=http=127.0.0.1:2099;https=…;socks=…`,
  `ProxyOverride=<local>`; **killed the process** (simulated crash) → settings
  stayed; `proxy-off` → restored the *exact* pre-existing values
  (`enable=0 server=http://127.0.0.1:12334`, MATCH true/false/true → all true).
- No orphaned `xray.exe`/`sni-tunnel.exe`, backup file removed, log shows
  `[CORE UP (unverified)] → [VERIFYING] → [ERROR]` (honest, no fake connect).

### Relevant files
`src/{engine,sysproxy}.rs`, `src/main.rs` (`proxy`/`proxy-off`), `src/session.rs`.

### Errors / caveats
- Graceful Ctrl+C path uses the same `sysproxy::restore` as the tested
  `proxy-off` path.
- `--mode tun` is rejected with a clear message (not implemented yet).

## Stage 4 — GUI (Slint, dark)  ✅ COMPLETE (tested)

Deliverable: a Slint desktop front-end over the Stage-3 engine, with tabs for
dashboard, servers, SNI editor, logs and settings.

### Done
- [x] `ui/app.slint`: dark themed window; custom tab bar (no TabWidget) with
      Dashboard / Servers / SNI / Logs / Settings.
- [x] Dashboard: phase chip (colour-coded), detail, external IP, live up/down
      speed + totals, active connections, uptime; Connect/Disconnect/Test/
      Refresh IP buttons.
- [x] Servers tab: import box (`vmess://`/JSON), list with select highlight,
      Use / Remove.
- [x] SNI tab: list + editor (name, SNI, WS host/path, dial address/port,
      no-TLS, allow-insecure); Use / Remove / Disable override.
- [x] Logs tab: live log bus (500-line ring) + Clear.
- [x] Settings tab: HTTP/SOCKS ports, system-proxy toggle, auto-reconnect,
      log level; persisted with `store::save`.
- [x] `src/gui.rs`: engine on its own Tokio runtime in a background thread; UI
      drives it via `EngineHandle` commands and polls engine/log broadcast
      channels with a Slint `Timer` (150 ms). Graceful `Shutdown` on close.
- [x] `sni-tunnel gui` CLI command (headless CLI unchanged).

### Evidence
- `cargo build` clean (no new warnings); `cargo test` → **19 passed** (3 new
  GUI import/merge tests).
- GUI launched from the console — process stayed alive with empty stderr, and
  the log recorded the engine booting (`[DISCONNECTED] idle`); the Windows
  system proxy was left untouched (no auto-connect).
- Re-launched with one stored server + one SNI profile present → still healthy,
  empty stderr. Test data removed afterwards.

### Relevant files
`ui/app.slint`, `src/gui.rs`, `src/main.rs` (`gui` command), `Cargo.toml`.

### Errors / caveats
- `Phase::CoreReady` still shows as an amber "unverified" chip; "FORWARDING"
  (green) is only shown after a real proxied HTTP 200 — no fake status.
- Routing/TUN tab is not present yet (flows into Stage 5); only system-proxy
  mode is exposed.

## Stage 5 — Tray, autostart, packaging  ✅ COMPLETE (tested)

Deliverable: run as a background tray app, autostart with Windows, and produce
a distributable release zip. (TUN mode was split out into Stage 6 — see below.)

### Done
- [x] `src/tray.rs`: system tray on a dedicated thread with its own Windows
      message pump. Menu (Show / Connect / Disconnect / Test / Quit) and
      left-click / double-click events are drained on the UI thread via
      muda's global channels (`tray::poll`). Icon is generated in code.
- [x] `src/gui.rs`: tray wired into the GUI; closing the window hides it to the
      tray (`Window::on_close_requested`) when "minimize to tray" is on;
      tray actions drive the engine; "Quit" calls `slint::quit_event_loop()`;
      a "Hide to tray" dashboard button. Logs `system tray ready`.
- [x] `src/autostart.rs`: "start with Windows" via the per-user
      `HKCU\...\Run` key (`"<exe>" gui`); `is_enabled`/`set_enabled` +
      Settings checkbox. CLI: `sni-tunnel autostart [on|off|status]`.
- [x] `src/console.rs`: hides the console window on GUI startup **only** when
      we are the console's sole owner (double-click / autostart) — a terminal
      launch keeps its console. No args now launches the GUI.
- [x] Packaging: `scripts/build-dist.ps1` builds release and produces
      `dist\sni-tunnel-<ver>-windows-x64.zip` (exe + README); user `README.md`
      with install/usage/security/troubleshooting.

### Evidence
- `cargo test` → **20 passed** (new autostart `run_value` test).
- `sni-tunnel autostart on` → registry `HKCU\...\Run\SniTunnel` =
  `"C:\dev\sni-tunnel\target\debug\sni-tunnel.exe" gui`; `status` → enabled;
  `off` → value removed, `status` → disabled; bad arg → exit 2.
- GUI (debug **and** release) launched → alive, empty stderr, log shows
  `[DISCONNECTED] idle` + `system tray ready`.
- `scripts\build-dist.ps1` → `dist\sni-tunnel-0.1.0-windows-x64.zip`
  (6.5 MB; exe 13.3 MB + README); release exe runs and opens the tray.

### Relevant files
`src/tray.rs`, `src/autostart.rs`, `src/console.rs`, `src/gui.rs`,
`ui/app.slint`, `src/main.rs`, `scripts/build-dist.ps1`, `README.md`,
`Cargo.toml`.

### Errors / caveats
- **Release build crash fixed**: fat LTO (`lto = true`, `codegen-units = 1`)
  made `rustc` die with `STATUS_STACK_BUFFER_OVERRUN` (0xc0000409) while
  compiling Slint deps (usvg/rowan/moxcms). Switched the release profile to
  `lto = "thin"`, `codegen-units = 16`; release now builds in ~10 min.
- Tray tooltip is static (muda/tray-icon `TrayIcon` is not `Send`); live
  status is shown in the window, not the tooltip.
- "Minimize" to tray is implemented as close-to-tray (Slint exposes no
  per-window minimize event to hook).

## Stage 6 — TUN mode (wintun, via Xray's native `tun` inbound)  [x] COMPLETE (tested)

Deliverable: a real device-level tunnel. `xray.exe` creates the wintun
adapter itself (`"protocol": "tun"`), so there is **no tun2socks dependency**
any more — one binary does the adapter, the stack and the outbound.

### Done
- [x] `src/tun.rs`: Xray `tun` inbound + single outbound config, wintun adapter
      address/DAD handling, route planning, uplink discovery, TUN verification.
- [x] Administrator gating (`tun::is_elevated`, token elevation check) with a
      clear error instead of a cryptic failure.
- [x] Adapter setup: `netsh interface ipv4 set address` in the **keyed** form
      (`source=static addr= mask=`) plus `set interface ... dadtransmits=0`
      (otherwise the address stays `Tentative` and cannot be bound to).
- [x] Routing: two `/1` routes shadow the default route without deleting it
      (a crash leaves the machine online); the VPN server keeps a `/32` route
      via the physical gateway. Routes are installed with
      `netsh interface ipv4 add route ... interface=<name>` because
      `route.exe add` binds a wintun next hop to the wrong interface.
- [x] **Loop avoidance**: the outbound is pinned to the physical uplink with
      `streamSettings.sockopt.interface` (Windows `IP_UNICAST_IF`), discovered
      from the default route. Without it Xray's own traffic is routed back into
      the tunnel and the connection times out.
- [x] "Verified" for TUN (no local proxy listener): a real HTTPS GET whose
      **source address is bound to the TUN IP** (`reqwest::local_address`),
      so the request provably traverses the tunnel. The egress IP is read from
      Cloudflare's `/cdn-cgi/trace`.
- [x] `--mode tun` in the engine (`Live::Tun` variant), in `check`/`connect`
      (`tun_check`) and in `proxy`; GUI checkbox "TUN mode: route the whole
      device (needs Administrator)".
- [x] Teardown removes the routes *before* stopping Xray, so traffic is never
      pointed at a vanishing interface.
- [x] `tun-selftest` (+ `--full` for the full `/1` routing table) CLI command.
- [x] tun2socks dropped from `corebin.rs` (and from the machine's bin dir).

### Evidence
- `cargo test` → **32 passed** (12 new `tun` unit tests: config builders,
  netsh/route command shapes, default-route parser, alias parser, trace
  parser, exclusion planning).
- `sni-tunnel.exe tun-selftest` → **TUN SELF-TEST PASSED**
  (`TUN HTTPS GET -> 200 (297 ms)`, `egress IP seen by the remote: 197.47.144.87`)
  — real TLS 1.3 round trip through wintun, with only `1.1.1.1/32` routed.
- `sni-tunnel.exe tun-selftest --full` → **TUN SELF-TEST PASSED** with the two
  `/1` routes installed (whole-device routing); afterwards **no leftover
  routes** (`route print -4` clean) and the machine's internet works again.
- Independent cross-check with `curl --interface 198.18.0.1
  https://1.1.1.1/cdn-cgi/trace` → HTTP 200, `colo=CDG`, `warp=off`,
  `ip=51.91.248.209` — proves the packets really leave the physical NIC.
- GUI (debug build) launched → alive, window "SNI Tunnel", no orphan
  processes, no system-proxy side effects.

### Relevant files
`src/tun.rs` (new), `src/engine.rs`, `src/xray.rs` (`vmess_outbound`),
`src/corebin.rs`, `src/main.rs` (`tun-selftest`, `tun_check`), `src/gui.rs`,
`ui/app.slint`, `AGENTS.md`.

### Errors / caveats
- **tun2socks was dropped mid-stage.** It works, but its Windows driver is
  registered as `tun://` (not `wintun://`), it needs `--long` flags (pflag
  mis-parses `-device` as the `-d` shorthand), and it cannot avoid the
  routing loop by itself. Xray's own `tun` inbound does all of it.
- **Xray's `gateway` setting did not apply the adapter address** (the adapter
  came up with an APIPA address), so the address is always set by us with
  netsh.
- **Cloudflare WARP blocks TUN traffic.** Its WFP filter driver rejects
  connections from a foreign TUN adapter with `WSAEACCES (10013)`. The
  self-test was re-run with the WARP service stopped (and restored
  afterwards). Documented in `AGENTS.md` §5.
- TUN mode is **IPv4 only**; IPv6 traffic is not captured and would leak out
  of the physical interface. TUN mode also reports no byte counters (traffic
  is measured by Xray, not by our local proxy).

## Stage 7 — Server management & configuration editor  [x] COMPLETE (tested)

Deliverable: a full VMess server manager. Every imported server can be
edited in a dedicated configuration window, exported, duplicated, tested and
reconnected — with the TLS SNI, the WebSocket Host, the WebSocket path and
the dial address kept strictly independent.

### Done
- [x] `src/editor.rs` (pure, unit-tested): profile ⇄ editor fields, validation,
      extra-header parsing, profile duplication.
- [x] `ui/app.slint`: new `ServerEditor` window — General / Transport /
      TLS-SNI / Advanced sections, transport-specific fields shown only when
      relevant, dropdowns for transport, encryption, fingerprint, TCP/QUIC/KCP
      header types, "Show advanced settings" toggle, Save / Cancel / Test.
- [x] Servers tab: **Add, Edit, Delete, Duplicate, Import from clipboard,
      Export as vmess://, Select active, Test connection**, plus
      **double-click a row to edit it**. Each row shows name, address, port,
      transport, configured SNI, status and latency.
- [x] `src/vmess.rs`: **export** to a `vmess://` link (credentials and every
      transport parameter preserved) and a second link format
      (`base64(auto:uuid@host:port)?…`) is now parsed as well.
- [x] `src/xray.rs`: config generation for **tcp, ws, h2, grpc, httpupgrade,
      xhttp, quic, kcp**, plus extra WS headers, mux, per-server DNS and
      per-server proxy ports.
- [x] `src/clipboard.rs`: clipboard read/write without new dependencies
      (Win32 on Windows, `wl-paste`/`xclip`/`xsel` on Linux).
- [x] **Custom SNI**: imported servers show their SNI (verified with the
      `ea.com` server), the field is editable at any time, and changing it
      never touches the address or the WS Host.
- [x] **Save flow**: validate → persist (`store::save`) → the new config is
      used on the next connect → if the *active* server was edited while
      connected, the app asks whether to reconnect (`Command::Reconnect`).
- [x] **Diagnostics**: `netcheck::diagnose()` runs DNS → TCP → TLS(SNI) →
      VMess forwarding and reports the first failing stage; the engine uses it
      instead of the generic "core is up but no traffic is being forwarded".
      A certificate that does not cover the SNI gets an actionable hint.
- [x] **Linux portability**: every platform-specific module now has a
      portable implementation (`dpapi_linux`, `sysproxy_linux`,
      `autostart_linux` → `.desktop` entry, `tray_linux`, `elevate_linux` →
      `pkexec`); `winreg`/`tray-icon`/`windows-sys` moved to
      `[target.'cfg(windows)'.dependencies]`.

### Evidence
- `cargo test` → **53 passed** (new: editor round-trips, SNI independence,
  validation, header parsing, vmess export/import round-trip, the new link
  format, per-transport Xray config, mux/DNS, store round-trip, TLS hints).
- Import of a `vmess://` link with `sni=ea.com` → `config.json` keeps
  `sni=ea.com` while `ws_host` stays `front.example.com` (independence).
- `config.json` shows `uuid_sealed` (a DPAPI blob), never the raw UUID.
- GUI (debug) launches with the new UI, renders the server list and leaves
  no orphan processes.
- The new Linux platform modules were **compiled for
  `x86_64-unknown-linux-gnu`** in an isolated crate (`cargo check`).

### Relevant files
`src/editor.rs` (new), `src/clipboard.rs` (new), `src/{model,vmess,xray,store,netcheck,engine,gui}.rs`,
`ui/app.slint`, `src/{dpapi,sysproxy,autostart,tray,elevate}_linux.rs` (new),
`Cargo.toml`.

### Errors / caveats
- **The whole crate could not be compiled for Linux in this environment**:
  Slint needs the system `fontconfig` development files and `native-tls`
  needs OpenSSL headers, and this is a Windows machine. The platform modules
  themselves were verified with `cargo check --target x86_64-unknown-linux-gnu`
  in an isolated crate. A full Linux build additionally needs
  `libfontconfig1-dev`, `libssl-dev` and a Wayland/X11 runtime.
- On Linux the UUID is *obfuscated* (SHA-256 keystream + a `0600` key file),
  not encrypted like Windows' DPAPI.
- The system proxy and the tray icon are Windows-only; on Linux the app logs
  that and the local proxy has to be pointed at manually.
- The editor's dropdowns bind to their string property, so the value shown is
  always the one that will be saved.

## Stage 8 — Application icon  [x] COMPLETE (tested)

Deliverable: the supplied artwork ("Neon Cyber Shield Tunnel") is now the icon
of the app — window, tray and executable.

### Done
- [x] `assets/` holds the generated icon assets: `icon.png` (512 px master),
      `icon-256.png` (window), `icon-32.rgba` (tray, raw RGBA — no image
      decoder needed at runtime) and `icon.ico` (6 sizes, 16→256).
- [x] `scripts/make-icon.ps1` regenerates all of them from a master PNG.
- [x] **Window icon**: `ui/app.slint` sets `icon: @image-url(...)` on the main
      window and the server editor.
- [x] **Tray icon**: `src/tray.rs` uses the embedded 32×32 RGBA instead of the
      old code-drawn rounded square.
- [x] **Executable icon**: `build.rs` runs GNU `windres` (w64devkit) over
      `assets/app.rc` and links the `.rsrc` section into the binary. The step
      is skipped with a warning when `windres` is unavailable, so the build
      never breaks.

### Evidence
- `cargo test` → **53 passed** (unchanged).
- `objdump -h` on the executable shows a `.rsrc` section (0x5a720 bytes) that
  was not there before.
- `Icon::ExtractAssociatedIcon()` on the built exe returns the new artwork
  (verified by extracting it to a PNG and looking at it).
- Screenshot of the running GUI: the title bar shows the new shield icon.
- The 32×32 RGBA handed to the tray API was rendered back to a PNG and
  matches the artwork (and the R/B channels are in the right order).

### Errors / caveats
- GNU `windres` cannot read PNG-compressed `.ico` files ("read of N returned
  6"); the generator therefore writes classic DIB entries.
- PowerShell unrolls a returned `byte[]`, which silently produced a 108-byte
  `.ico`; the generator now returns `,$bytes` and keeps the `MemoryStream`
  open (`leaveOpen`).
- `windres` resolves the paths inside the `.rc` relative to its working
  directory, so the build script runs it from `assets/`.

## Stage 9 — Fix the Windows TUN failure (exit code 23)  [x] COMPLETE (tested)

Deliverable: TUN mode works again, and every failure now names the stage and
the reason instead of a generic message.

### The actual root cause

The reported error was

> could not start the TUN tunnel: failed to configure the TUN adapter.
> Xray exited with code 23 while setting up the TUN adapter

Exit code 23 only means *"the core refused to start"* — it does **not** point
at the adapter. The real reason was in Xray's own output:

```
Failed to build TLS config. > common/errors: The feature "allowInsecure" has
been removed and migrated to "pinnedPeerCertSha256".
```

Xray 26 removed the `allowInsecure` TLS option, but the config generator
emitted it unconditionally, so **every** TLS profile (TUN *and* system proxy)
made the core exit with code 23. The app then blamed the TUN adapter.

### Done
- [x] **Config migration**: `allowInsecure` is never emitted any more. A
      profile that asked for an unverified certificate now gets the modern
      equivalent: the server's leaf certificate is read and **pinned**
      (`pinnedPeerCertSha256`), which trusts exactly one certificate instead of
      everything. Verification is never disabled.
- [x] **Config validation before touching the network**: `xray run -test -c …`
      parses the configuration without opening a listener, so a broken config
      is reported before an adapter or a route exists.
- [x] **Accurate error reporting**: the core's stdout/stderr is captured and
      classified (`ConfigRejected`, `WintunMissing`, adapter failure, …) and
      the stage that failed is named.
- [x] **Staged setup** with a report per stage: configuration → core →
      adapter → addressing → DNS → routing → forwarding.
- [x] **Specific failures** (`TunFailure`): missing wintun driver, insufficient
      permissions, adapter creation failed, addressing failed, DNS failed,
      routing failed, VPN adapter conflict, forwarding failed.
- [x] **wintun.dll check**: the driver must exist next to `xray.exe` and its
      PE architecture must match the core's (`pe_machine`).
- [x] **Fallback**: if the TUN tunnel cannot be set up, the engine reports the
      reason and continues in **system proxy mode** — it never claims a
      connection that does not work.
- [x] **Crash-safe cleanup**: the installed routes are journalled
      (`tun-routes.json`); the next start removes routes left behind by a
      crash, so a crash can no longer leave the machine without connectivity.
- [x] **Loop prevention**: the outbound stays pinned to the physical uplink
      (`sockopt.interface`), and the VPN server keeps a `/32` route via the
      physical gateway.
- [x] **Deprecation clean-up**: the WebSocket `Host` header moved from
      `headers.Host` to the independent `host` field (Xray 26 deprecates the
      old form).
- [x] **Elevation**: the GUI asks for Administrator rights when a TUN
      connection is requested and restarts elevated; the CLI reports the
      missing privilege precisely. The rest of the app never needs elevation.
- [x] **Linux**: TUN mode is explicitly reported as Windows-only; the
      portable modules are unchanged.
- [x] `THIRD-PARTY-NOTICES.md`: Xray-core (MPL-2.0) and Wintun (its own
      license) — the driver comes from the **official** Xray release, is not
      redistributed in our archive and is checked against the core's
      architecture.

### Evidence
- `cargo test` → **60 passed** (new: `allowInsecure` is never emitted, the
  pin replaces it, the WS host form, the PE architecture reader, one test per
  failure message, the stage labels).
- `xray run -test` on the old config → `exit 23` with the exact Xray message;
  on the new config → `Configuration OK.`
- **Live test with the user's own server** (`sp3.vpnjantit.com:10001`,
  SNI `ea.com`, WS host `ea.com`, path `/vpnjantit`, UUID and transport
  untouched):
  * TUN mode → `[ok] FORWARD  TUN GET -> HTTP 200`, `TUNNEL VERIFIED`
  * system proxy mode → `[ok] FORWARD  proxied GET -> HTTP 200`
  * the certificate is pinned (`sha256 12ea3fb3…`) because the server's
    certificate does not cover `ea.com`.
- `tun-selftest` prints the stage report and passes.
- No leftover routes or adapters after a test.

### Relevant files
`src/tun.rs`, `src/xray.rs`, `src/netcheck.rs`, `src/engine.rs`, `src/gui.rs`,
`src/model.rs`, `src/store.rs`, `src/main.rs`, `THIRD-PARTY-NOTICES.md`.

### Errors / caveats
- The old `allow_insecure` flag is kept in the profile as the *intent*; the
  actual trust decision is made by pinning one certificate. The flag is
  migrated at connect time, the profile is not rewritten.
- A certificate that changes on the server invalidates the pin — the user
  then has to confirm the new certificate (the error says so).
- The full crate still cannot be compiled for Linux in this environment
  (Slint needs `libfontconfig1-dev`, native-tls needs OpenSSL headers); the
  platform modules are checked with `cargo check --target
  x86_64-unknown-linux-gnu` in an isolated crate.

## Stage 10 — parity with Nekoray / NetMod (WE Egypt quota)  [x] COMPLETE (tested)

Deliverable: the *configuration* and *routing* differences that separate this
client from the ones that were counted against the WE gaming quota are now
visible, comparable and fixable — without guessing and without touching the
server, the SNI or the credentials.

### What the comparison found

`sni-tunnel diagnose --compare <export>` compares this app's profile with a
configuration exported by Nekoray / NetMod / v2rayN (Xray JSON, sing-box JSON
or a Clash YAML) and reports every difference, marking the ones that are
**visible on the wire**. Against a Nekoray-style export of the same server the
only differences were:

| Setting | This app (before) | Nekoray-style export | Visible on the wire |
|---|---|---|---|
| **ALPN** | (none) | `h2, http/1.1` | yes - changes the TLS ClientHello |
| **MUX** | off (0) | on (8) | yes - changes the connection pattern |

Everything else was already identical: address, port, protocol, transport, TLS,
SNI (`ea.com`), fingerprint (`chrome`), WS host (`ea.com`), WS path
(`/vpnjantit`).

Both differences change what the operator sees on the wire, so they are the
most plausible reason the two clients were classified differently. They are
now applied through the **`nekoray` preset** (`sni-tunnel preset nekoray`),
which only sets ALPN and MUX - never the server, the SNI, the host or the
credentials. A **`netmod` preset** (sing-box style: MUX on, certificate
verified) is available as well.

### Done
- [x] `src/diag.rs`: the effective configuration with **every secret redacted**
      (including the base64 of a `vmess://` link), the settings that reach the
      wire, the traffic that bypasses the tunnel and the exit addresses.
- [x] `sni-tunnel diagnose` and `sni-tunnel diagnose --compare <file>`:
      compares against an Xray / sing-box / Clash export, credentials are never
      printed.
- [x] `sni-tunnel preset nekoray|netmod`: optional compatibility presets
      that only touch the transport settings.
- [x] **DNS is tunnelled now**: Windows asks *every* configured resolver in
      parallel, so the physical resolver answered directly and the DNS traffic
      left the machine outside the tunnel. The physical adapters now point at
      the tunnel's resolver while it is up, and the previous setting is
      restored on disconnect (and after a crash, from a journal).
- [x] **Idempotent route installation**: a route left over from a crashed run
      ("the object already exists") aborted the whole TUN setup.
- [x] **The TUN lock no longer misfires on a reused PID**: a stale lock is
      only honoured when the process is really an instance of this program.

### Evidence
- `sni-tunnel diagnose` prints the effective configuration with the UUID
  redacted (`***f2cbd57b***`), also inside the base64 share link.
- `diagnose --compare <nekoray export>` → before: `ALPN` and `MUX`; after the
  preset: **"none - the configuration is equivalent"**.
- `sni-tunnel verify-full` with the preset → **FULL TUNNEL VERIFIED**
  (ordinary request exits via 185.121.15.92, the adapter counters move, DNS
  resolves through the tunnel, IPv6 is blocked).
- DNS: `Wi-Fi` 192.168.1.1 → 1.1.1.1 and `PdaNet` 8.8.8.8 → 1.1.1.1 while
  the tunnel is up, both restored afterwards (and after a simulated crash).
- `cargo test` → **76 passed** (69 unit + 7 integration).

### Relevant files
`src/diag.rs` (new), `src/tun.rs`, `src/main.rs`, `src/util.rs`, `src/lib.rs`.

### Errors / caveats
- **The ISP's quota classification cannot be verified from here.** Only the
  user's WE usage records can confirm whether the traffic is now counted
  against the gaming quota. What this stage removes is the two
  configuration-level differences that were visible to the operator.
- The comparison reads Xray JSON, sing-box JSON and (best effort) a Clash
  YAML; a Clash file with an unusual layout may only be read partially.
- The presets change ALPN and MUX only. They are **not** a guarantee - the
  classification may also depend on things the client cannot see (the
  subscription, the traffic mix, the operator's own rules).

## Stage 12 - Honest full-tunnel proof (tested)

Deliverable: the dashboard only says **FULL TUNNEL ACTIVE** when the system
routing table, the tunnel's byte counters and the measured exit addresses
agree. The exit address of a VMess tunnel *is* the address of the server it
runs on - that is correct, and it is now proven instead of assumed.

### Done
- [x] **One implementation for both callers** (`src/diag.rs`): the CLI
      (`verify-full`) and the engine (GUI) use the same verdict, so the CLI
      can no longer report success with weaker checks than the dashboard.
- [x] **The exit address is no longer taken on faith.** The old code accepted
      `counters moved OR exit changed`; now every check has to pass:
      the split routes point into the TUN, the measured host is routed through
      the TUN, the adapter carried the bytes, and (in the system proxy mode)
      the ordinary and the explicitly proxied request leave through the same
      address.
- [x] **A leak is detected deterministically**: the route of the host that is
      used for the measurement is read from the routing table, so ordinary
      traffic cannot bypass the tunnel unnoticed.
- [x] **The exit address is explained**: the report names the exit address,
      the address before connecting and the server's address, and says which
      check failed.
- [x] **The active-connection counter works in TUN mode.** The operating
      system does not know the connections of a TUN tunnel (the core takes the
      packets off the adapter itself), so the count is taken from the core's
      own log (`accepted` / `connection ends`).
- [x] **A leftover route no longer aborts the setup**: a route to the server
      that another program had installed is kept (and no longer deleted on
      disconnect - the app only removes what it installed itself).
- [x] The mojibake in the sources (em dashes that a PowerShell round trip
      mangled) is repaired.

### Evidence
- `sni-tunnel verify-full` with the tunnel up: **FULL TUNNEL VERIFIED** - the
  ordinary request exits via the server's address, the TUN routes are
  2 IPv4 + 2 IPv6, the measured host is routed through the TUN, the adapter
  carried the bytes, DNS resolves through the tunnel, IPv6 is blocked.
- With the routes removed the same run reports **FULL TUNNEL NOT VERIFIED**
  and names the failing check.
- `cargo test` -> **85 passed** (78 unit + 7 integration), `cargo clippy` clean.
- The Linux build still compiles (`cargo zigbuild --target
  x86_64-unknown-linux-musl --no-default-features`).

### Errors / caveats
- The public IP lookup is a third-party service; repeated calls in a short
  time are answered with a connection reset. The verification retries, but a
  burst of lookups can still make the IPv4 exit look unavailable.
- The server sometimes asks for a TLS renegotiation, which the Rust TLS
  stack refuses - the connection then fails with "unexpected EOF". This is a
  property of the server, not of the app.
- In TUN mode there is no local proxy, so the two exit addresses cannot be
  compared; the routing table and the adapter counters carry the proof there.

## Stage 13 - VMess multiplexing (MUX) (tested)

Deliverable: MUX can be switched on and off per server, with a configurable
concurrency and the optional XUDP (UDP over the multiplexed connection) that
the installed Xray-core understands.

### Done
- [x] **The core is asked what it understands** (`src/xray.rs`): a
      configuration with an *invalid* `xudpProxyUDP443` and one with a string
      in `xudpConcurrency` have to be rejected. A core that ignores the fields
      accepts both - so the probe tells the two apart without guessing from
      the version number.
- [x] **Only supported fields are written.** The `mux` object carries
      `enabled` and `concurrency`; `xudpConcurrency` and `xudpProxyUDP443` are
      added only when the core knows them and the user asked for XUDP.
- [x] **Per-server settings that persist** (`src/model.rs`, `src/store.rs`):
      MUX off by default, concurrency 8, XUDP off, 16 UDP streams, UDP/443
      policy `reject`. A profile that was stored before MUX existed loads with
      MUX off - MUX is never switched on behind the user's back.
- [x] **GUI** (`ui/app.slint`): a *Multiplexing (MUX)* section in the
      advanced settings with the toggle, the concurrency, the XUDP toggle
      (only shown when the core supports it), the number of UDP streams and
      the UDP/443 policy, plus a short explanation. The inputs are disabled
      while MUX is off.
- [x] **The configuration is validated before the core is started** - a
      broken configuration can no longer leave a half-open tunnel behind.
- [x] **Command line** for the headless build: `--mux`, `--no-mux`,
      `--mux-concurrency`, `--mux-xudp`, `--no-mux-xudp`,
      `--mux-xudp-concurrency`, `--mux-udp443`. They override the profile for
      that run; `sni-tunnel mux <ref> ...` changes the settings of a *stored*
      server and saves them.
- [x] **A helpful error** when XUDP is requested but the core does not know
      it, instead of a configuration that is silently changed.

### Evidence
- Six parallel requests through the tunnel, counted at the server:
  **MUX off -> 8 transport connections**, **MUX on -> 2**. The traffic really
  is multiplexed, and every request was answered (HTTP 200) with the
  server's address as the exit address.
- `sni-tunnel verify-full --mux --mux-xudp` -> **FULL TUNNEL VERIFIED**; the
  DNS of the tunnel resolves through it, IPv6 stays blocked.
- The generated configuration (`runtime-xray.json`):
  `"mux": {"enabled": true, "concurrency": 8, "xudpConcurrency": 16,
  "xudpProxyUDP443": "reject"}`.
- The probe: `Xray-core capabilities: XUDP supported` (Xray 26.3.27).
- The settings survive a restart: `sni-tunnel mux 1 --mux --mux-concurrency 8
  --mux-xudp` saves them, a *new* process reads them back (`MUX: on
  (concurrency 8)`, `XUDP: on (streams 16, UDP/443 reject)`).
- The Windows installer was rebuilt with the MUX code: the silent install
  exits with 0, the app and the bundled core are in place and the installed
  `sni-tunnel.exe selftest` reports **SELF-TEST PASSED**; the uninstaller
  removes everything again.
- `cargo test` -> **89 passed** (82 unit + 7 integration), `cargo clippy`
  clean, the Linux (musl) build compiles.

### Errors / caveats
- The XUDP fields need Xray 1.8.6 or newer. With an older core the probe
  reports "not supported" and the app leaves the fields out.
- MUX makes the server see one transport connection for many streams. Some
  providers count connections, so MUX can change the price of a profile.

## How to resume in a fresh session
1. Read `AGENTS.md` (§1 for the exact build env, §2 architecture).
2. `$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\tools\w64devkit\bin;$env:PATH"`
3. `cargo test` then `target\debug\sni-tunnel.exe selftest` to confirm the
   environment still works before changing anything.
4. Pick the next unchecked stage item; do ONE stage per task, then commit.

## 2026-10-21 connection behavior audit (current session)

- Read AGENTS.md; preserved pre-existing uncommitted changes.
- Audited parser, endpoint resolution, Xray generation, diagnostics, TUN route/DNS lifecycle and engine verification.
- Confirmed installed core 26.3.27; last runtime preserves ea.com; no initial live Xray process. No reference client exports available.
- Added effective runtime comparison and GUI diagnostics/export; removed speculative presets; fixed ordinary probe proxy inheritance, certificate trust bootstrapping, TUN HTTPS verification, core DNS omission, TCP/ALPN imports and dashed UUID redaction.
- See DIAGNOSIS.md for source-backed version findings, capture and controlled balance procedures, and explicit remaining routing/security limitations.
- Validation in progress: Windows tests/GUI compilation and Linux headless cross-build. First run found and fixed dashed UUID scrubber; regression tests being rerun.

### Audit validation update

- 89 unit tests + 7 workflow integration tests passed in the headless Windows build after export-scrubbing fixes.
- Installed-core generation integration test passed (proxy + TUN document with safe validation inbound).
- Windows GUI compilation and release build passed before the final pin-editor addition; final full-feature tests/release rebuilding now.
- Linux headless release cross-build passed; rebuilding the final source revision. Linux execution unavailable (no WSL/Linux runtime).
- Selected profile has legacy insecure=true and no explicit trusted pin; real forwarding is blocked until trust is provisioned. Strict TLS probe returned EOF. No route-changing live TUN test was attempted with unauthenticated trust.
- Reference exports still needed for actual client comparison. Remaining known limits: OS adapter DNS selection, firewall-backed kill switch, per-socket/DNS/IPv6 leak attribution, optional packet capture. No claim of gaming-quota eligibility.

### Final audit build/test result

- Final `cargo test`: 92 unit + 7 workflow integration tests passed, zero failures. Installed-core test is opt-in and passed in its separate explicit invocation.
- Final Windows GUI `cargo build --release`: PASS; executable --help smoke test: PASS.
- Final Linux headless `cargo zigbuild --release --no-default-features --target x86_64-unknown-linux-gnu`: PASS (cross-built, not executed).
- `cargo clippy --all-targets`: completed with warnings. `git diff --check`: PASS. Repository-wide format check still reports differences; no broad reformat performed.
- Deliverables: existing repository source changes, DIAGNOSIS.md, this progress log, and copied Windows/Linux release binaries plus report in this chat's outputs directory.
- Further work remains conditional on a trusted server certificate pin and actual NetMod/Nekoray exports. Firewall kill-switch enforcement and full leak attribution remain unresolved limitations, not claimed fixes.

## 2026-10-21 user-requested Allow insecure restoration

- User explicitly approved restoring the previous certificate handling after the audit gate broke the working profile.
- Restored Windows unverified certificate discovery + session-only certificate hash when Allow insecure is enabled. Explicit pins and strict mode are unchanged; no saved credentials/SNI modified.
- Confirmed from actual Xray log: certificate covers fr1.vpnjantit.com, not ea.com. This explains strict-mode failure.
- 90 unit + 7 workflow tests passed. Opt-in isolated live test passed: native strict probe failed with EOF; insecure discovery succeeded; authenticated VMess forwarding of HTTPS to ipify and Cloudflare succeeded with matching public IP. The initial TLS certificate acquisition is unauthenticated; this is not normal server-identity verification.
- Added tests/cert_compatibility.rs as an ignored opt-in regression/live test. It uses private local ports, a temporary config and managed cleanup; does not change routes, system proxy or shared runtime config.
- Windows release building; publish under a new compatibility executable filename because the previous executable is currently open.
- Final Windows release build and startup smoke test passed; published outputs/sni-tunnel-windows-compat.exe. Live forwarding was verified before delivery.


## 2026-10-21 missing Logs messages

- Replaced lossy GUI broadcast consumption with a revisioned, bounded (2,000 entries), scrubbed history. Startup messages remain available, mode changes refilter history, Clear resets retained entries, and UI stalls no longer silently discard broadcast messages.
- Recognize Xray bracketed severity case-insensitively, including `[Warning]`; preserve failed-line capture for startup diagnostics.
- Replaced the fixed positive auto-scroll offset with a real scroll view, bottom tracking, wrapped text and visible levels. Label UTC timestamps and explain that changing core verbosity requires reconnecting.
- NetMod is running Xray 1.6.1 from its bundled v2ray.exe. Its command line names `%LOCALAPPDATA%\NetMod\v2raycfg.json`, but that file is absent after startup. A bounded capture is awaiting a user reconnect; no actual comparison or gaming-quota fix is claimed.


### Captured NetMod comparison and logging validation

Successfully captured NetMod's transient runtime JSON on reconnect, with credentials removed before writing. The actual outbound matches SNI Tunnel's last-written runtime on server fr1.vpnjantit.com:10001, VMess auto security / alterId 0, TLS SNI ea.com, WebSocket Host ea.com and /vpnjantit. Credentials are concealed and their equality was not verified. Neither client explicitly sets ALPN or fingerprint. NetMod uses Xray 1.6.1; SNI Tunnel uses 26.3.27. Version-specific default TLS behavior remains a concrete difference requiring on-wire comparison, not a proven billing cause.

NetMod specifies mux concurrency 0, but Xray 1.6.1 normalizes this to 8 (https://github.com/XTLS/Xray-core/blob/v1.6.1/infra/conf/xray.go#L102-L120), matching our explicit 8. Do not treat that raw JSON difference as a behavioral mismatch. NetMod requests allowInsecure and allowInsecureCiphers; SNI Tunnel uses a session-discovered certificate pin for the user's opt-in insecure mode. NetMod uses SOCKS/HTTP inbounds plus a separate TAP/tun2socks process; SNI Tunnel's last configuration uses native TUN and Wi-Fi socket binding. No DNS section is present in either core configuration; this does not establish OS DNS equivalence. NetMod's empty routing rules do not establish an application bypass.

Logging validation: all 96 GUI-feature unit tests and 7 workflow tests passed, including regression coverage for retained startup logs, mode switches, redaction, Clear, bounded history, and Xray severities. Release build and --help smoke passed. New artifact: outputs/sni-tunnel-windows-logs.exe. GUI layout compiled; no interactive visual verification is claimed. No network profiles, routes or TLS transport settings were modified during this logging fix. Gaming-quota billing remains unresolved.


## 2026-10-21 connection reliability repair

User asked to fix repeated EOF/HTTP 500/timeout failures and requested overwriting the existing delivered executable, not creating additional build names.

Confirmed client fixes:
- Certificate discovery now makes up to three attempts, each with a five-second total deadline, with 300 ms backoff. Successful discovery stops retries; a failed attempt cannot set a certificate pin. Exhaustion honors the configured auto-reconnect preference in the engine.
- TUN verification now applies the caller's timeout to the entire retry operation. Previously six attempts could each consume the full per-attempt timeout.
- Background health checks every 30 seconds probe actual HTTPS traffic through the live transport. A failed check removes the healthy status; another check follows after five seconds. Two failed checks stop the session, preserve the configured TUN kill-switch behavior and schedule reconnect only when enabled. Disconnect aborts pending checks. This does not upgrade the route-based kill switch into firewall enforcement.
- Removed automatic TUN-to-system-proxy fallback, which silently reduced application coverage when device-wide setup failed.
- Core exit now honors the existing TUN kill-switch setting; full-tunnel state resets on teardown. The GUI recognizes FullForwarding as connected.

Isolated live comparison used private loopback HTTP proxies, temporary configs removed after each child stopped, and no system route/proxy/profile changes. Current TLS default: 6/6 with mux and 12/12 without mux HTTP requests passed, with 3 and 2 logged EOFs respectively. Native TLS experiment: 6/6 and 11/12 passed, with zero logged EOFs; one curl TLS failure remained. Bundled NetMod core/config: 5/6 passed, with 2 EOFs and one HTTP 500. This short, sequential sample does not establish a reliable advantage or billing effect. No TLS fingerprint or stored transport settings were changed. The server/path can fail under both versions; client repairs cannot guarantee removal of remote HTTP 500 responses. Gaming-quota attribution remains unverified.

Regression coverage includes transient certificate recovery, bounded permanent failure, entire-operation TUN timeout, health status invalidation/recovery, reconnect preference and cancellation on disconnect.


### Certificate probe follow-up

The first post-fix live test still failed: all three Schannel certificate discovery attempts ended with EOF. Inspection found that the probe did not offer ALPN while Xray's WebSocket dialer offers HTTP/1.1. Enabled native-tls ALPN support and made Windows certificate discovery offer HTTP/1.1 for WebSocket, or the profile's ALPN for other transports. The actual Xray transport settings remain unchanged. A separate six-handshake Schannel sample succeeded 1/3 without ALPN and 2/3 with HTTP/1.1; this small sample does not prove that ALPN alone cures the remote closures. Final live retest is recorded below.


Final validation after ALPN alignment: 102 GUI-feature unit tests passed; the seven workflow integration tests also passed during this repair. The opt-in live certificate/HTTPS forwarding test passed after the alignment: session certificate acquisition succeeded and ipify/Cloudflare both reported 51.91.248.209 through the explicit local proxy. The earlier failed live attempt remains documented above. Full device-wide TUN routing and sustained server stability were not retested; quota billing is not measured by these tests.

Release build and --help smoke test passed. Overwrote the existing outputs/sni-tunnel-windows-logs.exe; source/destination SHA-256 hashes match. No new build filename created.


## 2026-10-21 actual TLS comparison and NetMod compatibility experiment

Captured ClientHello messages from the installed binaries into an isolated loopback listener (not a packet capture of WE's network). Used the captured NetMod effective configuration and SNI Tunnel's last runtime configuration, changing only the dial destination to the local capture socket and disabling mux for isolated samples. Credentials were never transmitted because the listener stops after ClientHello. Two samples each showed:
- NetMod's installed Xray 1.6.1: 264-byte handshake; SNI ea.com, ALPN http/1.1, X25519 key share.
- Pinned Xray 26.3.27 with omitted fingerprint: 1740 / 1772-byte handshakes, shuffled Chrome-style extensions and a 1216-byte hybrid key share.
- Pinned Xray 26.3.27 native TLS experiment: 1508-byte handshake; still not equivalent to NetMod's old Go TLS.
These prove a transport mismatch, not the ISP's billing decision. Raw structured summaries are in outputs/tls-handshake-comparison.json.

Added explicit, reversible `netmod_compatibility` setting (off by default for other installations). For this VMess/WebSocket/TLS test, modern pinned Xray still owns TUN, while a managed loopback SOCKS connection forwards TCP/UDP to NetMod's locally installed Xray 1.6.1. No older binary is downloaded and the pinned package is not replaced. The installed backend version must match 1.6.1. The remote server IP is resolved before DNS redirection and is selected from the route exclusions; the remote socket binds the physical uplink address. Credentials, SNI, Host and path are preserved; saved server profiles are unchanged. This tests the actual known-working core instead of guessing a browser fingerprint.

Compatibility is limited to TUN + VMess/WebSocket/TLS. Unsupported transport/XUDP settings and explicit certificate pins fail rather than being silently discarded. Allow insecure is respected using the old core's native option only when already enabled. Both core processes are monitored and managed; temporary credential-bearing backend configs are removed on normal/error cleanup. A crash can leave a session config; diagnostics associate a backend only with an unambiguous matching loopback port, and label the runtime snapshot as potentially stale. Diagnostics show both the TUN frontend and the remote backend, and compare the latter against NetMod exports.

Validation: isolated modern frontend -> old backend HTTPS passed; contained TUN HTTPS plus UDP DNS passed; full-device TUN test passed with the same bound/ordinary exit (51.91.248.209). The full-route test cleaned up: no Xray/v2ray processes, /1 routes, or netmod-session JSON files remained. 106 unit tests passed, including preservation of independent transport fields, pin/unsupported-mode rejection, local-only bridge and diagnostics selection. Workflow test result and final build recorded below. None of these checks measures quota billing.

All seven workflow tests passed. Release build and startup smoke passed. Existing outputs/sni-tunnel-windows-logs.exe overwritten and hash-verified. Enabled netmod_compatibility for the current TUN configuration after saving a local backup; server/SNI profiles are semantically unchanged. User still needs to measure quota outcome.

## 2026-10-21 — NetMod TAP datapath experiment

User explicitly requested trying NetMod's forwarding and DNS after old-core + modern-TUN compatibility still consumed main quota. Replaced the compatibility frontend with installed badvpn-tun2socks and the existing TAP-Windows V9 adapter; retained native modern Xray for normal mode. Uses NetMod subnet/gateway, MTU, SOCKS5 UDP option and Google DNS, leaves physical DNS alone, journals/restores TAP address/DNS/MTU, refuses an active NetMod helper. IPv6 blocking remains an intentional difference. Runtime diagnostics reflect the real backend; source-bound verification and route/counter checks understand TAP. No new driver installed.

Passed contained HTTPS/UDP and full-device route/counter/ordinary-HTTPS tests, exit 51.91.248.209. Verified cleanup and DHCP/DNS restoration. User must measure quota; no billing outcome claimed. Deliver by overwriting outputs/sni-tunnel-windows-logs.exe only.
Final offline suite passed: 106 unit tests and 7 workflow tests. The two relevant ignored TAP live tests were explicitly run and passed separately. Release compilation started after these results.
Release build succeeded (1m42s). Overwrote the existing outputs/sni-tunnel-windows-logs.exe; source/destination SHA-256 matched and the delivered executable --help smoke test exited 0. No additional executable name was created. Saved TUN mode and compatibility flag remain enabled.

## TAP orphan-process fix (2026-10-21)
Confirmed old SNI Tunnel PID 23960 had exited but left its compatibility core and tun2socks alive. The core command line referenced our private netmod-session file and both children shared the dead parent; no NetMod application was running. Removed only those verified orphan children and restored the saved TAP addressing/DNS. Windows children now join a non-inherited kill-on-close Job Object, so forced application exit also closes its core/helper processes. Failure to assign supervision fails startup instead of leaving an unmanaged child. The conflict message no longer assumes a foreign tun2socks is NetMod. Crash route recovery now selects the correct IPv4/IPv6 family. Regression test checks job closure kills an assigned child and preserves an unrelated process.
Validation: release build succeeded; 107 unit tests and 7 workflow tests passed. Replaced outputs/sni-tunnel-windows-logs.exe in place (SHA256 96F16CA53EAB300AA2BB7C8CF1C55025BF2DD27A98016CB9C669E8895F0266C3). User launched the new build at 19:36:46 and its live log reached FULL TUNNEL ACTIVE at 19:37:13, exit 51.91.248.209. A separate contained integration test was blocked by that newly active helper; its startup cleanup disturbed the active routes. User was informed to disconnect/reconnect; do not claim this isolated test passed. Follow-up: ensure integration tests and stale route cleanup respect a live renamed GUI instance before any cleanup.

## Linux/source and bundled Windows installer delivered (2026-10-21)
Built Linux amd64 DEB/RPM/tar.gz with a Python/Tk desktop launcher, native Linux TUN/DNS recovery, official Xray 1.6.1 and static BadVPN (commit 07268f0). Rust backend requires glibc >=2.30. Verified ELF architecture, tar/DEB permissions and metadata, RPM payload, shell syntax, Python syntax and 3 recovery unit tests. Linux runtime/kernel/desktop/ISP accounting remain untested on this Windows host. Source ZIP contains complete application sources, Linux build/package scripts, official Xray 1.6.1 source/release archives and BadVPN source, excludes private configuration/logs and build target directories.
Windows NSIS installer bundles current fixed app, official Xray26.3.27/Wintun, user-tested NetMod compatibility core/helper + VC runtime and signed TAP V9 driver. Installs TAP only if no existing TAP V9 adapter exists; shared driver/user profiles retained on uninstall. No NetMod install required at runtime. Installer compilation and extracted payload are verified; did not reinstall driver over the active connection.

## Linux desktop layout 0.4.1 (2026-10-21)
Replaced the single-form Tk launcher with the Windows layout: header/status badge, six tabs (Dashboard, Servers, SNI, Logs, Settings, Diagnostics), four dashboard cards, Connect/Disconnect/Test/Verify/Refresh IP controls and matching palette. Retained the working Linux helper transport. Added server selection/import/duplicate/export/delete, SNI profile persistence, legacy profile migration, log copy/export/clear with redaction, diagnostics and kernel TUN byte/speed counters. Active connections remain unavailable (dash); window manager minimize replaces unsupported tray integration. Added an explicit verify command over helper stdin so full-tunnel checks do not disconnect it. Verified UI construction and all six tab switches, legacy migration, profile persistence/duplication, SNI selection, log secret redaction and simulated connection state changes on host Tk. Three DNS/route recovery tests pass. Linux live desktop and networking are not executed on this Windows host.

Published v0.4.1 to https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/tag/v0.4.1 and marked Latest. Seven assets present. GitHub digests match local DEB/RPM/tar/source ZIP. A failed zero-byte tar upload placeholder was cleared and the remaining uploads retried before publishing. The prior v0.4.0 release retains its original packages.

## 2.1 health monitoring work (in progress)
- Audited Linux helper, engine periodic checks and full-tunnel diagnostics.
- Confirmed gaps: no explicit proxy comparison in TUN mode; Linux recovery removes protection; Windows route-only kill switch requires further audit.
- Release gate: no publication until implementation and required tests/builds pass.
- Keep pinned cores and existing profile data unchanged.

### Implemented checkpoint (2.1.0 candidate)
- Independent bounded proxy/system traffic checks, fresh DNS verification, session baseline and 60-second defaults.
- Persistent health preferences; cancellable Rust checks; Linux netlink and Windows native network events.
- Linux nftables output guard with real isolated packet tests; Windows WFP session guard cross-compiles, native CI test pending.
- Dashboard, notifications/history, emergency disconnect, bounded reconnect jitter.
- No release published yet; required CI/package gates remain.
