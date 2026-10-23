# SNI Tunnel 2.1.2 — measured desktop proxy improvements

This publishes the desktop performance changes tested and installed locally.
Existing Linux 2.1.1 connection-health stability fixes are retained.

- Coalesce buffered HTTP request headers/body to remove a repeatable local
  delayed-ACK stall (controlled small POST median: 41.036 ms to 0.194 ms).
- Add optional Balanced, Low Latency and Maximum Throughput settings for the
  local HTTP/SOCKS relay. TUN traffic bypasses this relay; no gaming/TUN speed
  claim is made. Low Latency can reduce bulk-download speed, and larger buffers
  use more memory. See PERFORMANCE_RESULTS.md for complete tradeoffs.
- Cancel accepted relay tasks on stop and correctly account for initial HTTP data.
- Bound diagnostic address failover and label DNS + TCP measurements accurately.
- Preserve existing server credentials, SNI, TLS/security options, MUX, DNS, MTU,
  routing and pinned core versions. No WARP integration is enabled.

Linux: portable tar.gz (used by install.sh), AppImage, DEB, Arch binary package
and distribution-specific offline dependency bundles. Windows: installer and
portable ZIP. Android phone/TV APKs remain the unchanged, signed 1.0.3 builds.

Validation: 139 Rust and 21 Linux helper tests passed for the production changes;
Windows/Linux packaging CI is required before publication. Physical Windows
networking, long-session stability, roaming and suspend/resume are not claimed.
The installer command installs/updates the latest published release, preserving
profiles and registering the app launcher.
