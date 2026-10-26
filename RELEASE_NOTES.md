# SNI Tunnel 2.1.5 — a clearer desktop, phone and TV

- Restore the compact desktop overview: slim navigation, teal connection panel,
  one server/public-IP row, fixed health tiles and equal download/upload/session cards.
- Keep full-width traffic bars from actual statistics. Switch to the measured
  DNS + TCP latency history without crowding the dashboard; failed probes leave gaps.
- Refine every page with consistent navy/teal colors, readable controls and native
  focus/hover states. All existing pages, configuration fields and actions remain.
- Android phone and TV 1.0.6 retain a clearer connection action, grouped server
  controls, fixed metric cards, compact phone menu and visible TV focus states.
  System-bar/cutout spacing, Allow insecure editing and stored profiles are retained.
- Android 1.0.6 adds a partial CPU wake lock for VPN startup, forwarding and recovery
  while the screen is off. Disconnect, startup failure, VPN revocation and service
  destruction release it. Both flavors passed screen-off 2 MiB transfer and lock
  lifecycle checks in the emulator.
- Desktop networking/core implementations are unchanged.

Includes Linux portable/archive, AppImage, DEB, RPM, Arch binary package, Windows
portable/installer and signed Android phone/TV APKs. Android retains its application
IDs and signing identity for upgrades. The Linux install/update command selects
Latest and preserves existing profiles.

Native UI rendering and emulator checks cover presentation and interaction;
physical phone/TV networks and the user's server are not inferred from those tests.
