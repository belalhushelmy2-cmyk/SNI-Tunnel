# V2 migration and rollback

## Isolation

V2 uses `%LOCALAPPDATA%\SniTunnelV2` on Windows and
`$XDG_DATA_HOME/sni-tunnel-v2` (normally `~/.local/share/sni-tunnel-v2`) on Linux.
`SNI_TUNNEL_DATA_DIR` explicitly overrides this location for isolated development.
Neither the old store nor the installation directory is used for V2 profile writes.

A private backup of the actual current Linux stores was created and verified inside
`private/previous-release-backup/`. It is deliberately excluded from Git, source
archives and release downloads. Two real legacy profiles were migrated into an
isolated private test store and compared after saving/reloading. No credentials
or real server endpoints appear in public release notes or screenshots.

## Import a previous store

Close/disconnect the previous application first. Do not run two TUN sessions.
Migration requires an empty V2 store and never overwrites an existing V2 config/key.

Linux (native Slint V2 binary):

```sh
/usr/lib/sni-tunnel-v2/sni-tunnel migrate "$HOME/.config/sni-tunnel-linux"
```

For an existing Rust store instead, explicitly pass its directory, normally
`~/.local/share/sni-tunnel`, containing `config.json` and `secret.key`.

Windows (run as the same user who owns the old profiles):

```powershell
& "$env:ProgramFiles\SNI Tunnel V2\sni-tunnel.exe" migrate "$env:LOCALAPPDATA\SniTunnel"
```

Migration copies original configuration bytes and the associated key into a
permanent private `backups/previous-<id>/` directory before parsing. Native stores
are copied without re-encoding; legacy Linux records are converted to native
profiles while the complete original JSON (including unknown preferences and
fields) is retained in the backup. The tool checks all converted profile fields
through save/reload and writes a migration receipt only after that comparison.
Migration failures retain the source and backup. Do not delete these backups.
Legacy unknown fields are preserved in the original backup; only fields supported
by the native model are active in the V2 interface.

## Replacement gates

Do not uninstall or replace the installed previous application just because V2
compiles. Verify both installers, real traffic, recovery, DNS/routes and migration
first, then obtain the user's explicit approval for the final uninstall/install.
At the user's subsequent explicit request, the existing Linux launcher was
updated in place on 2026-10-22. The original Tk client files remain under
`/usr/lib/sni-tunnel`; the V2 payload is alongside them in
`/usr/lib/sni-tunnel/v2-ui-2.0.0-cd06652`. The standard menu/command launcher
now opens V2. Both profiles were migrated and verified, and the old helper was
stopped gracefully with route/journal cleanup checked. The new GUI opened
without automatically reconnecting a tunnel. The installed launcher and
AppImage now default to 100% GUI scale at the user's request.

The system installation and launcher backup is
`/var/backups/sni-tunnel-ui-20261022-140839` (administrator access required).
V2's prior empty-store files were retained in its own `backups/` directory.
After disconnecting and closing V2, restoring `installation/start.sh` and the
saved `desktop` file from that backup restores the old launchers. No original
server data needs to be deleted or overwritten. Keep any subsequent V2 edits
backed up separately when rolling back.

The current Linux installation was located at `/usr/lib/sni-tunnel`. No registered
pacman/dpkg ownership or supported uninstaller was found in the initial inspection;
therefore a destructive directory deletion is not an acceptable replacement plan.
Identify its original installer/package mechanism before removing it.

## Rollback

Keep the previous release source/artifacts and the verified original data backup.
The Git tag `checkpoint-before-v2-20261022` identifies the previous source. V2 has
separate install paths and identifiers, so removing V2 through its package manager
or Windows uninstaller does not require removing the previous installation.
Never copy the V2 store over the previous store. Launch the previous application
with its untouched original data; disconnect V2 first. If recovery is needed,
compare backup manifests and restore only the relevant original store with the
application stopped, preserving current files in another backup first.
