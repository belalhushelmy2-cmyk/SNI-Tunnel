# Android 1.0.0 validation — 2026-10-23

Executed locally with JDK 21, Gradle 8.11.1, Android SDK 35/build tools 35.0.0.

- Phone and TV signed release APKs built successfully.
- Eight profile/config regression tests passed for each flavor (16 executions,
  no skipped/failing tests): independent dial/SNI/Host, append-only duplicate
  import, subscriptions, JSON arrays, invalid ports/UUIDs/transports and atomic
  rejection of malformed batches.
- Release lint: zero errors for both flavors. Warnings remain for localization
  of English status text and the 320x180 vector TV banner.
- apksigner verification passed; both APKs have the same retained private release
  signing identity. Keys/passwords remain outside tracked source and APKs.
- Both APKs contain byte-identical SHA256-pinned Xray arm64/x86_64 executables,
  Hev JNI libraries and third-party license texts. Native ARM64 LOAD segments
  are aligned to 16 KB. The APKs declare API 29 minimum, API 35 target and only
  arm64-v8a/x86_64 ABIs. TV declares a Leanback launcher/banner and optional touch.
- Installed both signed APKs successfully into an isolated Android 15/API 35
  x86_64 KVM emulator. Launched and inspected phone/landscape TV UI screenshots.
- Both flavors imported a synthetic VMess profile, accepted Android VPN consent,
  started their bundled core/packet forwarder, performed a successful proxied
  HTTPS health request, forwarded a TCP HTTP request from the Android shell
  UID (captured by the VPN) through VMess to a synthetic host endpoint, then disconnected.
  (The test shell UID was captured by the VPN; only the app/core UID bypasses it.)
- TV focus moved with D-pad key events. This was the TV flavor on a standard
  Android emulator, not a physical TV launcher/remote or Google TV system image.
- While TV VPN was active, Android resolved www.iana.org using the configured
  DNS server. The test VMess server recorded UDP forwarding to 1.1.1.1:53.
  ICMP echo itself is not supported; a failed ping was not counted as an ICMP pass.
- Encrypted profiles and selection persisted across a phone app process restart.
- Desktop profiles, running desktop tunnel and installed desktop app were not
  replaced or restarted. Emulator and synthetic server were isolated test data.

Not executed: physical ARM64 phone/TV installation, OEM-specific permission or
battery behavior, real-user VMess profiles, IPv6 end-to-end forwarding/leak tests,
Wi-Fi/cellular handoff, long-session soak, Android 10/11 runtime compatibility,
Play Store review or publication. No claim of persistent lockdown or automatic
reconnect: this first Android version exposes failure and supports manual control.

Signed APK filenames: SNI-Tunnel-Android-1.0.0.apk and
SNI-Tunnel-Android-TV-1.0.0.apk. See output SHA256SUMS for exact bytes.
