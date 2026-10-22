# V2 context

This repository now contains the native Slint GUI for Linux and Windows. V2 uses
separate application data/install identifiers. README.md, MIGRATION.md and
TEST_RESULTS.md describe the current behavior. The inherited notes below remain
useful for Windows GNU builds; their statements that Linux is headless and lacks
TUN/system-proxy support describe the older baseline. Pinned vendor archives are
restored by packaging/linux/fetch-vendor.py. Never publish private/, runtime user
stores, keys, or real-profile diagnostics.

# AGENTS.md — SNI Tunnel

Windows desktop client: VMess via Xray-core with **independent custom TLS SNI**.
Rust + Slint GUI, system tray, per-user autostart. No C crypto implemented by hand.

## 1. Build environment (READ THIS FIRST — non-obvious)

The machine has **no MSVC and no C toolchain by default**. We use the Rust
**GNU** toolchain plus a standalone binutils/GCC kit.

* Rust: `stable-x86_64-pc-windows-gnu` (rustup, `--profile minimal`), installed at
  `%USERPROFILE%\.rustup`. `cargo`/`rustc` live in `%USERPROFILE%\.cargo\bin`.
* binutils/GCC: **w64devkit 2.10.1** extracted to `C:\tools\w64devkit`
  (provides `dlltool.exe`, needed by rustc's `raw-dylib` path, and `gcc` for C crates).
* Linker: `.cargo/config.toml` forces `linker = "rust-lld"` +
  `-Clink-self-contained=yes` so we do **not** link through w64devkit's gcc
  (whose `libgcc_eh.a` layout is incompatible).
* Some mingw import libs (`libshlwapi.a`, …) were **copied into** the toolchain
  self-contained dir:
  `%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\lib\self-contained`
  from `C:\tools\w64devkit\lib`. If a fresh checkout fails with
  `unable to find library -lxxx`, copy the missing `libxxx.a` the same way.
* **Release profile must NOT use fat LTO here.** `lto = true` (+
  `codegen-units = 1`) makes `rustc` die with `STATUS_STACK_BUFFER_OVERRUN`
  (0xc0000409) while compiling Slint deps (usvg/rowan/moxcms). The release
  profile uses `lto = "thin"`, `codegen-units = 16` (see `Cargo.toml`); a full
  release build takes ~14 min.
* **`RUST_MIN_STACK=8388608` is required for the release build.** With the
  default 2 MB stack `rustc` dies with `STATUS_STACK_BUFFER_OVERRUN` (0xc0000409)
  on some crates (`syn`, `skrifa`, …). `scripts/build-dist.ps1` sets it.
* **A crashed compiler leaves a corrupted target directory.** The symptoms are
  `only metadata stub found for 'rlib' dependency 'core'` and
  `found invalid metadata files for crate ...`. `cargo clean` (or
  `cargo clean -p sni-tunnel`) fixes it; the release profile may then need a
  full rebuild.
* **The crate is a library plus a thin binary** (`src/lib.rs` + `src/main.rs`)
  so that `tests/` can exercise the behaviour. `slint::include_modules!()` and
  the Slint component types live in the library.

### Build / test commands (PowerShell)

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\tools\w64devkit\bin;$env:PATH"
$env:RUST_MIN_STACK = "8388608"   # only needed for the release build
cargo build            # debug
cargo build --release
cargo test             # 71 tests (64 unit + 7 integration)
cargo clippy --all-targets
cargo fmt --all -- --check
```

### Run

```powershell
target\debug\sni-tunnel.exe selftest
target\debug\sni-tunnel.exe check   --vmess "vmess://..." --sni cdn.example.com
target\debug\sni-tunnel.exe proxy   --vmess "vmess://..."     # sets system proxy, Ctrl+C restores
target\debug\sni-tunnel.exe proxy   --vmess "vmess://..." --mode tun   # whole-device TUN (Administrator)
target\debug\sni-tunnel.exe tun-selftest           # TUN datapath self-test (Administrator)
target\debug\sni-tunnel.exe tun-selftest --full    # same, but with the full /1 routing table
target\debug\sni-tunnel.exe proxy-off                          # crash recovery
target\debug\sni-tunnel.exe                           # (or `gui`) opens the tray app
target\debug\sni-tunnel.exe autostart on|off|status   # start with Windows
```

Runtime data dir: `%LOCALAPPDATA%\SniTunnel\`
(`bin\xray.exe`, `bin\wintun.dll`, `runtime-xray.json`, `config.json`, `sni-tunnel.log`).

## 2. Architecture

| Module | Responsibility |
|---|---|
| `src/main.rs` | CLI entry: dispatch, `parse_args`/`Opts`, `selftest`/`check`/`connect`/`proxy`/`proxy-off`; falls back to the stored selection when `--vmess` is omitted |
| `src/storecli.rs` | Profile CLI: `list`/`add`/`remove`/`use`/`export` + `sni list|add|use|off|remove` |
| `src/editor.rs` | Server editor logic (pure): profile ⇄ editor fields, validation, header parsing, duplication |
| `src/clipboard.rs` | Clipboard read/write (Win32 on Windows, `wl-paste`/`xclip` on Linux) |
| `src/gui.rs` + `ui/app.slint` | Slint desktop UI; engine on a background Tokio runtime, polled from a UI `Timer` |
| `src/tray.rs` | Stage-5 system tray (own thread + message pump); events drained via `tray::poll` |
| `src/autostart.rs` | Stage-5 "start with Windows" via the per-user `Run` key |
| `src/console.rs` | Hides the console on GUI startup when we own it (double-click/autostart) |
| `src/model.rs` | Data model: `ServerProfile`, `SniProfile`, `Settings`, `Endpoint`, `resolve_endpoint()` |
| `src/vmess.rs` | Import: `vmess://`, base64 subscriptions, JSON (no network) + **export** to `vmess://` |
| `src/xray.rs` | Xray config generation (all transports) + `XrayProcess` subprocess lifecycle |
| `src/corebin.rs` | Pinned download + SHA-256 verify + unzip of core binaries |
| `src/session.rs` | Headless tunnel session (core + local proxy + wait-for-port) |
| `src/tun.rs` | Stage-6 TUN mode: wintun adapter config, routing, uplink discovery, TUN verification |
| `src/engine.rs` | Actor: phases, system proxy **or** TUN, stats, core-death, auto-reconnect |
| `src/sysproxy.rs` | WinINet system proxy capture/apply/restore + crash-safe backup |
| `src/localproxy.rs` | Local **HTTP + SOCKS5** listeners, byte-counting TCP relay |
| `src/netcheck.rs` | Real DNS / TCP / TLS / proxied-HTTP diagnostics + the staged `diagnose()` ladder |
| `src/stats.rs` | Atomic traffic counters (real, measured at the relay) |
| `src/logging.rs` | Log bus (UI + file) with secret scrubbing; bridges the `log` crate |
| `src/store.rs` + `src/dpapi.rs` | Config persistence; UUIDs sealed with Windows DPAPI |
| `src/util.rs` | Paths, free port, formatting, secret scrubbing |
| `src/lib.rs` | The library: exposes every module for the integration tests |
| `assets/` | Application icon: `icon.png` (master), `icon-256.png` (window icon), `icon-32.rgba` (tray icon), `icon.ico` (executable resource) |
| `scripts/make-icon.ps1` | Regenerates every icon asset from the master PNG |

### Application icon

The artwork lives in `assets/` and is used in three places:

* **Window icon** — `ui/app.slint` sets `icon: @image-url("../assets/icon-256.png")`.
* **Tray icon** — `src/tray.rs` embeds `assets/icon-32.rgba` (raw 32×32 RGBA, so
  no image decoder is needed at runtime).
* **Executable icon** — `build.rs` runs GNU `windres` (w64devkit) on
  `assets/app.rc` and links the resulting `.rsrc` section into the binary.
  If `windres` is missing the build still succeeds, only without the icon.

To change the icon, replace the master image and run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\make-icon.ps1 -Source "C:\path\to\icon.png"
```

> The `.ico` must use **classic DIB entries**, not PNG-compressed ones —
> GNU `windres` cannot read PNG-compressed icons. `make-icon.ps1` writes the
> DIB format for that reason.

### Platform split

Windows has the native implementations; every other platform uses a portable
one (see `src/main.rs` for the `use … as …` aliases):

| Module | Windows | Other platforms |
|---|---|---|
| `dpapi` | DPAPI (`CryptProtectData`) | `dpapi_linux.rs`: SHA-256 keystream obfuscation with a `0600` key file (obfuscation, not encryption) |
| `sysproxy` | WinINet registry | `sysproxy_linux.rs`: no-op that reports "not managed on this platform" |
| `autostart` | `HKCU\…\Run` | `autostart_linux.rs`: `~/.config/autostart/sni-tunnel.desktop` |
| `tray` | `tray-icon` | `tray_linux.rs`: no tray (the window still works) |
| `elevate` | `ShellExecuteW("runas")` | `elevate_linux.rs`: `pkexec` |
| `clipboard` | Win32 clipboard | `wl-paste` / `xclip` / `xsel` |
| `tun` | netsh/route + wintun | not available (Administrator check fails cleanly) |

### Data flow (why it is built this way)

```
system-proxy mode:
app ──▶ local Rust proxy (HTTP/SOCKS5, counts bytes)
          └─▶ Xray SOCKS inbound (127.0.0.1, private port)
                └─▶ VMess outbound + TLS/WS with custom SNI ──▶ server

TUN mode (stage 6):
app ──▶ wintun adapter (created by Xray itself)
          └─▶ Xray tun inbound ──▶ VMess outbound + TLS/WS with custom SNI ──▶ server
```

The local proxy is **ours** so traffic statistics are *measured*, not estimated,
and so local-proxy errors are first-class diagnostics.

## 3. Key technical decisions

* **Xray-core pinned**: v26.3.27, `Xray-windows-64.zip`,
  sha256 `d004c39288ce9ada487c6f398c7c545f7d749e44bdfdd59dbc9f865afba4e1ad`.
  The zip also contains `wintun.dll`, the driver used by TUN mode.
* **TUN mode uses Xray's own `tun` inbound** (`"protocol": "tun"`), *not*
  tun2socks. Xray creates the wintun adapter, so we ship one binary and get the
  userspace network stack for free. (tun2socks v2.7.0 was evaluated and
  dropped: its Windows driver is registered as `tun://`, it needs `--long
  flags`, and it cannot avoid the routing loop by itself.)
* **Loop avoidance is mandatory in TUN mode**: the outbound is pinned to the
  physical uplink with `streamSettings.sockopt.interface` (Windows
  `IP_UNICAST_IF`), so Xray's own traffic never re-enters the tunnel. The
  uplink is discovered from the default route (`route print -4` +
  `netsh interface ipv4 show addresses`).
* **Routing**: two `/1` routes (`0.0.0.0/1`, `128.0.0.0/1`) shadow the default
  route without deleting it, so a crash leaves the machine connected. The VPN
  server is excluded with a `/32` route via the physical gateway. `route.exe add`
  picks the *wrong* interface for a wintun next hop — routes are therefore
  installed with `netsh interface ipv4 add route ... interface=<name>`.
* **wintun adapter setup**: `netsh interface ipv4 set address` needs the keyed
  form `source=static addr=... mask=...` (the positional form silently no-ops),
  and DAD must be disabled (`set interface ... dadtransmits=0`) or the address
  stays `Tentative` for ~3 s and cannot be bound to.
* **TLS via `native-tls`/Schannel** (not rustls/ring) → no C crypto dependency,
  uses the Windows cert store. `tokio-native-tls` for async handshakes.
* **Address / SNI / Host are independent** (`model::resolve_endpoint`). An SNI
  profile never changes VMess credentials, and the **server editor keeps them
  as separate fields** (`src/editor.rs`): the TLS SNI, the WebSocket Host, the
  WebSocket path and the dial address can each be changed without touching
  the others.
* **The SNI of an imported server is detected, not guessed**: `vmess::parse_vmess_link`
  reads the `sni` field of the link and the editor shows it. Nothing is
  hard-coded per server.
* **No fake status**: "connected" is only reported after a *real* proxied HTTP
  request succeeds through the tunnel. When it fails, the engine runs the
  staged diagnostic ladder (`netcheck::diagnose`: DNS → TCP → TLS → VMess
  forwarding) and reports the first failing stage instead of a generic
  message.
* **TLS verification stays on.** A certificate that does not cover the
  configured SNI is reported with an actionable hint (use the SNI the
  certificate was issued for, or explicitly enable "Allow insecure").
* **Xray 26 removed `allowInsecure`.** Never emit it — the core refuses to
  start (exit code 23). The supported equivalent is `pinnedPeerCertSha256`
  (one specific certificate); `netcheck::ensure_cert_pin()` resolves the pin
  from the server when a profile asks for an unverified certificate. The same
  applies to the WebSocket `Host` header: it now lives in the independent
  `host` field, not in `headers`.
* **Exit code 23 means "the core refused to start"** — never assume it is an
  adapter problem. `xray::validate_config()` runs `xray run -test` before the
  network is touched, and `XrayProcess::failure()` classifies the core's own
  output.
* **TUN mode stages**: configuration → core → adapter → addressing → DNS →
  routing → forwarding. Each stage reports into the log and the status line
  (`tun::TunStage`), and each failure has a specific reason
  (`tun::TunFailure`).
* **Routing journal**: the routes we install are written to
  `tun-routes.json` and removed again on the next start, so a crash cannot
  leave the machine without connectivity.
* **Secrets (UUIDs) are scrubbed from all log output and DPAPI-sealed on disk.

## 4. Stage plan

1. **DONE** — headless VMess + custom SNI + real verification.
2. **DONE** — profile store + DPAPI persistence exposed via CLI (`storecli`).
3. **DONE** — engine + Windows system proxy set/restore + crash recovery.
4. **DONE** — Slint GUI over the engine (`sni-tunnel gui`).
5. **DONE** — tray + minimize-to-tray, start-with-Windows, release packaging
   (`scripts/build-dist.ps1`, `README.md`). TUN split out into stage 6.
6. **DONE** — TUN mode (Xray native `tun` inbound + wintun), admin-gated.
   Verified live: `tun-selftest` and `tun-selftest --full` both pass.

## 5. Known limitations

* TUN mode is **Windows only** (the routing uses `netsh`/`route.exe`); the
  Linux release is a headless client that provides the local HTTP/SOCKS5
  proxy.
* TUN mode is blocked by **Cloudflare WARP** (and similar WFP-based VPN
  clients): their filter driver rejects connections from foreign TUN adapters
  with `WSAEACCES (10013)`. The app now detects such adapters and warns
  (`other VPN clients are active (...)`); stop them before using TUN mode.
* **A failed TUN tunnel tears itself down.** The TUN captures *all* traffic,
  so a tunnel that cannot carry traffic would leave the machine with no
  internet. The engine therefore removes the TUN routes as soon as
  verification fails - unless the **kill switch** is enabled (the default), in
  which case the routes are kept so that traffic is *blocked* instead of
  escaping. A crash is repaired on the next start (route journal).
* **IPv6 is blocked, not tunnelled**: the `::/1` routes are installed, so
  IPv6 cannot leak, but the tunnel does not carry IPv6 traffic yet.
* The **GUI** relaunches itself elevated (`src/elevate.rs`) when a TUN
  connection is requested without Administrator rights; the **CLI** still
  requires an elevated shell.
* Local proxy supports SOCKS5 CONNECT and HTTP (CONNECT + absolute-URI) only;
  no UDP relay (so no UDP-over-tunnel in system proxy mode).
* Downloads require network access on first run (`%LOCALAPPDATA%\SniTunnel\bin`).
* The full crate still cannot be compiled for Linux *with the GUI* in this
  environment (Slint needs `libfontconfig1-dev`); the headless Linux CLI is
  cross-compiled with `cargo-zigbuild` and the platform modules are
  compile-checked for `x86_64-unknown-linux-gnu`.

### Testing TUN mode on this machine (read before running it)

* The disruptive test is `tun-selftest --full` (it installs two `/1` routes,
  i.e. the whole device is routed into the TUN). The contained variant
  `tun-selftest` only routes `1.1.1.1/32` and is safe to run.
* `scripts/smoke-test.ps1` runs the whole ladder (build, tests, CLI, TUN, full
  tunnel verification, cleanup) and reports PASS/FAIL/BLOCKED per step.
* `scripts/measure.ps1` reports the idle RAM/CPU and the startup time.
