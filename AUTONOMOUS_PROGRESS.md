# AUTONOMOUS_PROGRESS — SNI Tunnel

Windows + Linux VMess/VLESS/Trojan client built on Xray-core (Rust + Slint).
This file is the running log of the autonomous session.

## Where we are

| Area | State |
|---|---|
| Headless VMess + custom TLS SNI | **done, tested** (stage 1) |
| Profile store, DPAPI secrets, CLI | **done** (stage 2) |
| Engine + Windows system proxy + crash recovery | **done** (stage 3) |
| Slint GUI (servers, SNI, logs, settings) | **done** (stage 4) |
| Tray, minimize-to-tray, autostart, packaging | **done** (stage 5) |
| TUN mode (wintun, native Xray `tun` inbound) | **done** (stage 6) |
| Server manager + configuration editor + custom SNI | **done** (stage 7) |
| Application icon | **done** (stage 8) |
| Xray 26 `allowInsecure` removal (the "exit code 23" bug) | **done** (stage 9) |
| Real full tunnel (IPv4+IPv6), kill switch, real counters, verification | **done** (stage 10) |
| VLESS + Trojan, Linux x86_64 release | **done** (stage 11) |
| Quality gates (clippy 0, fmt clean, 71 tests) | **done** |
| Library split + integration tests + smoke test | **done** |
| Linux TUN mode (routing on Linux) | **todo** — needs a Linux host to verify |
| AppImage / .deb | **prepared, not built** (no Linux host) |

## Milestones completed in this session

### Stage 9 — the "TUN fails with exit code 23" bug (root cause fixed)
* **Root cause**: Xray 26 removed the TLS field `allowInsecure`; the config
  generator emitted it unconditionally, so *every* TLS profile made the core
  refuse to start (exit 23) — and the app blamed the TUN adapter.
* **Fix**: `allowInsecure` is never emitted. A profile that wants an
  unverified certificate gets the modern equivalent: the server's leaf
  certificate is **pinned** (`pinnedPeerCertSha256`) — one exact certificate,
  verification stays on.
* Also: config validated with `xray run -test` before touching the network,
  the core's own output captured and classified, staged error reporting,
  wintun.dll architecture check, fallback to system-proxy mode, route journal
  for crash recovery, WebSocket `Host` moved to the new independent field.

### Stage 10 — real full tunnel
* **IPv6 is now tunnelled** (adapter IPv6 address + two `/1` IPv6 routes), so
  IPv6 cannot leak around the tunnel.
* **Kill switch** (default on): when the tunnel dies the routes are kept, so
  traffic is *blocked* instead of escaping; a clean disconnect removes them and
  a crash is repaired on the next start.
* **Real counters** in TUN mode from the wintun adapter (`GetIfEntry2` via the
  interface LUID) — the dashboard no longer shows 0 B/s.
* **`verify-full`** command + GUI button: public IP before/after, installed
  routes, an *ordinary* (unproxied) request, adapter counters, DNS, IPv4/IPv6.
  The exit IP must change, otherwise the tunnel is not confirmed.
* **Poisoned adapter recovery**: a wintun adapter left behind by a crash makes
  the next Xray start fail ("initialization has already been completed"). The
  app now clears the stale address on every start and falls back to a fresh
  adapter name (`sni-tunnel-1`, `-2`, …).

### Stage 11 — protocols and the Linux release
* **VLESS and Trojan** are now first class: the protocol is a profile field,
  the editor has a dropdown, the config generator builds the right outbound
  (`vless` → `vnext`/`encryption: none`, `trojan` → `servers`/`password`), and
  `vless://` / `trojan://` links are imported and exported.
* **Linux x86_64 release**: cross-compiled with `cargo-zigbuild` (zig as the C
  compiler and linker) into a 4.3 MB headless CLI. The GUI needs a Linux
  sysroot (fontconfig) and is therefore not cross-built.
* The build script now only links the Windows icon resource for Windows
  *targets* (it used to check the host), and `rustls` replaces `native-tls`
  outside Windows so no OpenSSL is needed.

### Quality, tests and packaging
* `cargo clippy --all-targets`: **0 warnings**; `cargo fmt --check`: clean.
* **71 tests** (64 unit + 7 integration in `tests/workflow.rs`).
* The crate is now a **library + a thin binary** so the behaviour can be
  tested with integration tests.
* `scripts/smoke-test.ps1`: a live end-to-end test that reports
  PASS/FAIL/BLOCKED per step — **10/10 PASS** against the real server.
* `scripts/measure.ps1`: resource measurements.
* `scripts/build-dist.ps1 -Linux`: builds and packages both releases.
* `packaging/debian/*` and `packaging/linux/build.sh`: the Debian package and
  the AppDir (buildable on a Linux host).

## Two real bugs the new tooling found

1. **Old configuration files stopped working.** A profile written before the
   `protocol` field existed deserialized with an *empty* protocol, so every
   connection failed with "Unsupported protocol". The store now has proper
   defaults (`vmess` / `auto` / `tcp`) and a regression test.
2. **A crashed run could block the next start.** A leftover wintun adapter
   holds the device; the app now takes a lock file, reports "another instance
   already owns the TUN adapter" and terminates stale core processes.
3. **The TUN lock was not released when the setup failed** (only on a clean
   stop), so the *next* start was refused. The lock is now released on every
   error path.
4. **The public IP lookup could time out right after the tunnel came up** (it
   races with the route installation); it is now retried three times, which
   made the verification stable (3/3 runs).

## Verified live on this machine (the user's own server)

`sp3.vpnjantit.com:10001`, VMess+WS+TLS, **SNI `ea.com`**, WS host `ea.com`,
path `/vpnjantit`.

```
[1] public IP before connecting : 51.91.248.209
[Xray configuration] ok … [routing configuration] ok
[3] system routes via the TUN   : 8 IPv4, 2 IPv6
[4] ordinary request exit IP   : 185.121.15.92   (the VMess server)
    the exit IP changed -> the traffic used the tunnel
[5] TUN adapter counters moved  : true (29967 / 21845 bytes)
[6] system DNS resolution      : ok
[7] IPv4 exit                   : 185.121.15.92
[7] IPv6 exit                   : blocked by the tunnel (no leak)
FULL TUNNEL VERIFIED
```

## Next up (see NEXT_STEPS.md)

1. Linux TUN mode (routing on Linux) — needs a Linux host to verify.
2. AppImage / Debian build on a Linux host.
3. Automatic reconnection over a long outage, network-change handling.
4. A Windows installer (MSI/Inno Setup).
