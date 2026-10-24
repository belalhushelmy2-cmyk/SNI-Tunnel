# SNI Tunnel for Android and Android TV 1.0.5

Two native Android apps share a VpnService, encrypted profile store and pinned
network runtimes. Both flavors use Overview, Servers, SNI profiles, Activity, Settings and
Diagnostics. The phone app has a navigation drawer; the TV flavor adds a
Leanback launcher, landscape layout, large focusable controls and D-pad focus
borders. Both accept VMess links, base64 subscription text, JSON profiles and
UTF-8 file imports; every import appends with new profile IDs. A TV without a
file picker can use its on-screen keyboard to enter a link.

Requirements: **Android 10/API 29 or newer**, **arm64-v8a or x86_64 userspace**,
Android's VPN API and permission to sideload APKs. A TV with a 64-bit CPU but a
32-bit Android OS is not supported. No root or separate Xray installation is
needed. Phone package: `com.snitunnel.android`; TV: `com.snitunnel.android.tv`.
Install the flavor appropriate to your device; only one VPN can be active.

## Usage

1. Install the matching APK and open SNI Tunnel from the phone/TV launcher.
2. Open Servers, import a VMess profile and press Use. Edit the server to adjust
   dial address, TLS SNI and WebSocket Host/path independently. SNI profiles can
   override SNI without changing the dial address or Host. Then connect from Overview.
3. Accept Android's VPN connection request. Allow notifications to see the
   foreground connection notification. Disconnect in the app or notification.

Supported: TCP, WebSocket and gRPC VMess, with standard verified TLS or plain
VMess. TLS SNI, WebSocket Host/path and dial address remain independent.
Unsupported legacy alterId and other transports are rejected. The server editor
also supports ALPN, fingerprint, multiplexing, an independent certificate
verification name and a trusted leaf SHA256 pin. Normal TLS validation stays on
by default. A pin replaces normal identity validation; use only a fingerprint you
trust. **Servers → Edit → TLS → Allow insecure** accepts the certificate presented
by that server without verifying its identity, matching desktop behavior. It
is off by default; imported `allowInsecure` preferences are preserved. Xray uses
a temporary certificate pin discovered for each connection/recovery attempt;
it is never saved as a trusted pin. An explicit trusted pin takes precedence.
Discovery makes at most three attempts (one normal, then two with TLS 1.2),
each limited to at most 10 seconds. If TLS 1.2 discovery succeeds, that session's
Xray connection also uses TLS 1.2. Activity names this stage and records retries;
failed discovery never starts the device VPN.
The phone menu and drawer respect status-bar, camera-cutout and keyboard insets. No VLESS, REALITY, subscription fetching or QR scanner is included.

The VPN captures IPv4/IPv6 and sends TCP/UDP through Hev's SOCKS5 forwarder to
Xray. This application's UID (including Xray) is excluded to prevent loops.
Other apps use the VPN; Android System VPN settings remain the authority on
routing. Before enabling the VPN, startup requires an actual SOCKS5 HTTPS response.
Periodic checks use two providers and run every 60 seconds by default; interval
and timeout are configurable. Activity displays redacted core diagnostics.
A successful check verifies the proxy; it does **not** prove every app's route,
DNS, IPv6 behavior or every external site's reachability. A failed check leaves
the VPN interface active and visibly degraded. Recovery makes up to three attempts while retaining an established VPN; manual
Reconnect resets that budget. Automatic recovery can be disabled in Settings.
Android always-on/lockdown is not implemented. Stopping/revoking
VPN releases its interface and child process. On service/process termination,
Android may restore ordinary connectivity; this is not a persistent kill switch.

Profiles are AES-GCM encrypted using an app-specific Android Keystore key.
Android app backup is disabled. Uninstalling loses local profiles and the key.
The phone and TV apps have independent stores; neither reads desktop data.
No real server credentials, private signing keys or user profiles are packaged.

## Reproducible inputs and build

Use JDK 17+ (tested JDK 21), Android SDK platform 35 / build tools 35.0.0 and
Gradle 8.11.1 through the checked-in wrapper. Point ANDROID_HOME to the SDK or
set `sdk.dir` in local.properties. Accept the SDK licenses using Android tools.

```sh
python3 android/fetch-runtime.py
cd android
./gradlew testPhoneDebugUnitTest testTvDebugUnitTest assemblePhoneDebug assembleTvDebug
```

Debug APKs are signed with the local Android debug key. For stable signed
release APKs, from the repository root:

```sh
python3 android/build-release.py
```

That script creates a private signing key under the git-ignored
`private/android-signing/` directory if no external key is supplied. **Back up
that directory securely**: Android updates must be signed by the same key.
Never commit it. To use an external key, set `SNI_ANDROID_KEYSTORE` and
`SNI_ANDROID_STORE_PASSWORD` (alias `sni-tunnel`). Future builds must increment
versionCode when changing the released app. Version 1.0.3 uses versionCode 4 and the same retained key as 1.0.0. Versions
are independent of the 2.x desktop app. No Play Store publication is implied.

Pinned components (SHA256 checked by fetch-runtime.py):

- Xray-core v26.10.21 official Android arm64/amd64 ZIPs, packaged as executables
  in nativeLibraryDir (`libxray.so`) so Android can execute bundled code.
- HevSocks5Tunnel 2.19.1 official JNI AAR; native class
  `hev.htproxy.TProxyService`. Its API 29 minimum determines app compatibility.

License texts ship in APK assets/licenses. Xray: MPL-2.0,
[source](https://github.com/XTLS/Xray-core/tree/v26.10.21). Hev: MIT,
[source](https://github.com/heiher/hev-socks5-tunnel/tree/2.19.1).
Android source remains separate from desktop binaries and stores.

## Validation

See the repository TEST_RESULTS.md for executed build/test/device checks.
Physical phone and Android TV checks, real-server forwarding, Wi-Fi handoff,
OEM battery management and long sessions must be reported separately from
successful compilation, signing or emulator startup.

## DNS and transfer rates — 1.0.4

UDP port 53 requests from Android's resolver are translated to DNS-over-TCP
through the same VMess outbound and the requested resolver. No device-side
public DNS bypass is added, and other UDP traffic is unchanged. Startup and
health checks require both HTTPS forwarding and a matching DNS answer.
The verification badge describes those probes; it does not certify every app.
Download/upload rates are bytes per second measured at the packet forwarder;
received/sent totals remain separate and accumulate across recovery.
See [the separate-app regression fixture](test-support/README.md).

## Arranged native screens — 1.0.5

Compact phone menu beneath system/cutout insets, a TV navigation rail with visible
D-pad focus, one connection action, grouped server/import/settings controls and
fixed metric cards. All six pages and existing TLS/SNI/preferences remain. This
release changes presentation only; the 1.0.4 networking repair is retained.
