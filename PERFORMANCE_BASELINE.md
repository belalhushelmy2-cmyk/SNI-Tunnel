# Performance baseline — 2026-10-23

Local checkpoint: `perf-baseline-20261023` (before networking edits). Public
baseline commit: `ef7a36e344d2be62b48f52c4ced710df3a1eeccb`; its relay
source is byte-for-byte identical to the checkpoint. Existing Rust/
Slint architecture, bundled cores, server profiles and active tunnel retained.
Measurements are bounded; no bulk Internet transfer, route change, reconnect,
MUX change, DNS change or global TCP tuning was performed.

## Reproducible local proxy baseline

`cargo run --release --locked --example perf_probe`. Five trials, 30 sequential
GETs and 30 512-byte POSTs per trial; 16 concurrent POSTs; three 8 MiB uploads
and downloads per trial. All bytes stay on loopback. A synthetic SOCKS core
responds without DNS, TLS or a remote server. This isolates the Rust proxy;
it does not measure VMess/TUN, Internet throughput, gaming RTT or packet loss.

| Workload | Median trial p50 ms | Median trial p95 ms |
|---|---:|---:|
| small_get | 0.396 | 0.628 |
| small_post | 41.071 | 42.011 |
| concurrent_post | 41.977 | 43.193 |

Loopback throughput (median trial):

- download: 1079.9 MiB/s.
- upload: 162.4 MiB/s.

## Existing active tunnel, lightweight sample

No real-profile diagnostics or credentials are included here. Existing active
SOCKS listener used; HTTPS certificates remained verified. Five sequential and
four concurrent small trace requests; response bodies were not retained.
No new real-server account connection or tunnel was started.

| Measurement | p50 ms | p95 ms | Failures |
|---|---:|---:|---:|
| system_dns | 76.52 | 82.57 | 0/5 |
| server_tcp_connect | 150.13 | 208.58 | 0/5 |
| proxied_https_sequential | 209.40 | 222.16 | 0/5 |
| proxied_https_concurrent_4 | 202.61 | 204.51 | 0/4 |

Three-second idle sample (not a long-running resource profile):

- gui: 0.33% of one CPU, RSS 44.0 MiB.
- core: 0.33% of one CPU, RSS 46.2 MiB.

## Measured priorities

1. About 41 ms repeatable local small-POST stall, versus sub-millisecond GET.
   Investigate split writes, Nagle and delayed acknowledgements first.
2. Local upload throughput is substantially below download throughput. Test
   socket latency and bounded relay-buffer alternatives under the same workload.
3. DNS is a material part of measured server-connect time. Inspect sequential
   address failover and avoid a broken address stalling a working address.
   No application DNS cache is justified without TTL and invalidation support.

Live RTT/jitter attribution, upload/download Internet throughput, packet loss,
Wi-Fi/suspend recovery, startup/reconnect and Windows runtime measurements remain
unmeasured. Idle GUI/core samples are already low and do not justify UI rewrites.
Linux TUN bypasses the Rust local proxy; local-proxy gains must not be claimed
as Linux TUN or real-world latency improvements. Existing transport remains
subject to TCP head-of-line blocking. No WARP registration or server installation
has been performed; a VMess link does not authorize changing its provider.
