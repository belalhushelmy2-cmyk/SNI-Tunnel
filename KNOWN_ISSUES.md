# KNOWN_ISSUES — SNI Tunnel

Ordered by severity. Every entry states whether it is a real defect, a
limitation of the design, or an environment problem.

## Critical / user-visible

1. **Linux has no TUN mode.** The TUN code paths use `netsh`/`route.exe` and
   are Windows specific; on Linux the app reports "TUN mode is currently
   implemented on Windows only" and the system proxy is a no-op as well, so the
   Linux release is a **local HTTP/SOCKS5 proxy** (`sni-tunnel connect` →
   `127.0.0.1:2080` / `:1080`). A Linux implementation of the routing
   (TUN device + `ip route`) is the next milestone; it could not be verified
   here because there is no Linux host.

2. **A crashed run can leave the machine without a working TUN adapter.**
   A wintun adapter that is still registered from a crashed process makes the
   next Xray start fail with *"initialization has already been completed"*; the
   adapter cannot be removed reliably (WMI/`netcfg` refuse). *Mitigation
   implemented*: the app clears the stale address on every start, takes a lock
   file and falls back to a fresh adapter name (`sni-tunnel-1`, `-2`, …).
   *Residual*: a crashed run can leave an unused adapter behind (it is removed
   by the next clean start of the core, otherwise it stays as a harmless "Up"
   device).

3. **Cloudflare WARP (and similar WFP-based VPNs) block TUN traffic.**
   Their filter driver rejects connections from a foreign TUN adapter with
   `WSAEACCES (10013)`. The app detects the conflicting adapters and warns
   before starting.

4. **IPv6 does not work end-to-end through the tunnel on this machine.**
   The IPv6 `/1` routes are installed, so IPv6 is *blocked* rather than leaked
   (verified: "Network unreachable"), but the tunnel itself does not carry
   IPv6 traffic yet. *Impact*: IPv6-only sites and some QUIC traffic do not
   work while the tunnel is up.

5. **A full tunnel with a dead server blocks the machine** (by design, the kill
   switch). The kill switch can be switched off in the settings. After a crash
   the routes are removed on the next start, so the machine comes back online.

## Limitations

6. **Xray's own `tun` inbound does not configure the system on this
   machine.** `gateway`, `autoSystemRoutingTable` and `autoOutboundsInterface`
   silently do nothing (the adapter keeps an APIPA address, no routes are
   added), so the app does the addressing and the routing itself with `netsh`
   and pins the outbound to the physical interface with `sockopt.interface`.

7. **The system proxy is Windows only** (WinINet). On Linux the desktop's
   proxy settings are not touched.

8. **TUN mode needs Administrator.** The GUI asks for elevation when a TUN
   connection is requested and restarts elevated; the CLI must be run from an
   elevated shell.

9. **The UUID is only encrypted on Windows.** On other platforms it is
   obfuscated with a SHA-256 keystream and a `0600` key file.

10. **The GUI takes ~2 s to show its window** (Slint + the software renderer).
    The headless CLI is ready in <100 ms.

## Cosmetic / low

11. The tray tooltip is static (the `tray-icon` crate does not allow updating
    it from the UI thread).
12. "Minimize to tray" is implemented as close-to-tray (Slint exposes no
    per-window minimize event).
13. The release is a single executable; no installer yet.
14. The GUI uses the software renderer, so it needs ~89 MB of RAM.
