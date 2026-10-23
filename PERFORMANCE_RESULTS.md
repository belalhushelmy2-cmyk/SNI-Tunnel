# Performance results — 2026-10-23

Code checkpoint: `144650d1f9594ca2dcc0e9566be8d8e7f1bd83bd`.
Baseline source: `ef7a36e344d2be62b48f52c4ced710df3a1eeccb` (identical relay
source to the local `perf-baseline-20261023` checkpoint).

## What changed

1. Buffered HTTP request headers/body are forwarded together. The original split
   writes repeatedly triggered a roughly 41 ms delayed-ACK/Nagle stall.
2. Optional TCP_NODELAY and bounded relay buffers are available in Settings.
   Balanced remains the conservative default. No profile is auto-switched.
3. Accepted proxy tasks belong to their listener and are cancelled on stop;
   counters are released on cancellation. Buffered initial HTTP data is counted.
4. Rust diagnostic/TLS TCP dialing tries alternate address families after a
   250 ms stagger, with at most four concurrent attempts and the original overall
   deadline. Failed attempts are cancelled; no application DNS cache was added.
   The latency label now correctly says DNS + TCP, not pure TCP RTT.

No Xray core, VMess destination/UUID, ea.com SNI, WebSocket Host/path, TLS
verification, ALPN, fingerprint, MUX/XUDP preference, DNS resolver, MTU, routing,
kill switch, notification policy or recovery policy was changed.

## Controlled before/after comparison

`python3 scripts/benchmark-proxy.py` extracts the checkpoint's **real original
proxy code** into a temporary adapter and runs the same workload against it and
the current relay. The adapter changes only the constructor signature. Results
below use two rounds in opposite orders (A/B/L/M, then M/L/B/A), five trials per
variant/round. No competing local build ran during the final response-validated comparison;
ordinary desktop background activity was not eliminated. All bytes stayed on
loopback. These are **not WAN, VMess, TLS, TUN, gaming or Internet-speed results**.

Per variant: 300 GETs, 300 512-byte POSTs, 300 two-frame CONNECT requests,
160 concurrent POSTs, and 240 MiB each of local upload and download payload.
The CONNECT workload deliberately separates header/body frames by a 1 ms sleep;
it exposes interactive stream buffering, not a real game's RTT. Zero request
failures occurred; the runner aborts on a failed/incomplete response.

Latency cells are median trial p50 / median trial p95, milliseconds.

| Workload | Checkpoint | Balanced | Low Latency | Maximum Throughput |
|---|---:|---:|---:|---:|
| small_get | 0.310 / 0.449 | 0.307 / 0.565 | 0.206 / 0.354 | 0.209 / 0.355 |
| small_post | 41.036 / 42.042 | 0.194 / 0.325 | 0.173 / 0.256 | 0.166 / 0.255 |
| interactive_connect | 41.443 / 42.732 | 41.577 / 42.326 | 2.559 / 2.904 | 2.565 / 2.868 |
| concurrent_post | 41.957 / 42.786 | 1.061 / 1.379 | 0.674 / 0.973 | 1.149 / 1.508 |

Bulk cells are median trial throughput in MiB/s, **loopback only**.

| Workload | Checkpoint | Balanced | Low Latency | Maximum Throughput |
|---|---:|---:|---:|---:|
| download | 986.6 | 1061.6 | 799.2 | 1399.2 |
| upload | 218.5 | 163.8 | 712.6 | 1644.7 |

The default small-POST improvement is repeatable and large. Balanced does not
improve bulk throughput: this final run shows slower upload, while the previous
clean comparison's upload confidence interval included parity. This is an
unresolved bulk-performance tradeoff, so Balanced is not presented as a
universal speed upgrade. Low Latency also has a measured bulk-download tradeoff.
Maximum Throughput increased bulk speed in this local workload at the cost of
larger buffers. Network throughput and other machines can behave differently.
Bootstrap median-ratio 95% intervals (4,000 resamples, seed 20261023):
Balanced download 0.96–1.20, upload 0.73–1.38; Low Latency download
0.72–0.92, upload 2.66–4.99; Maximum Throughput download 1.21–1.63,
upload 6.31–10.67. These summarize sampled local trials, not independent WAN
sessions or guarantees.

