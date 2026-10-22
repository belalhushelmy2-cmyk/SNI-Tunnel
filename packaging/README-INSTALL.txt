SNI TUNNEL V2 — SEPARATE RELEASE

Windows: SNI-Tunnel-V2-Setup-x64.exe installs under Program Files\SNI Tunnel V2.
Linux: sni-tunnel-v2_2.0.0_amd64.deb installs under /usr/lib/sni-tunnel-v2.
The GUI is native Rust/Slint on both systems. No Rust, Cargo, Tk or development
workspace is required at runtime. Linux includes the existing Xray 1.6.1/BadVPN
core and uses Python 3 only for the separately elevated network helper.

Linux installation (only after reviewing the release verification report):
  sudo apt install ./sni-tunnel-v2_2.0.0_amd64.deb
Start SNI Tunnel V2 from the desktop menu or run sni-tunnel-v2.
Portable Linux: extract the tar.gz and run ./start.sh in its directory.
The DEB records the actual minimum glibc requirement of its compiled binary.
Native builds may require a newer glibc than older Debian/Ubuntu installations;
consult TEST_RESULTS.md rather than assuming compatibility from the filename.

V2 uses a separate data directory. Import the previous store using the native
binary's 'migrate SOURCE_DIRECTORY' command. See MIGRATION.md in the source
release for exact paths, backup verification, limitations and rollback.
The previous app and its profiles are never automatically removed by V2.

Linux TUN permission is requested through pkexec. Only the network helper is
privileged. It uses the same /run/sni-tunnel lock and sni0 ownership journal as
the previous release, preventing simultaneous ownership of the TUN interface.
The preserved legacy Linux core supports VMess/WebSocket/TLS; unsupported XUDP
or certificate-pin combinations report compatibility errors without resetting
saved fields. The Windows TUN path retains modern Xray and Wintun.

GNOME/Cinnamon system proxy settings are supported. Other desktops can configure
the local HTTP/SOCKS listener manually or use TUN. The Linux kill-switch control
is explicitly unavailable rather than claiming unimplemented protection.

Do not replace the previous installation until live connection/recovery tests
have passed and you have explicitly approved the replacement stage.
