# Solus x86_64 package

The `.eopkg` bundles the release's native Slint desktop, Xray 1.6.1 and
BadVPN tun2socks. System libraries and tools are installed from Solus's own
repositories. No Linux distribution package can replace the destination OS's
libc, display server or desktop services with bundled copies.

Download the package from the GitHub release, verify its SHA256SUMS entry, then:

```sh
sudo eopkg install ./sni-tunnel-v2-2.1.5-1-1-x86_64.eopkg
```

The same command updates an existing package. The desktop menu entry is
**SNI Tunnel V2**, or run `sni-tunnel-v2`. Existing profiles remain in
`~/.local/share/sni-tunnel-v2`. Disconnect and quit the application before updating.
TUN uses the existing Linux helper, pkexec and the desktop's authentication agent.
A working Linux kernel TUN device and desktop polkit agent are required.

The terminal installer in INSTALL.md also recognizes an existing Solus package
and upgrades it through eopkg. A fresh terminal installation uses the existing
per-user installation workflow with Solus runtime dependency installation.

## Rebuild

Use a Solus build environment with ypkg (or solbuild). This recipe repackages the
SHA-256 pinned, published portable desktop; it does not rebuild Rust or Xray.

```sh
ypkg build packaging/solus/package.yml --output-dir /path/to/output
```

Keep `files/` next to package.yml. Install the runtime libraries in the build
root as well, so ypkg's ELF dependency resolver can identify their providers.
`strip: false` and `debug: false` preserve the upstream release executable.
Raise the numeric `release` for every future package update; Solus uses it for
upgrade ordering. This is a third-party package, not an official Solus repository
submission.
