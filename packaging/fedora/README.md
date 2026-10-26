# Fedora RPM (x86_64)

Install or update the native package with Fedora's dependency resolver:

```sh
sudo dnf install ./sni-tunnel-v2-2.1.5-1.x86_64.rpm
```

Disconnect and quit SNI Tunnel before updating. The package includes the native
Slint application, Xray, BadVPN tun2socks, Linux network helper, application menu
entry, icon and third-party licenses. Fedora installs its own compatible runtime
libraries, PolicyKit, network tools and X11/Wayland clipboard utilities via DNF.
Saved profiles and settings remain in the existing V2 application data directory.

Launch **SNI Tunnel V2** from the application menu or run `sni-tunnel-v2`.
The repository terminal installer recognizes this RPM on subsequent updates and
uses `dnf install` with the verified release RPM instead of replacing package-owned
files. A running desktop PolicyKit authentication agent is required for TUN use.

The binary targets x86_64/glibc 2.30+; package installation is tested on Fedora 44.
This is a native RPM for mutable Fedora systems; Fedora Atomic uses its own
package layering workflow.

## Build

Build dependencies include `cargo-zigbuild` 0.24.0, `cargo-generate-rpm` 0.21.1,
Zig, CMake and Python 3. From the repository root:

```sh
./packaging/linux/build-portable.sh --rpm
```

To package an existing compatible binary without rebuilding it:

```sh
python3 packaging/linux/package.py \
  --binary target/x86_64-unknown-linux-gnu/release/sni-tunnel --rpm
```

The RPM build derives its glibc requirement from the actual binaries. Other
runtime dependencies are explicit because the build host is not Fedora and GUI
backends load some libraries dynamically. CI tests installation,
reinstallation, payload integrity, the desktop file and configuration retention
in the official Fedora 44 container before publishing future releases.
