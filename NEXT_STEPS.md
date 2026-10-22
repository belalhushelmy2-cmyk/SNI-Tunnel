# NEXT_STEPS — SNI Tunnel

Exact, ordered steps. Each item is small enough to finish and verify in one
sitting.

## 1. Linux TUN mode — HIGH
* The TUN code is Windows specific (`netsh`, `route.exe`). For Linux the same
  recipe with the `iproute2` tools:
  * `ip link set dev <name> up`, `ip addr add 198.18.0.1/24 dev <name>`,
    `ip -6 addr add fc00::1/64 dev <name>`;
  * `ip route add 0.0.0.0/1 dev <name> metric 1` and `128.0.0.0/1` (plus the
    IPv6 equivalents) instead of a default route, so the original default
    route survives;
  * the server keeps a `/32` route via the physical gateway
    (`ip route show default` provides it);
  * DNS: `resolvectl dns <iface> 1.1.1.1` when systemd-resolved is present,
    otherwise leave the resolver alone and report it;
  * cleanup: delete the routes, the addresses and the lock file.
* The uplink discovery, the route planning and the cleanup must be shared
  between the platforms (only the command construction differs).
* **Needs a Linux host to verify** — the current environment has none.

## 2. Network-change handling — MEDIUM
* When the Wi-Fi interface reconnects, the interface index and the default
  gateway change: the TUN routes point at the old gateway and the traffic
  stops. Detect the change (poll the default route every few seconds or listen
  for the interface index to change) and re-apply the routes.

## 3. Automatic reconnection, verified — MEDIUM
* The engine already schedules a reconnect (5 attempts, 3 s apart); it was
  never exercised over a real outage. Test: kill the core while connected and
  check that the engine reconnects, re-verifies the tunnel and updates the
  status.

## 4. Packaging — MEDIUM
* The Windows installer is done: `packaging/windows/build.ps1` builds
  `dist/SNI-Tunnel-Setup-Windows-x64.exe` (Inno Setup) plus a portable zip
  and the checksums.
* The Debian package is written (`packaging/linux/build.sh`) but was never
  built here: the GUI needs the fontconfig headers, which only exist on a
  Linux host. The CI workflow (`.github/workflows/release.yml`) builds it on
  Ubuntu - check the first run and fix what it reports.
* The AppImage is still missing.

## 5. GUI polish — LOW
* The status line should show the individual stages (core running, TUN
  initialised, routes installed, DNS protected, forwarding verified) instead
  of one combined message.
* A "System proxy" hint for the applications that ignore the proxy settings.

## 6. Performance — LOW
* The GUI's ~2 s startup: the software renderer initialises the font
  database. Measure whether the "femtovg" renderer can be initialised in the
  background while the window is shown.
* Idle RAM of 89 MB: check how much of it is the renderer.
