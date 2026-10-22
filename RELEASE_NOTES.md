SNI Tunnel V2 2.0.1 fixes the Servers page showing only one row when several
profiles were saved. All saved profiles now occupy separate, clickable rows.
Search filters the displayed model and shows both the matching and saved counts;
row actions keep their original profile indices after sorting or filtering.

The existing 2.0.0 features remain: append-only imports, native Linux desktop
tray, measured TCP latency every three seconds, and the premium native Slint UI.
No user profile migration or deletion is needed for this fix.

Validation: the previous layout failed the native five-row click regression
(first click hit profile 5 instead of profile 1). The repaired normal and compact
layouts passed at 100% and 150% scale. The Rust regression suite and both platform
builds must pass before this release is published. See TEST_RESULTS.md and
MIGRATION.md for executed checks and platform/networking limits.

Linux AppImage, DEB and portable tar, Windows x64 installer, complete source,
validation/migration notes and SHA256SUMS are included. V2 uses the existing
separate V2 profile store. Releases 2.0.0 and 0.4.3 remain available.
