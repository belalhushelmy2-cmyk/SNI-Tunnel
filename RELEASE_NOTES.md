# SNI Tunnel 2.1.1 — stability fixes and Android workflow update

## Linux desktop

- Fixed probe socket timeouts being divided by six: the default 10-second
  preference previously became about 1.7 seconds per operation. It now applies
  to socket I/O, with a bounded full-check budget of three times that value.
- Repeated diagnostic failures must persist for at least 60 seconds before
  automatic recovery tears down a live tunnel. Follow-up checks and network
  event bursts are spaced by at least 15 seconds. Manual Verify stays immediate.
- A degraded health check keeps the session controls/uptime live; it no longer
  presents an intact tunnel as disconnected. Health remains visibly degraded.
- Verified egress changes refresh the baseline only after current proxy/system,
  route/protection and DNS checks pass. NAT/load-balanced egress changes alone
  no longer force a reconnect.
- Core death and route/firewall integrity loss still trigger immediate repair.
  Existing profile choices, firewall enforcement and manual stop are preserved.

Remote server HTTP 500 responses, WebSocket resets and account/server outages
can still interrupt traffic; these fixes do not guarantee server availability.

## Android phone and Android TV 1.0.1

- Matching Overview, Servers, SNI profiles, Activity, Settings and Diagnostics
  sections. Phone navigation drawer; TV sidebar with single-click/D-pad controls.
- Individual server rows with Use, Edit, Test, Duplicate, Export and Delete;
  append-only import, search, sorting and separate reusable SNI profiles.
- Complete server editor preserves dial address, port, UUID, transport, TLS SNI,
  WebSocket Host/path, certificate verification name/pin, ALPN, fingerprint and
  multiplexing independently.
- Server forwarding is verified before a device VPN is established. Failed TLS
  or WebSocket startup shows redacted errors in Activity instead of leaving the
  device in an unverified VPN with a generic green-looking status.
- Explicit verification-name and trusted leaf-certificate pin controls support
  custom-SNI deployments. Normal TLS verification remains enabled by default;
  imported requests for insecure TLS are never silently enabled.
- Bounded recovery retains an established VPN during retry; interval/timeout and
  reconnect preferences are saved. Stats refresh independently of health success.
- Signed with the same Android release identity as 1.0.0; versionCode 2 supports
  in-place APK updates without deleting saved profiles. Requires Android 10+
  and arm64-v8a or x86_64 userspace. Phone and TV stores remain separate.

## Packages and validation

Windows installer/ZIP, Linux DEB/AppImage/tar, Arch-bin and Arch/Ubuntu 24.04
self-contained offline installers are built by the release workflow. Signed
Android phone/TV APKs are checked against the public release digest manifest.
All checks complete before the draft is marked Latest; previous releases remain.

See TEST_RESULTS.md and android/TEST_RESULTS.md for executed checks. Android
hardware/OEM behavior, long-session soak and the user's particular remote server
have not been verified by emulator or unit tests. Android always-on lockdown is
not implemented; explicit Disconnect restores ordinary networking.
