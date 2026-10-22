# SNI Tunnel V2 — 2.0.0

- Native Slint interface on Windows and Linux, with an original dark visual identity,
  vector navigation, connection overview, measured traffic history, compact metrics,
  server sorting/search, a scrollable dashboard at short window heights, and a
  scrollable configuration editor.
- Independent V2 application data, autostart entries, installers and installation paths.
  Previous profiles are never edited in place.
- Non-destructive migration with verified original-byte backups, credential preservation,
  profile-count checks and complete save/reload comparison.
- Linux Slint TUN sessions delegate to the existing separately elevated Python/Xray/
  BadVPN helper. The interface remains unprivileged. The shared helper lock prevents
  old and new Linux TUN sessions from owning sni0 simultaneously.
- GNOME/Cinnamon proxy adapter captures settings before changes and restores only
  values still owned by this session; unsupported desktops expose manual proxy/TUN use.
- Linux DNS preferences reach the helper; externally changed resolver settings remain
  protected during restoration.
- Bounded recovery distinguishes degraded forwarding, unavailable physical routes,
  helper exits and fatal configuration errors. Manual disconnect cancels recovery.
- UUID visibility controls, inline basic validation, unsaved-edit confirmation,
  settings-save rollback, and preservation of extended VMess import/export fields.
- Software rendering on Linux, bounded traffic history, deferred hidden-log rendering,
  and removal of a synchronous core probe from the editor-opening path.

See TEST_RESULTS.md for executed tests and outstanding release gates. An installer
build is not proof that a VPN works on every supported operating system.

- AppImage distribution with the native V2 interface, hash-verified payload cache
  outside FUSE, and autostart entries that retain the AppImage path.

- Sidebar navigation responds to the first click, including icon/row padding hits;
  centered icons, labels and selection markers retain identical row geometry.
