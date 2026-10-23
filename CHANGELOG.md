# 2.1.0 — connection health and recovery

- 60-second full-tunnel checks compare explicit proxy and ordinary IPv4 exits, fresh DNS replies, routes and TUN traffic. Probe responses and deadlines are bounded; IPv6 protection is checked without an external IPv6 request.
- Checks use independent providers, preserve a verified session exit baseline, and never use the VMess dial address as exit evidence.
- Linux netlink and Windows interface/route notifications trigger coalesced checks. Core exits remain immediate failure signals; disconnect cancels scheduled recovery.
- Bounded reconnect delays start at 1, 2, 4, 8 and 15 seconds with jitter. Linux pauses retries while the physical network is unavailable.
- Linux nftables and Windows dynamic WFP guards survive core restart and enforce output policy independently of TUN routes. Disconnect releases application-owned protection.
- Connection Health settings/panel, Verify Now, Reconnect Now, emergency disconnect, native desktop notifications and in-app notification history.
- Existing server credentials, SNI, transport, MUX and explicit preferences are preserved; pinned cores are unchanged.

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


## Android phone and TV 1.0.0 — 2026-10-23

Added a separate native Android project with phone and TV flavors, VMess imports,
independent SNI/Host configuration, Keystore AES-GCM profile storage, foreground
VpnService, official SHA256-pinned Xray/Hev runtimes, real HTTPS health checks and
manual disconnect cleanup. Android 10+ / arm64-v8a and x86_64; TV Leanback entry,
landscape UI and D-pad focus. Signed APKs built locally with a retained private
key; no secrets committed. Both release lint gates passed; eight unit tests per
flavor passed. API 35 x86_64 emulator: both installs/launches, import, VPN consent,
synthetic VMess TCP forwarding, health verification and disconnect passed; TV
UDP DNS and phone process-restart profile retention passed. Physical devices,
IPv6/handoff/soak and Play Store distribution remain untested. See
android/TEST_RESULTS.md for exact scope. Desktop release assets are unchanged.


## Stability and matching Android workflow — 2.1.1 / Android 1.0.1

Fixed Linux socket timeouts accidentally divided by six, introduced a 60-second
sustained diagnostic-failure window and 15-second event/retry spacing, retained
connected controls during degraded health, refreshed a changing egress baseline
only after current verification, and kept route/firewall integrity repair prompt.
A portable local hotfix was staged without restarting the user's active session.
131 Rust tests passed (5 opt-in tests ignored), plus 21 helper tests. Release
version metadata/packages advance to 2.1.1; previous release assets are preserved.

Android 1.0.1 adds the six desktop menu sections, individual server actions,
separate SNI profiles, complete editor, diagnostics/events and saved monitoring/
recovery preferences. Startup verifies forwarding before creating device VPN.
Two-provider bounded probes, explicit verification name / trusted leaf pin,
credential-redacted core logs, independent stats and bounded retained-VPN
recovery replace the single-page generic-failure workflow. Fifteen unit tests
per flavor and both lint gates passed. API 35 emulator checks passed for both
menus, append/search/Use, proxy + ordinary TCP forwarding and disconnect;
untrusted custom-SNI TLS failed before VPN capture and an explicitly pinned
VMess/WebSocket/TLS test forwarded correctly. APK signatures match 1.0.0.
See android/TEST_RESULTS.md for exact scope and physical-device limitations.

Release CI also gates Android unit/lint/build checks and validates the locally
tested, signed APKs against android/release-assets.json before publication.
Publication is pending the workflow's successful final gates at this checkpoint.
