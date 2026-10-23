# Install or update from the terminal

Run as your normal desktop user, **without sudo**:

```bash
curl -fsSL https://raw.githubusercontent.com/belalhushelmy2-cmyk/SNI-Tunnel/main/install.sh -o "$HOME/sni-tunnel-install.sh" && bash "$HOME/sni-tunnel-install.sh"
```

The same command installs or updates to the **latest stable GitHub release**.
It downloads the Linux archive and SHA256SUMS from that release, verifies the
archive before execution, installs runtime dependencies using your own distro's
repositories, then installs the app under `~/.local/opt/sni-tunnel-v2`.
Package transactions request sudo and display their normal confirmation prompt.
Arch uses `pacman -Syu` to avoid a partial system upgrade; review that transaction.
No Rust compiler, Cargo, FUSE or Git checkout is needed.

Prerequisites: `curl`, CA certificates, Python 3.8+, Bash, sudo for dependency
installation, and a normal Linux desktop session. Disconnect and quit SNI Tunnel
before updating. The installer refuses to replace a running app.

Launch it from the application menu as **SNI Tunnel V2 (online)**, or run:

```bash
~/.local/bin/sni-tunnel-v2
```

Update an installation made by this installer:

```bash
~/.local/bin/sni-tunnel-v2 update
```

If `~/.local/bin` is on PATH, these are also `sni-tunnel-v2` and
`sni-tunnel-v2 update`. Re-running the repository command also obtains the newest
installer logic. Updates retain previous app directories and atomically switch
the current app; download/checksum/extraction failures leave the previous app
selected. Saved servers, keys and settings remain in the existing V2 data store.
An existing DEB/Arch installation is retained; launch the **online** menu entry to
use this installation. It does not modify package-manager ownership records.
Desktop-session autostart configured with an older installation may need to be
turned off and on from the online app's settings.

Check compatibility and preview dependency commands without changing anything:

```bash
bash "$HOME/sni-tunnel-install.sh" --check
```

## Linux compatibility

Published binaries support **x86_64 with glibc 2.30 or newer**. Automatic runtime
package installation is provided for these package-manager families:

| Package manager | Distributions/families |
| --- | --- |
| apt-get | Debian, Ubuntu, Mint, Pop!_OS and compatible derivatives |
| pacman | Arch, EndeavourOS, Omarchy and compatible derivatives |
| dnf | Fedora and compatible RPM systems with the listed packages available |
| zypper | openSUSE |
| xbps-install | Void Linux **glibc** |

These are dependency mappings, not a claim of completed installation tests on
every distribution. Packages must exist in your enabled repositories. Immutable
systems such as Fedora Atomic require a suitable host/container setup rather
than treating the base OS as a mutable dnf system.

Other glibc distributions can use `--no-deps` after installing the equivalent
runtime packages themselves. Required libraries/tools: fontconfig, GLib tools,
libxkbcommon/X11 support, libxcb/XKB, X11/Xcursor/Xrandr/Xi, Wayland client,
Python 3.8+, CA certificates, iproute2, nftables, libnotify, PolicyKit/pkexec, kmod,
D-Bus; wl-clipboard/xclip enable clipboard import. A working `/dev/net/tun` and a
running desktop PolicyKit authentication agent are needed for TUN authorization.
The installer does not change routes, connect a tunnel, or enable system services.

```bash
bash "$HOME/sni-tunnel-install.sh" --no-deps
```

ARM, Alpine/musl, glibc older than 2.30 and native NixOS without the standard ELF
loader are not supported by the current release binaries. Those targets need
separate builds/environments; one shell command cannot make an incompatible ELF
binary work on every distro.

Remove only the online installation (saved profiles are retained):

```bash
python3 ~/.local/opt/sni-tunnel-v2/installer.py --uninstall
```
