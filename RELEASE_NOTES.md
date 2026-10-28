# SNI Tunnel 2.1.6 — per-app traffic usage

Linux desktop now includes **App usage → Start monitoring**:

- Visible applications ranked by measured TCP traffic.
- Download/upload totals, live rates and open TCP connections.
- Fixed-width numeric columns, a scrolling list, Pause and Reset.
- Session-only history: no usage records are saved or transmitted.

The reader samples device TCP counters once a second. **Direct and tunneled TCP
are included; this is not an exact breakdown of VPN totals.** UDP/QUIC, inaccessible
processes and connections entirely between samples are omitted. Local IPC and
Xray/tunnel relay processes are excluded to avoid counting their copies again.
Requires Linux iproute2 (`iproute` on Fedora), already declared by the packages.
No administrator access is requested for monitoring.

Desktop Xray, routing, DNS, profiles and connection behavior are unchanged.
Windows packages receive the updated desktop UI with a clear Linux-only state
for App usage. Android phone/TV 1.0.6 APKs are unchanged, signed originals carried
forward with verified checksums. Per-app accounting is not available on those
platforms. macOS and FreeBSD previews remain available in the
[previous release](https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/tag/v2.1.5).

Linux downloads include the portable archive, AppImage, Debian package, Fedora
RPM, Arch package, Solus eopkg, AerynOS stone/index and offline installers. The portable archive is used by the
existing terminal installer/updater. User configuration is preserved.

See TEST_RESULTS.md for validation and SHA256SUMS for download integrity.
