# SNI Tunnel V2 2.0.2 — offline desktop bundles

New complete offline installers for **Arch/Omarchy x86_64** and **Ubuntu 24.04 x86_64**, available as executable `.bin` files and extractable `.tar.gz` archives. Each contains SNI Tunnel, Xray, the Linux TUN helper, the AppImage, portable files, and the distribution's full runtime dependency closure, including Python, clipboard utilities, fonts, polkit and a graphical authentication agent.

- Arch also has a native `sni-tunnel-v2-bin` pacman package and checksum-pinned PKGBUILD. Dependencies in the offline bundle are official Arch packages verified against repository SHA256 values and package signatures.
- Ubuntu dependency packages are downloaded and authenticated by apt. The offline installer uses only the local package repository.
- Windows installer and portable ZIP include Xray, compatibility runtime, DLL, and TAP driver payload. Windows itself and its built-in system components remain prerequisites. TAP installation requests Administrator rights.
- Run Linux `.bin` installers as your normal desktop user. The installer requests administrator permission for package installation, and starts a graphical authentication agent if none of the supported agents is running.
- Existing profiles are retained. The server-list, import, tray and live TCP latency fixes from 2.0.1 remain included.

## Distribution compatibility

These are distribution-specific bundles, not an entire operating system. They require an existing x86_64 graphical Linux installation with a working package manager, sudo, and kernel TUN support. On Arch, if the bundled dependencies require changing an installed system library, the installer stops and asks for a complete system update instead of performing an unsupported partial upgrade. Ubuntu offline packages target 24.04; other Debian/Ubuntu releases are not claimed compatible. The standalone DEB/AppImage/tar packages remain available; use the matching offline installer when dependencies are missing.

Android is not included. It requires a separate Android VpnService, protected outbound sockets, native core integration and device testing; the desktop TUN helper cannot run on Android.
