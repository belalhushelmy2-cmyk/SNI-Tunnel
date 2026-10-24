# SNI Tunnel 2.1.4 — traffic bars and latency history

- Paired download/upload bars show real traffic history again.
- Fixed, equal metric cards and health indicators keep their geometry when numbers change.
- A separate latency chart uses the existing DNS + TCP reader every three seconds.
  It retains the last 60 probes; failed probes leave gaps rather than fabricated zeros.
  Selecting another endpoint clears its history. This is server connect latency, not
  an HTTPS or full-tunnel health measurement.
- Centered action icons, readable premium navigation and all existing pages/settings.
- Desktop Xray-core, routing, DNS, transport, MUX and recovery behavior are unchanged.

Includes the latest signed Android phone and TV 1.0.4 APKs, unchanged from 2.1.3:
DNS-over-TCP through VMess fixes the reproduced separate-app DNS failure, with real
transfer rates and recovery-safe totals. Both flavors passed the controlled
separate-UID 2 MiB download fixture and TLS fallback regression. Physical-device
behavior and the user's server are not inferred from those controlled tests.

The Linux update command continues to select the Latest release. Existing profiles
and application signing identities are retained.
