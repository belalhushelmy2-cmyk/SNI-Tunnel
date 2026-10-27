# macOS and FreeBSD preview builds

These native desktop builds use the same Slint interface, profile editor,
custom SNI, MUX, measured traffic and automatic proxy reconnection as Linux.
They bundle a SHA-256 pinned Xray 26.3.27 (matching the desktop proxy backend) for their OS and architecture.
They are **local HTTP/SOCKS proxy clients**: full-device TUN, system proxy
management and kill switch protection are not implemented on these platforms.
Configure the application that should use the tunnel with the HTTP/SOCKS ports
shown in Settings (defaults 2080 and 1080 on 127.0.0.1). Other applications keep
using their existing network connection. Do not interpret the proxy health check
as whole-device protection. Local proxy mode carries TCP, not arbitrary UDP.

For a certificate that is untrusted or does not match your custom SNI, obtain
its verified SHA-256 fingerprint from the server administrator and enter it in
the server editor's certificate-pin field. These platforms keep the existing
strict certificate-discovery behavior: enabling Allow insecure alone does not
silently trust an unverified certificate. Explicit pins are supported and wrong
pins are rejected.

## macOS 12 or later

Choose the arm64 disk image for Apple Silicon, x86_64 for Intel. Mount the DMG,
drag SNI Tunnel.app to Applications, and launch it. Quit the old copy before
replacing the app to update. Profile storage is retained at
`~/.local/share/sni-tunnel-v2`.

The app is ad-hoc signed for executable integrity. It is not Apple Developer ID
signed or notarized; macOS may require approval in Privacy & Security. No Apple
signing identity is included in this repository. Clipboard uses pbcopy/pbpaste;
start-at-login writes a per-user LaunchAgent for the next login. Closing the
window exits; there is no menu-bar tray implementation yet.

Build on the matching Mac architecture:

```sh
cargo build --release --locked
python3 packaging/native/package.py --output releases/v2/macos
```

## FreeBSD 14 amd64 desktop

Install the package and repository dependencies with:

```sh
sudo pkg install ./sni-tunnel-v2-2.1.5.pkg
```

Launch SNI Tunnel V2 from the desktop menu or run `sni-tunnel-v2 gui`.
Use an X11/Wayland desktop. The optional tray needs a StatusNotifier host and
session D-Bus. Native pkg upgrades retain the same per-user profile directory.

Build on FreeBSD after installing Rust, Python 3, pkgconf, fontconfig,
libxkbcommon, libX11, libXcursor, libXrandr, libXi, libxcb, mesa-libs, wayland,
glib, dbus, xclip and ca_root_nss:

```sh
cargo build --release --locked
python3 packaging/native/package.py --output releases/v2/freebsd
```

Use Rust 1.99.1 for parity with the desktop release. Packages must be built
natively; Linux binaries and helpers are never substituted.
