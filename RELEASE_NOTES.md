# SNI Tunnel V2 2.1.0 — connection health and recovery

Full-Tunnel mode now checks real forwarding every 60 seconds by default. It compares an explicit VMess proxy request with an ordinary IPv4 request, validates a fresh DNS response, checks routing and IPv6 protection, and checks the session's previously verified exit. Checks run off the GUI thread and do not inherit proxy environment variables.

Network changes and core exits trigger earlier checks/recovery. Automatic reconnection uses bounded backoff with jitter. The dashboard has connection health, verification timing, protection status, Verify Now, Reconnect Now and Disconnect / release protection. Settings include monitoring, interval, timeout, attempts and notifications. Notifications are grouped and retained in Activity.

Linux uses an application-owned nftables output guard; Windows uses a dynamic Windows Filtering Platform session. These remain independent of Xray's lifetime. Neither changes unrelated firewall rules. IPv6 internet forwarding is not claimed: this release protects IPv6 while verifying IPv4 forwarding. A small probe cannot prove every packet's route; enforcement and checks are complementary.

## Recovery and compatibility

- Manual disconnect releases protection. If the Linux privileged helper crashes, run `sudo /usr/lib/sni-tunnel-v2/network.py --recover` (or the helper beside your portable executable) to restore application-owned networking. Windows dynamic filters are removed when the application exits, avoiding permanent lockout.
- The legacy Windows TAP compatibility backend remains available, but cannot report full-tunnel verification without an independent health inbound. Standard Wintun is the supported verified path.
- Linux requires nftables and a working TUN kernel; native packages and distribution-specific offline bundles include the new dependencies. Linux notifications require the desktop's Freedesktop notification service (including Sway notification daemons). Windows uses native notification-area messages; OS notification preferences still apply.
- Saved servers and explicit settings are retained. New/missing health preferences use enabled defaults; an existing disabled automatic-reconnect preference stays disabled.
- Xray/core versions are unchanged. Previous release assets are preserved.

See TEST_RESULTS.md for tests actually executed and remaining runtime limitations. Windows EXE/portable ZIP, Linux DEB/AppImage/tar, native Arch-bin package, and Arch/Ubuntu 24.04 offline .bin bundles are built by the release workflow.
