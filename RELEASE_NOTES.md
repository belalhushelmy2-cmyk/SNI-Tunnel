SNI Tunnel V2 adds the native Slint desktop interface on Linux alongside Windows,
with a dark sidebar, measured traffic dashboard, server search/sorting, a scrolling
editor and profile migration with permanent backups.

- Server imports show results on the Servers page and preserve distinct accounts
  and SNI/transport settings sharing the same endpoint.
- Sidebar items activate on the first click, with centered icons and equal rows.
- Linux launchers default to 100% scale and preserve explicit scale overrides.
- Linux AppImage, DEB and portable tar, Windows x64 installer, complete source
  archive, migration instructions and SHA256SUMS are included.
- V2 uses separate profile storage. Original profile files and backups are retained.
- Connection states and traffic counters come from the real networking backend.

Local validation included 121 Rust tests and 12 Python tests, real migration of
two saved profiles, HTTPS forwarding, and 12/12 HTTPS requests over 63.58 seconds.
The native UI input/layout harness checked all six sidebar actions at 100% and
150% scale. GitHub's build checks must pass before this release is published.

This is the first public V2 build. Read TEST_RESULTS.md and MIGRATION.md before
replacing a working installation. Full-device TUN across target distributions,
Wi-Fi/suspend recovery, and Windows installer fresh/upgrade/uninstall behavior
remain outside the executed validation. Windows CLI/build checks do not establish
all Windows networking behavior. Linux retains the bundled legacy VMess WS/TLS
transport restrictions documented in the migration notes. Host networking and
permission services are still required by the AppImage.

The prior v0.4.3 release remains available for rollback. No personal profiles,
UUIDs, keys, or real-network logs are included in these assets.
