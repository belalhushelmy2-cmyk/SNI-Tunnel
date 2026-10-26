# Separate-app VPN regression fixture

`networkprobe` is a test-only APK with its own UID; it is never included in the
phone/TV release. This catches failures a probe from the VPN app's excluded UID
cannot detect. Use a disposable Android emulator, not a physical user's profile.

1. Run `python3 android/test-support/tcp_only_fixture.py` on the host. It starts
   a loopback VMess fixture, drops all remote UDP, and provides TCP DNS plus a
   deterministic 2 MiB download. Only the app's small HTTPS health checks use
   the Internet; the download payload stays on loopback. Stop it after testing.
2. Build/install the signed phone or TV app and grant VPN permission. Import
   `work/android-dns-fixture/profile.json`; it contains synthetic test credentials.
3. Build `:networkprobe:assembleDebug` and install its debug APK separately.
4. On the emulator only, set Private DNS Off to isolate the UDP-DNS regression.
5. Connect the tunnel. Launch:

```sh
adb shell am start -n com.snitunnel.networkprobe/.ProbeActivity --es url http://download.sni-test.invalid/payload
adb shell run-as com.snitunnel.networkprobe cat files/result.json
```

Require `ok: true`, 2097152 bytes and SHA-256
`91d3beb88a9b2f778a6c44a1c53b63d3c79931845a9aef84b3fb414610bd1938`.
The old 1.0.3 build instead reports proxy success and the separate app gets
UnknownHostException under this fixture. Repeat on both flavors, then disconnect.
This reproduces one failure mode, not a physical-device/network guarantee.

## CPU wake-lock lifecycle (phone and TV 1.0.6)

On the disposable emulator, repeat for both application IDs
`com.snitunnel.android` and `com.snitunnel.android.tv`:

1. With the VPN disconnected, `adb shell dumpsys power` must have no current
   `PARTIAL_WAKE_LOCK` entry tagged `<application-id>:TunnelCpu`.
2. Connect using the fixture above; require actual proxy/DNS verification.
3. Check that a current partial wake-lock entry exists. Send
   `adb shell input keyevent KEYCODE_SLEEP`, then verify `mWakefulness=Asleep`
   and that the same wake lock remains held.
4. Run the separate-UID payload transfer while the display is off; require the
   same complete 2 MiB payload/hash, and confirm the screen remains asleep.
5. Wake the screen, disconnect, and require that the current lock entry disappears.
6. Try an unreachable synthetic server. After startup reports failure, require
   that no current CPU lock remains. Historical ACQ/REL entries are not held locks.

These emulator checks cover lifecycle/forwarding; they are not a guarantee about
physical OEM standby policies or battery optimization behavior.