The initial audit and final interleaved baseline are separate runs; use this
same-run comparison for attribution, not differences between audit tables.

## Modes and scope

| Mode | Changes | Memory / limitation |
|---|---|---|
| Balanced (default) | Coalesced buffered HTTP writes; standard Nagle behavior; 8 KiB each direction | Conservative default; no automatic MUX/DNS/TLS edits |
| Low Latency | TCP_NODELAY on both local relay sockets; 8 KiB each direction | Controlled interactive improvement; bulk downloads may slow |
| Maximum Throughput | TCP_NODELAY; 64 KiB each direction | Up to 128 KiB relay buffers per connection versus 16 KiB; can use more memory |

These controls apply to a new local HTTP/SOCKS proxy session. They are disabled
for TUN because Linux BadVPN and Windows Xray/Wintun bypass this Rust relay.
The local relay remains TCP-only; existing full-TUN UDP forwarding is unchanged.
The benchmark's resident memory stayed around 16 MiB; it includes its synthetic
server/client and is not an application idle-memory or battery measurement.
No TUN or gaming speed improvement is claimed, and TCP head-of-line blocking
cannot be removed from a VMess/WebSocket/TCP transport by these client changes.

## Existing connection and resource observations

The baseline used nine small HTTPS requests through the existing active core
(all succeeded). The final opt-in live smoke wrapped the existing SOCKS listener
with each optimized relay mode; all three HTTPS requests passed with certificate
verification enabled for the probe and positive relay byte counters. There was
no separate account login, core restart, route change or new device TUN. Response
bodies, IPs and real credentials were not retained in published artifacts.
Live timing differences are not attributed to the code: provider/network state
and the underlying active session can change between samples.

Five small UDP DNS queries to the existing resolver returned four responses and
one timeout. This small sample does not estimate gaming packet loss and does not
justify silently changing DNS. Baseline idle sampling was about 0.33% of one CPU
and 44–46 MiB RSS each for GUI/core over three seconds. Recovery time and sustained session stability were not remeasured. No post-update idle,
long-session, suspend/resume, Wi-Fi roaming, remote upload/download, physical
Windows networking, path-MTU or IPv6 leak-proof testing has been claimed.

## Validation and delivery

- 139 Rust tests passed; five existing opt-in integration cases remain ignored.
- Clippy and formatting passed; existing unrelated warnings remain.
- 21 Linux helper tests passed; no helper networking changes were necessary.
- Offline Slint six-page rendering, first-click menus, separate server rows,
  keyboard navigation and small-window scrolling passed; settings controls
  rendered without redesigning the UI.
- Controlled address failover and proxy-stop/counter cancellation regressions pass.
- Generated core transport/security documents are identical across modes.
- Linux glibc 2.30 release build and development AppImage extraction/CLI smoke passed.
- GitHub CI passed Linux tests/DEB/AppImage, Windows tests/x64 installer, Arch
  bin/offline packages and Android gates. Publication was intentionally skipped.
- Final benchmark validates complete response payloads; zero request failures.
- The active installation was not replaced or restarted.

CI: https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/actions/runs/40005336492

## Cloudflare WARP request

A VMess link provides tunnel access, not administrator access to the provider's
server; the app cannot install WARP there. Xray documents a separate client-side
WireGuard chain, but it needs a registered WARP profile and a compatible core.
The retained Linux 1.6.1 core rejects a WireGuard outbound with `unknown config
id: wireguard` during configuration validation. This task explicitly forbids
replacing Xray, so no core replacement, WARP signup, server change, fake toggle
or unverified WARP-success status was introduced.

Primary references: [Xray client-side chain](https://xtls.github.io/en/document/level-2/warp.html),
[Cloudflare WARP modes](https://developers.cloudflare.com/warp-client/warp-modes/).

## Reproduction and development artifact

Run `python3 scripts/benchmark-proxy.py` from a complete Git checkout.
Raw response-validated measurements accompany the development AppImage as
`comparison.json`. The optional `cargo run --release --locked --example
perf_live_smoke -- <existing-loopback-SOCKS-port>` sends three small live HTTPS
requests; it must be invoked explicitly and never starts a second core.
The AppImage is a development artifact, not a newly published GitHub release.
Close the existing application before trying it; no profile migration is required.
