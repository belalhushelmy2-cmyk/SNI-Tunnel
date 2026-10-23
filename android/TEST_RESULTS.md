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
