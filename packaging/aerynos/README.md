# AerynOS x86_64 package

This package bundles the published native Slint Linux desktop, Xray 1.6.1 and
BadVPN tun2socks. Its payload is entirely under `/usr`, with file paths and
metadata encoded by the official AerynOS `stone` library. `moss` resolves shared
libraries and tools from the system's configured AerynOS repositories and manages
installation atomically. User profiles stay under `~/.local/share/sni-tunnel-v2`.

## Install and update

After these assets have been published in the GitHub release, register its
single-package repository:

```sh
sudo moss repo add sni-tunnel https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/latest/download/stone.index
sudo moss repo update
sudo moss install sni-tunnel-v2
```

For subsequent updates, keep the same repository and run:

```sh
sudo moss repo update
sudo moss sync --update
```

The sync command also upgrades other installed system packages through their own
repositories; review the transaction. Disconnect and quit SNI Tunnel first.
Launch **SNI Tunnel V2** from the application menu or run `sni-tunnel-v2`.

Do not manually copy files into `/usr` on AerynOS. The generic installer refuses
to overwrite an existing moss installation. The desktop uses the existing Linux
TUN helper and requires a working kernel TUN device and desktop polkit agent.
Clipboard buttons use wl-clipboard on Wayland; the tested AerynOS repository does
not supply xclip/xsel, so external clipboard buttons are unavailable on X11
unless the user supplies one. Manual text entry remains available.

For an offline/local package, put the `.stone` and matching `stone.index` in a
permanent directory, then register `file:///absolute/path/stone.index` with
`moss repo add`. Do not point a persistent repository at a temporary directory.
The index records the package's SHA-256 and relative download path.

## Rebuild

Install Rust/Cargo, a C compiler (for zstd) and moss, then:

```sh
python3 packaging/aerynos/build.py --output releases/v2/aerynos
```

The build downloads and SHA-256 verifies the desktop archive and official
os-tools v0.26.1 source archive (commit
`6f9a00db96002c234d2e81731f9766dc7facfcbe`). A small wrapper uses the unmodified
upstream stone writer; it does not reimplement the file format. Cargo dependencies
are locked. The result is inspected and indexed with moss. Use `--archive` and
`--tools-archive` to supply already downloaded, verified archives.

Raise both the source release/build release appropriately for future updates.
This third-party package repository contains only SNI Tunnel, and does not
replace or enable AerynOS volatile repositories. See the official
[AerynOS packaging workflow](https://aerynos.com/packaging/workflow/basic-workflow/).
