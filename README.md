## Android phone and Android TV

Native Android 1.0.3 phone/TV apps are available in the [Android project](android/README.md).
Both require Android 10+ and 64-bit ARM/x86_64 userspace. They bundle Xray and
use Android's VPN permission flow. TV adds a Leanback launcher and D-pad controls.
See [Android validation](android/TEST_RESULTS.md) and its documented limits.
Desktop and Android versions, package IDs and user stores are separate.

## Install or update on Linux

```bash
curl -fsSL https://raw.githubusercontent.com/belalhushelmy2-cmyk/SNI-Tunnel/main/install.sh -o "$HOME/sni-tunnel-install.sh" && bash "$HOME/sni-tunnel-install.sh"
```

Run without sudo; dependency installation requests it when needed. Launch with
`~/.local/bin/sni-tunnel-v2`; update with `~/.local/bin/sni-tunnel-v2 update`.
Supports Linux x86_64/glibc >=2.30 with dependency mappings for apt, pacman, dnf,
zypper and xbps. See [terminal installation](INSTALL.md) for prerequisites,
other distributions, checks and removal.

## 2.1.1 desktop release

Choose the **Arch/Omarchy** or **Ubuntu 24.04** `offline.bin` installer for bundled runtime dependencies. Run it as your normal desktop user; it requests administrator authorization for installation. Both offline bundles also include the AppImage and portable files. Native Arch `-bin`, DEB, AppImage, portable Linux tar, Windows installer and Windows portable ZIP are also produced. See [offline packaging](packaging/offline/README.md) for prerequisites and build details. Android is not included.

# SNI Tunnel V2

A Rust + Slint desktop client for Windows and Linux using Xray-core, with
independent VMess credentials, TLS SNI, WebSocket Host/path and dial address.
The Linux client now uses the native Slint interface.

[Latest release](https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/latest)
· [Validation](TEST_RESULTS.md) · [Migration and rollback](MIGRATION.md)

## Downloads and launch

| Platform | Artifact | Launch |
| --- | --- | --- |
| Windows 10/11 x64 | `SNI-Tunnel-V2-Setup-x64.exe` | Install, then open SNI Tunnel V2 |
| Linux x86_64 | `SNI-Tunnel-V2-2.1.1-x86_64.AppImage` | Mark executable and open |
| Debian-family amd64 | `sni-tunnel-v2_2.1.1_amd64.deb` | Install using the package manager |
| Portable Linux | `sni-tunnel-v2_2.1.1_amd64.tar.gz` | Extract and run `start.sh` |

Verify downloads against the release's `SHA256SUMS`. The installers include their
Xray/helper runtime assets; Rust and Cargo are build tools, not user requirements.
Linux requires glibc >=2.30, Python 3.8+, fontconfig and X11/Wayland desktop
libraries. TUN also requires host PolicyKit, iproute2, kmod and a usable TUN device.
See [AppImage details](packaging/linux/APPIMAGE.md), including its FUSE-free fallback.

## Interface and connection behavior

- Dark sidebar, first-click navigation, centered menu rows and a scrolling dashboard.
- Actual traffic counters/history, session duration, active server/SNI and measured IP.
- Server search/sorting, clipboard and VMess import/export, duplication and testing.
- Imports always append profiles with fresh IDs and distinct names; existing profiles are kept.
- Live server TCP latency every three seconds (two-second timeout), with honest unavailable readings. This measures TCP setup, not tunnel/HTTP health.
- Native Linux StatusNotifierItem tray for Wayland/X11 desktops that provide a tray host. Close-to-tray, show, connect, disconnect, test and quit are supported; a missing host keeps the window accessible.
- Scrollable editor, UUID visibility, validation and unsaved-change confirmation.
- Searchable/redacted logs with copy/export and simple/advanced views.
- Bounded recovery, explicit degraded/error states and cleanup of owned resources.

A successful connection state requires forwarding checks. CPU/RAM observations
and the exact test scope are in [TEST_RESULTS.md](TEST_RESULTS.md). The first V2
release still has unexecuted Windows networking, target-distribution installation,
and Wi-Fi/suspend recovery checks; a successful build does not cover those cases.

