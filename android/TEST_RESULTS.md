# Android 1.0.5 UI validation — 2026-10-24

- Signed phone and TV release builds passed; 33 JVM tests per flavor and zero
  lint errors. Both signatures match 1.0.4; application IDs retained, versionCode 6.
- API 35 emulator: adb install -r upgraded both existing apps while retaining
  synthetic profiles. Six pages reached using phone menu and TV sidebar.
- Tall display-cutout overlay: phone menu is below the notch and opens on one tap.
- TV D-pad moved native focus between controls. The new metric cards show their
  full totals without resizing; phone uses two columns and TV uses four.
- Phone and TV imported and selected a synthetic profile, edited Allow insecure,
  saved and reopened it to verify persistence. No real profile credentials used.
- Changes are presentation-only. The 1.0.4 DNS/transfer and TLS fallback code is
  retained byte-for-byte; earlier controlled networking evidence is below.

Physical-device networking/remote behavior remains unverified. Emulator UI checks
are not evidence about the user's server or WAN speed.

## Earlier validation

# Android 1.0.4 validation — 2026-10-24

- Signed phone/TV release builds passed; 33 Java tests per flavor (66 executions),
  zero lint errors. Existing application IDs/signing identity retained; versionCode 5.
- A separate-UID Android test app reproduced the 1.0.3 failure: proxy HTTPS
  succeeded but hostname resolution failed when the synthetic server dropped UDP.
- With 1.0.4, both flavors resolved the fixture hostname and downloaded 2 MiB
  with SHA-256 91d3beb88a9b2f778a6c44a1c53b63d3c79931845a9aef84b3fb414610bd1938.
  The payload was loopback-only; these timings are not WAN performance evidence.
- Both flavors also passed the TLS 1.3 rejection → TLS 1.2 discovery → native
  Xray TLS 1.2 forwarding regression. Disconnect/cleanup completed.
- Nine new tests cover DNS routing through VMess without a freedom/direct
  outbound, preserving profile fields, reply ID/header validation, classified
  DNS failures and rate/total continuity across core recovery.
- Reproduction sources are in [test-support](test-support/README.md); the test-only
  networkprobe package is never part of the phone or TV release APK.

Physical phone/TV, the user's real server and custom Private DNS providers remain
unverified. Probe success describes the measured proxy/DNS path, not every app's
route. No private profile/UUID/signing key is included in release artifacts.

## Earlier validation

# Android 1.0.3 validation — 2026-10-23

- Signed release phone/TV builds succeeded; 24 Java tests per flavor passed
  (48 executions), zero lint errors. Signing identities match 1.0.2; versionCode 4.
- Regression tests cover bounded discovery retries, TLS 1.2 fallback propagation
  into Xray configuration, no saved-profile mutation, strict/explicit-pin bypass,
  failure without creating a pin, and current-stage error classification.
- API 35 emulator phone and TV: a controlled frontend rejects every ClientHello that
  offers TLS 1.3. Normal discovery is rejected, TLS 1.2 discovery succeeds,
  and Xray's native TLS 1.2 path then verifies HTTPS and forwards an ordinary
  Android shell HTTP request through VMess/WebSocket/TLS. Disconnect passed.
- The controlled test caught browser-fingerprint handshakes continuing to offer
  TLS 1.3 despite protocol limits. TLS 1.2 fallback now selects native TLS with
  the session pin; normal configured fingerprints are unchanged.

Physical device/network behavior and real-user VMess forwarding remain unverified.
No real server credentials or real-profile diagnostics are included in artifacts.

## Previous validation

# Android 1.0.2 validation — 2026-10-23

- Signed phone/TV release builds succeeded; 19 JVM tests per flavor passed
  (38 executions), zero lint errors. Existing localization/banner warnings and
  expected warnings for explicit insecure certificate discovery remain.
- Both signatures match 1.0.1; versionCode 3. `adb install -r` updated both
  existing emulator apps while retaining their profile stores.
- API 35 x86_64 phone with the tall display-cutout overlay: menu button starts
  below the cutout/status bar and opens the drawer with one click.
- Phone: edited a synthetic TLS server, enabled Allow insecure, saved/reopened
  the editor and verified the switch remained checked. Connected and forwarded
  an ordinary Android shell HTTP request through a self-signed VMess/WS/TLS
  endpoint with independent custom SNI/Host. Disconnected successfully.
- TV landscape: the same edit/save/reopen and self-signed VMess/WS/TLS
  forwarding check passed, followed by Disconnect.
- The insecure compatibility path discovers a session-only leaf pin, preserving
  saved server fields. Strict TLS and explicit pins bypass discovery, covered
  by regression tests. Unresolved insecure configurations cannot start Xray.

Physical notched devices and physical TV remote behavior remain unverified.

## Previous 1.0.1 validation

# Android 1.0.1 validation — 2026-10-23

Build: JDK 21, Gradle 8.11.1, SDK/build tools 35. Minimum API 29, target 35,
arm64-v8a/x86_64. Signed phone and TV APKs built with zero lint errors.

- Fifteen Java regression tests passed for each flavor (30 executions): imports,
  independent dial/SNI/Host, atomic batch rejection, UUID/port/transport validation,
  verification-name/pin handling, refusal to silently enable unverified TLS, and
  redaction/bounded event history.
- apksigner verified both APKs. Their signing certificate matches both 1.0.0
  APKs; `adb install -r` updated existing emulator installations and retained data.
- API 35 x86_64 emulator: both phone drawer and TV sidebar reached Overview,
  Servers, SNI profiles, Activity, Settings and Diagnostics with one click.
- Both flavors appended a synthetic profile, searched it, selected Use and
  displayed the selection in Overview. Existing synthetic profiles were kept.
- Both flavors verified actual HTTPS through Xray before enabling Android VPN,
  forwarded an HTTP request from the Android shell UID through VMess to an
  isolated host endpoint, and disconnected successfully.
- A self-signed VMess/WebSocket/TLS server with custom SNI was rejected without
  a trusted certificate pin. Activity showed the certificate error and no
  device VPN interface was left active after failed startup.
- With an explicit SHA256 leaf certificate pin, the same controlled endpoint
  connected with independent custom SNI / WebSocket Host and forwarded an
  ordinary test request through VMess/WebSocket/TLS. The test server also
  recorded UDP DNS forwarding to 1.1.1.1:53.
- APKs bundle pinned Xray 26.10.21 / Hev 2.19.1 and license texts. No credentials
  or signing keys are included. Native ARM64 ELF LOAD alignment was checked at
  16 KB. Desktop connections/profile stores were not used as test credentials.

Limits: standard Android emulator, not physical phones/TVs or Google TV launcher
images. Real-user servers, OEM battery/network behavior, older API-level runtime
coverage, IPv6 leak/forwarding proof, Wi-Fi/cellular handoff and long soak remain
unverified. The user's exact screenshot error was not reproduced with their
credentials; certificate rejection is a demonstrated failure mode and errors
are now visible rather than hidden. English/localization/vector-banner lint
warnings remain. No persistent Android lockdown or Play Store publication.

Previous 1.0.0 results/artifacts remain in the earlier source revision/output
folder. This report describes the modified 1.0.1 build only.
