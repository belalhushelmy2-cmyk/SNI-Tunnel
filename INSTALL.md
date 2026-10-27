# Install or update from the terminal

Run as your normal desktop user, **without sudo**:

```bash
curl -fsSL https://raw.githubusercontent.com/belalhushelmy2-cmyk/SNI-Tunnel/main/install.sh -o "$HOME/sni-tunnel-install.sh" && bash "$HOME/sni-tunnel-install.sh"
```

The same command installs or updates to the **latest stable GitHub release**.
It detects an existing Arch, DEB, RPM or Solus package and updates it using `pacman -U`,
`apt-get install`, `dnf install` or `eopkg install` with the matching verified release package. An older manual
installation under `/usr/lib/sni-tunnel` or `/usr/lib/sni-tunnel-v2` is updated
in place with a backup, provided no package owns it. If neither exists, it
installs or updates the user installation under `~/.local/opt/sni-tunnel-v2`.
Downloads are checked against the release SHA256SUMS before installation.
Runtime dependencies come from your own distro repositories.
Package transactions request sudo and display their normal confirmation prompt.
Arch uses `pacman -Syu` to avoid a partial system upgrade; review that transaction.
No Rust compiler, Cargo, FUSE or Git checkout is needed.

Prerequisites: `curl`, CA certificates, Python 3.8+, Bash, sudo for dependency
installation, and a normal Linux desktop session. Disconnect and quit SNI Tunnel
before updating. The installer refuses to replace a running app.

Launch it from the application menu as **SNI Tunnel**, or run:

```bash
~/.local/bin/sni-tunnel-v2
```

Update the detected installation using the saved updater:

```bash
~/.local/bin/sni-tunnel-v2 update
```

If `~/.local/bin` is on PATH, these are also `sni-tunnel-v2` and
`sni-tunnel-v2 update`. Re-running the repository command also obtains the newest
installer logic. User updates retain previous app directories and atomically switch
the current app; download/checksum/extraction failures leave the previous app
selected. Native packages use their package manager transaction behavior. Saved servers, keys and settings remain in the existing V2 data store.
Existing DEB/Arch installations are updated through their package manager;
package-owned files are never replaced by an unmanaged copy. The user command
forwards to that updated system installation. Earlier duplicate menu entries are
consolidated into one visible `SNI Tunnel` entry, including replacement of a stale
`Hidden=true` override, and the desktop application database is refreshed.
Use `--target user` only if you explicitly want a separate user installation.
Arbitrary AppImages outside these known locations require a separate manual
update; the installer does not scan your disk or replace unrelated files.
Desktop-session autostart configured with an older installation may need to be
turned off and on from the online app's settings.

Check compatibility and preview dependency commands without changing anything:

```bash
bash "$HOME/sni-tunnel-install.sh" --check
```

## Fedora native RPM

Download `sni-tunnel-v2-2.1.5-1.x86_64.rpm` from the latest release, then run:

```bash
sudo dnf install ./sni-tunnel-v2-2.1.5-1.x86_64.rpm
```

DNF installs Fedora runtime dependencies and handles native package upgrades.
The app appears as **SNI Tunnel V2** in the launcher. Saved profiles are retained.
The terminal installer above also detects this RPM and updates it through DNF.
See [Fedora package details](packaging/fedora/README.md).

## Solus native package

Download `sni-tunnel-v2-2.1.5-1-1-x86_64.eopkg` and verify its SHA256SUMS entry:

```sh
sudo eopkg install ./sni-tunnel-v2-2.1.5-1-1-x86_64.eopkg
```

The same command upgrades the package and retains profiles. The terminal installer
also detects an existing Solus package. See [Solus details](packaging/solus/README.md).

## AerynOS native package repository

Use moss to install the `.stone` package and its runtime dependencies atomically:

```sh
sudo moss repo add sni-tunnel https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/latest/download/stone.index
sudo moss repo update
sudo moss install sni-tunnel-v2
```

For later updates: `sudo moss sync --update`. This also updates other installed
system packages; review its transaction. Keep the default AerynOS repositories.
The application repository supplies only SNI Tunnel. Do not copy the portable
archive directly into `/usr` on AerynOS. See [AerynOS details](packaging/aerynos/README.md).

## macOS and FreeBSD preview builds

These are native **local HTTP/SOCKS proxy clients**. Full-device TUN, kill switch
protection and automatic system proxy configuration are not implemented for
macOS or FreeBSD. Configure your browser/application with the local proxy ports
shown in Settings. Other applications keep using their existing network route.

On macOS 12+, choose the Apple Silicon arm64 or Intel x86_64 DMG, mount it and drag
**SNI Tunnel.app** to Applications. Quit before replacing it to update. The app is
ad-hoc signed, not Developer ID signed or notarized; macOS may require approval
in Privacy & Security.

On FreeBSD 14 amd64 with an X11/Wayland desktop:

```sh
sudo pkg install ./sni-tunnel-v2-2.1.5.pkg
```

See [preview support and build instructions](packaging/native/README.md).
Native build jobs must pass before these packages are published.

## Linux compatibility

Published binaries support **x86_64 with glibc 2.30 or newer**. Automatic runtime
package installation is provided for these package-manager families:

| Package manager | Distributions/families |
| --- | --- |
| apt-get | Debian, Ubuntu, Mint, Pop!_OS and compatible derivatives |
| pacman | Arch, EndeavourOS, Omarchy and compatible derivatives |
| dnf | Fedora and compatible RPM systems with the listed packages available |
| zypper | openSUSE |
| eopkg | Solus |
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