Linux defaults to 100% UI scale. Set `SLINT_SCALE_FACTOR` explicitly to override it.
Its system-proxy adapter supports GNOME/Cinnamon-compatible gsettings desktops.
Other desktops can use the local HTTP/SOCKS listeners manually or use TUN. Windows compatibility options remain specific to Windows; Linux uses its native
tray integration and nftables-based protection.
The preserved Linux Xray 1.6.1/BadVPN path supports VMess WebSocket + TLS and rejects
unsupported XUDP/certificate-pin combinations rather than discarding settings.

## Saved profiles

V2 uses separate user data:

- Windows: `%LOCALAPPDATA%\SniTunnelV2`
- Linux: `$XDG_DATA_HOME/sni-tunnel-v2`, normally `~/.local/share/sni-tunnel-v2`

Migration requires an empty V2 store, keeps permanent original-byte backups and
verifies saved/reloaded profiles. Run it as the owner of the original profiles;
Windows DPAPI data must stay with the same Windows account. Example for the
previous Linux Tk store:

```sh
./SNI-Tunnel-V2-2.1.1-x86_64.AppImage migrate "$HOME/.config/sni-tunnel-linux"
```

Disconnect the previous tunnel first. Do not run old/new clients against the same
TUN adapter. Original profiles and release artifacts remain available for rollback.
See [MIGRATION.md](MIGRATION.md); the app does not automatically uninstall old clients.

## Build from source

Use the pinned lockfile. Linux GUI build dependencies include a C compiler,
fontconfig development files, CMake, Python and standard desktop development libs.

```sh
cargo test --all --locked
cargo build --release --locked
```

For the portable glibc-2.30 build, install Zig 0.17.0 and cargo-zigbuild 0.24.0,
then run:

```sh
./packaging/linux/build-portable.sh
python3 packaging/linux/appimage.py
```

Third-party source/runtime archives are restored and hash-verified automatically
from the retained v0.4.3 source asset. They are included in complete release source
ZIPs; `python3 packaging/linux/fetch-vendor.py` restores them in a Git checkout.
No development caches, private profiles, keys or runtime logs are packaged.

Windows uses the Rust GNU toolchain, w64devkit resources/import libraries, 7-Zip
and NSIS. `packaging/windows/build.ps1` builds the installer. The Actions workflow
contains the pinned clean-runner setup; the inherited Windows notes in AGENTS.md
explain the self-contained linker and compiler stack requirements.

GitHub Actions builds/tests both platforms. Publishing is requested by a main
commit containing `[publish-release]`, or by the workflow's explicit publish
input. Assets upload to a draft first; the workflow marks the release Latest only
after both builds and all uploads succeed. Published version tags are preserved.

## Performance controls and measurements

Settings → Local proxy performance offers Balanced (the conservative default),
Low Latency (TCP_NODELAY), and Maximum Throughput (TCP_NODELAY plus 64 KiB
relay buffers per direction). These apply on the next proxy session; they never
change saved server credentials, `ea.com` SNI, TLS verification, MUX, DNS or MTU.
The selector is disabled in TUN mode because that datapath bypasses this relay.

See [PERFORMANCE_BASELINE.md](PERFORMANCE_BASELINE.md) and
[PERFORMANCE_RESULTS.md](PERFORMANCE_RESULTS.md). The benchmark is local-only:

```sh
python3 scripts/benchmark-proxy.py --baseline-ref perf-baseline-20261023
```

It compiles the checkpoint's original proxy code into a temporary benchmark
adapter and compares it with the current relay using identical loopback
workloads. No real profiles, external downloads or system routing changes are used.
Local benchmark throughput must not be interpreted as Internet speed.

Cloudflare WARP on a provider's server requires its administrator. A VMess link
alone cannot install or reconfigure that server. Client-side WARP chaining is
[a separate Xray capability](https://xtls.github.io/en/document/level-2/warp.html)
requiring a registered WARP/WireGuard profile and a compatible core. This
performance change preserves the existing Linux 1.6.1 core, and does not add a
nonfunctional WARP toggle or register an external account.
