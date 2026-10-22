# SNI Tunnel V2 AppImage

The x86_64 image launches the native V2 Slint interface with the left sidebar.
The default UI scale is 100%; an explicit SLINT_SCALE_FACTOR value overrides it.
It excludes the previous Tk `app.py` launcher. It neither replaces the old app
nor bundles any user profiles or keys. Existing V2 data remains in the standard
V2 data directory described in MIGRATION.md.

Run:

```sh
chmod +x SNI-Tunnel-V2-2.0.1-x86_64.AppImage
./SNI-Tunnel-V2-2.0.1-x86_64.AppImage
```

CLI commands are forwarded, for example `--help` or `migrate SOURCE_DIRECTORY`.
Keep the AppImage at its chosen path if enabling desktop-session autostart.

The image bundles the native client, Xray, BadVPN, network helper, branding and
licenses. Host requirements remain: Linux x86_64, glibc >=2.30, Python 3.8+,
fontconfig and normal X11/Wayland desktop libraries. System-proxy mode uses the
host's gsettings. TUN additionally requires host PolicyKit/pkexec, iproute2,
kmod and a working TUN device. It still requests permission through pkexec.
An AppImage cannot supply those host networking/privilege services itself.

If FUSE mounting is unavailable:

```sh
./SNI-Tunnel-V2-2.0.1-x86_64.AppImage --appimage-extract-and-run
```

The launcher verifies its payload hashes and copies the executables/helper to
`$XDG_CACHE_HOME/sni-tunnel-v2/appimage/<content-hash>` (normally under
`~/.cache`). This ordinary-filesystem copy lets the separately elevated helper
read its bundled core without depending on access to a user-owned FUSE mount.
It is executable cache only; credentials/configuration remain in the V2 store.
Cache versions are retained; they may be removed with V2 stopped and are recreated
on the next AppImage launch. A modified cache is rejected rather than executed.

Build after the portable Linux release:

```sh
python3 packaging/linux/appimage.py
```

The builder downloads hash-pinned official appimagetool 1.9.1 and type2-runtime
20251108 into its build-tool cache. It supplies the pinned runtime explicitly;
it does not select an unversioned continuous release or copy host Arch libraries.
The output is `releases/v2/linux/SNI-Tunnel-V2-2.0.1-x86_64.AppImage`.

References: [AppDir specification](https://docs.appimage.org/reference/appdir.html),
[appimagetool](https://github.com/AppImage/appimagetool/tree/1.9.1),
[FUSE fallback](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

Full-device TUN, recovery across Wi-Fi/suspend and target-distribution installation
remain the V2 test limits described in TEST_RESULTS.md. Do not start a V2 TUN
connection while the previous client owns the active tunnel.
