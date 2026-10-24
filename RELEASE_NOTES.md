# SNI Tunnel 2.1.3 — desktop interface and Android DNS repair

## Desktop

- Premium navy desktop design following the supplied reference: fixed icon sidebar,
  rounded connection/server/IP cards, blue actions and bundled Inter typography.
- Seven connection-health indicators, measured download/upload/session cards and
  real traffic history. Disconnected charts remain empty.
- Server search with Ctrl+K, focused keyboard controls, hover feedback, tooltips,
  adaptive spacing and scrolling at small sizes. Consistent styling on all pages.
- Retains single-click navigation, keyboard access, server editing, compact
  layout, scrolling, performance modes and all existing connection settings.
- No changes to desktop cores, VMess transport/security, DNS, MTU or routing.

## Android phone and TV 1.0.4

- Fix a reproduced failure where HTTPS proxy checks pass but another app cannot
  resolve names when remote UDP DNS is blocked. UDP DNS now travels as TCP
  through the same VMess connection and resolver; no direct DNS bypass.
- Check tunneled DNS before reporting probe success and during monitoring.
- Show actual per-second transfer rates separately from sent/received totals.
  Totals survive core recovery. Use the supported native forwarder timeout keys.
- Existing encrypted profiles, application IDs and signing identity are retained.

A separately installed test app reproduced DNS failure on 1.0.3 and verified a
complete 2 MiB download by hostname on 1.0.4 for both flavors with remote UDP
blocked. This is controlled emulator evidence, not confirmation of a repair on
every physical phone/TV or of the user's older TLS failure. TLS verification
preferences and custom SNI/Host are preserved.

Linux packages, Windows installer/portable ZIP, and signed phone/TV APKs are
provided below. The existing install.sh command selects the Latest desktop release.
