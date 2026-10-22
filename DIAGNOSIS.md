# Connection audit — 2026-10-21

## Scope and evidence

This audit extends the existing application; pre-existing uncommitted edits were retained. No credentials, saved SNI, transport, fingerprint, ALPN or MUX choices were changed. No NetMod/Nekoray effective exports were supplied, so no claim of equivalence or client-specific defaults is justified.

The installed core reports Xray 26.3.27, commit d2758a0, Go 1.26.1, windows/amd64. The last runtime document contains VMess over WS, TLS serverName ea.com, MUX enabled, an explicit certificate pin and Wi-Fi socket binding. ALPN/fingerprint are absent, meaning core defaults, not proof that they are absent on the wire. No Xray process was running at the initial inspection. Runtime files can be stale.

## Confirmed defects and changes

- Diagnostics previously fabricated a Wi-Fi binding and ignored effective SNI overrides/settings. They now read the last-written runtime JSON, label it as potentially stale, and only generate a clearly labelled preview when no runtime file exists. Refresh does not fetch certificates or rewrite runtime configuration.
- Comparison previously selected the first outbound (possibly freedom), compared only a small profile subset, and declared equivalence too broadly. The new comparison requires exactly one proxy outbound and compares explicit outbound authentication/security, TLS versions/SNI/fingerprint/ALPN, transports, headers, MUX/XUDP, DNS, ordered routing rules, all outbounds and inbounds. Missing fields remain unspecified. Redacted credentials cannot establish equality. Export the selected connection separately if multiple proxies exist. Reference exports are read-only; not imported into saved profiles.
- Removed speculative NetMod/Nekoray presets. They previously forced MUX and sometimes ALPN without evidence. Existing saved settings are preserved.
- Ordinary public-IP probes inherited proxy discovery. They now explicitly disable proxies; HTTP responses must contain valid IP addresses. Explicit-proxy tests remain explicitly proxied.
- TUN verification disabled HTTPS certificate validation. It now verifies certificates and ignores proxy discovery.
- An insecure profile automatically fetched an unverified certificate and trusted its hash. This does not authenticate the server. Automatic pin acquisition is now rejected: disable Allow insecure or provision a separately verified SHA-256 pin. Existing explicit pins remain supported. Previously auto-generated pins have unknown provenance and should be confirmed with the server operator.
- TUN generation dropped global/per-profile core DNS settings. Those settings are now carried into the TUN core document, with per-profile precedence. This is separate from Windows adapter resolver settings (see limitations).
- VMess JSON imports dropped ALPN arrays and TCP HTTP Host/path. These are now preserved. Existing WS/SNI/fingerprint behavior is retained.
- Dashed UUID redaction had a broken return path. Fixed, retaining undashed support. Structured diagnostic exports also conceal credential fields, arbitrary HTTP header values and URL query/userinfo. Reports exclude raw core logs. Concealed values are labelled unknown in comparisons.
- IPv6 request failure previously claimed leak prevention. It now reports failure/inconclusive. Successful IPv6 also needs route attribution. Unknown route/DNS evidence cannot pass full-tunnel verification.

## Core-version evidence

Primary source snapshots:

- [1.6.1 transport configuration](https://github.com/XTLS/Xray-core/blob/v1.6.1/infra/conf/transport_internet.go)
- [26.3.27 transport configuration](https://github.com/XTLS/Xray-core/blob/v26.3.27/infra/conf/transport_internet.go)
- [1.6.1 VMess account](https://github.com/XTLS/Xray-core/blob/v1.6.1/infra/conf/vmess.go)
- [26.3.27 VMess account](https://github.com/XTLS/Xray-core/blob/v26.3.27/infra/conf/vmess.go)
- [26.3.27 MUX configuration](https://github.com/XTLS/Xray-core/blob/v26.3.27/infra/conf/xray.go)
- [26.3.27 MUX handling](https://github.com/XTLS/Xray-core/blob/v26.3.27/app/proxyman/outbound/handler.go)

26.3.27 accepts a separate WS host field and warns about headers.Host; 1.6.1 used headers. The newer TLS parser rejects allowInsecure after June 1, 2026. This is time-gated behavior in that release, not a generic statement about every Xray 26 build. VMess emits a non-removal deprecation warning; that warning alone is not a forwarding failure or a quota explanation. The old VMess account parsed alterId; the new account no longer does. A nonzero legacy alterId cannot be assumed effective on this core. No automatic downgrade is performed.

26.3.27 parses XUDP concurrency and UDP/443 policy, absent from 1.6.1's configuration structure. Omitted UDP/443 policy defaults to reject. Zero TCP MUX concurrency becomes 8; negative values disable it. The app currently clamps its concurrency to a positive value; advanced negative-concurrency semantics are not exposed. Differences must be measured against actual exports, not inferred from client names. Modern WS and VMess warnings do not justify changing protocols or credentials.

## Routing and platform audit

Windows full tunnel installs two IPv4 /1 routes and two IPv6 /1 routes while retaining the physical default route. VMess server IPv4 addresses get /32 exclusions and the outbound is bound to the discovered physical interface to avoid loops. More-specific LAN, VPN or custom routes can still bypass the /1 routes. IPv6 split routes alone do not establish IPv6 forwarding or fail-closed behavior. The code configures an IPv6 TUN address; older documentation claiming IPv6 is always blocked was inaccurate.

Windows adapter DNS setup still uses 1.1.1.1 and rewrites physical-adapter IPv4 DNS with restoration journals. A core DNS override is not the same as selecting the OS resolver. Bootstrap server-name resolution, IPv6 DNS, other VPN routes, and parallel resolver behavior require capture to establish leakage. The diagnostic report does not equate successful DNS resolution with absence of leaks. UDP DNS probe success establishes only a response to that probe; it does not establish the path without route/capture evidence.

The kill switch retains routes and then stops the core. If the adapter disappears, Windows can fall back to a physical route; this is not a firewall kill switch. Logging and reports no longer promise guaranteed blocking. A proper firewall-backed kill switch remains outstanding and requires lifecycle/recovery testing. Physical-adapter bytes include encrypted VMess traffic, so they are not themselves evidence of a leak.

TUN counters are adapter byte counters, not ISP-accounted data. Proxy mode counts relay bytes and supports HTTP / SOCKS CONNECT, not SOCKS UDP. Accepted-connection logs are not a forwarding test. Linux currently supports the headless proxy; TUN setup is Windows-only. No Linux runtime is installed on this machine.

## Using the diagnostics and comparison

Open the existing application's Diagnostics tab and select Refresh / Test. It makes small ordinary HTTP requests to ipify and Cloudflare, an explicit HTTP-proxy lookup when appropriate, an IPv6 request, and a UDP DNS query. It reads routes and Windows Xray TCP peers; all Xray PIDs are shown, so verify process attribution. Export report writes connection-diagnostics.txt in the application data directory. Comparison uses a local path to an effective Xray JSON export.

CLI: `sni-tunnel diagnose --compare reference.json`. A runtime file is a configuration snapshot, not packet capture or proof of a running core. Comparing arrays preserves order and can show structural differences, not necessarily semantic differences. Header values and credentials are deliberately concealed. For each reference client provide: client/core version, effective running Xray JSON (not only a vmess link), selected outbound, operating mode, DNS and bypass options, and relevant route snapshots. Replace UUID/password/token values with a clear REDACTED marker. A client using sing-box needs a separate schema-aware comparison; the new effective comparison rejects that format instead of treating it as Xray.

## Optional local capture

With Wireshark/Npcap installed, capture only on the physical uplink while making one small request. Filter by the configured server IP and port, then inspect ClientHello server_name, offered ALPN and TLS versions. Correlate the socket with the Xray PID. Compare the offered ClientHello separately from negotiated TLS properties. Encrypted WS headers/path cannot be confirmed by ordinary passive capture; use trusted server logs if available. No TLS key logging/decryption is necessary. Keep raw captures local: they can contain other private traffic. Export only relevant field summaries, never UUIDs, tokens or full application payloads.

## Controlled WE balance comparison

1. Use the same server/account, access network and test device. Pause updates/sync/other client traffic. Record client/core versions, effective config, network mode and time.
2. Record authoritative WE main and gaming balances before the run. Wait for previous usage to settle; measure an idle interval to estimate background usage.
3. Connect one client. Verify explicit-proxy and ordinary-system traffic separately with the same small HTTPS endpoints. Download the same small fixed-size file once, avoiding cache (for example a 1–5 MiB file you control). Record actual bytes and duration; include connection overhead.
4. Disconnect. Wait for WE reporting to stabilize; record multiple readings and elapsed time rather than assuming an immediate deduction. If account granularity exceeds the sample, mark the result inconclusive rather than increasing usage without a limit.
5. Repeat for NetMod, Nekoray and SNI Tunnel, rotating order and retaining the same conditions. Do not run clients concurrently. Compare main/gaming deltas after accounting for idle traffic and reporting delays.

SNI, TLS fingerprint, route selection and public IP cannot establish or control WE billing eligibility. Only authoritative account usage can establish the observed deductions. None of these changes is a billing workaround or a guarantee of gaming-quota treatment.

## Validation

Final build/test results are recorded below. Live tests use certificate verification and report failures honestly. Complete firewall kill-switch enforcement, per-socket leak attribution, reference-client comparisons and Linux runtime verification remain external validation requirements.

### Additional verified default difference

The [1.6.1 WebSocket dialer](https://github.com/XTLS/Xray-core/blob/v1.6.1/transport/internet/websocket/dialer.go) enables uTLS only for an explicitly recognised fingerprint. In [26.3.27 TLS code](https://github.com/XTLS/Xray-core/blob/v26.3.27/transport/internet/tls/tls.go), GetFingerprint("") returns HelloChrome_Auto, and its WebSocket dialer uses that result. Therefore the currently omitted fingerprint can produce a different ClientHello between these versions. Both uTLS WebSocket paths force http/1.1 ALPN for ordinary non-ECH WebSocket handshakes. Preserving configured ALPN is not proof of the offered ALPN. This is evidence of a default difference, not evidence that NetMod left the fingerprint empty or that it changes billing. No fingerprint option was changed.

### Validation observations

- Final full-feature cargo test: 92 unit tests (including GUI logic) plus 7 workflow integration tests passed. The opt-in installed-core test passed separately. Windows GUI release and Linux headless release cross-builds succeeded. Windows release --help smoke test passed; Linux execution was unavailable.
- Installed Xray accepted the current runtime outbound with a temporary SOCKS validation inbound (exit 0). VMess/WebSocket deprecation categories were observed. The temporary credential-bearing file was removed.
- Ordinary ipify and Cloudflare requests returned the same valid IP; a small UDP DNS query received a response. These observations do not establish VMess forwarding because Xray was not running.
- The selected profile requests allow-insecure without an explicit trusted pin. The new check stops before changing routes/configuration. A separate strict certificate-verifying TLS probe ended with SSLEOFError (inconclusive). No verified tunnel forwarding or quota outcome is claimed.

### Final implementation notes

The server editor now exposes the certificate SHA-256 pin, validates its format and preserves it when editing other settings. Obtain this value from a trusted server administrator, not an unverified local handshake. The legacy insecure checkbox explicitly states that a verified pin is required.

Log sinks and exported logs additionally scrub dashed/undashed UUIDs, registered passwords even when short, escaped JSON credentials, URL queries and authorization/cookie suffixes. Free-form logging cannot reliably recognise every arbitrary secret; diagnostic reports therefore use structured configuration redaction and omit raw logs.

An opt-in integration test (`SNI_TUNNEL_TEST_XRAY`, `cargo test --no-default-features --test core_validation -- --ignored`) validates generated proxy and substituted-TUN documents with the actual installed core; it passed. The TUN inbound is replaced for validation so the test does not create adapters or install routes.

Repository-wide `cargo fmt --all -- --check` reports formatting differences, including untouched files. A broad unrelated reformat was avoided; the new diagnostics module and core-validation test were formatted. Clippy completed successfully with warnings (including an existing eight-argument GUI callback function). Linux compilation retains existing unused-import/unreachable-Windows-code warnings.


## 2026-10-21 missing Logs messages

- Replaced lossy GUI broadcast consumption with a revisioned, bounded (2,000 entries), scrubbed history. Startup messages remain available, mode changes refilter history, Clear resets retained entries, and UI stalls no longer silently discard broadcast messages.
- Recognize Xray bracketed severity case-insensitively, including `[Warning]`; preserve failed-line capture for startup diagnostics.
- Replaced the fixed positive auto-scroll offset with a real scroll view, bottom tracking, wrapped text and visible levels. Label UTC timestamps and explain that changing core verbosity requires reconnecting.
- NetMod is running Xray 1.6.1 from its bundled v2ray.exe. Its command line names `%LOCALAPPDATA%\NetMod\v2raycfg.json`, but that file is absent after startup. A bounded capture is awaiting a user reconnect; no actual comparison or gaming-quota fix is claimed.


### Captured NetMod comparison and logging validation

Successfully captured NetMod's transient runtime JSON on reconnect, with credentials removed before writing. The actual outbound matches SNI Tunnel's last-written runtime on server fr1.vpnjantit.com:10001, VMess auto security / alterId 0, TLS SNI ea.com, WebSocket Host ea.com and /vpnjantit. Credentials are concealed and their equality was not verified. Neither client explicitly sets ALPN or fingerprint. NetMod uses Xray 1.6.1; SNI Tunnel uses 26.3.27. Version-specific default TLS behavior remains a concrete difference requiring on-wire comparison, not a proven billing cause.

NetMod specifies mux concurrency 0, but Xray 1.6.1 normalizes this to 8 (https://github.com/XTLS/Xray-core/blob/v1.6.1/infra/conf/xray.go#L102-L120), matching our explicit 8. Do not treat that raw JSON difference as a behavioral mismatch. NetMod requests allowInsecure and allowInsecureCiphers; SNI Tunnel uses a session-discovered certificate pin for the user's opt-in insecure mode. NetMod uses SOCKS/HTTP inbounds plus a separate TAP/tun2socks process; SNI Tunnel's last configuration uses native TUN and Wi-Fi socket binding. No DNS section is present in either core configuration; this does not establish OS DNS equivalence. NetMod's empty routing rules do not establish an application bypass.

Logging validation: all 96 GUI-feature unit tests and 7 workflow tests passed, including regression coverage for retained startup logs, mode switches, redaction, Clear, bounded history, and Xray severities. Release build and --help smoke passed. New artifact: outputs/sni-tunnel-windows-logs.exe. GUI layout compiled; no interactive visual verification is claimed. No network profiles, routes or TLS transport settings were modified during this logging fix. Gaming-quota billing remains unresolved.


## 2026-10-21 connection reliability repair

User asked to fix repeated EOF/HTTP 500/timeout failures and requested overwriting the existing delivered executable, not creating additional build names.

Confirmed client fixes:
- Certificate discovery now makes up to three attempts, each with a five-second total deadline, with 300 ms backoff. Successful discovery stops retries; a failed attempt cannot set a certificate pin. Exhaustion honors the configured auto-reconnect preference in the engine.
- TUN verification now applies the caller's timeout to the entire retry operation. Previously six attempts could each consume the full per-attempt timeout.
- Background health checks every 30 seconds probe actual HTTPS traffic through the live transport. A failed check removes the healthy status; another check follows after five seconds. Two failed checks stop the session, preserve the configured TUN kill-switch behavior and schedule reconnect only when enabled. Disconnect aborts pending checks. This does not upgrade the route-based kill switch into firewall enforcement.
- Removed automatic TUN-to-system-proxy fallback, which silently reduced application coverage when device-wide setup failed.
- Core exit now honors the existing TUN kill-switch setting; full-tunnel state resets on teardown. The GUI recognizes FullForwarding as connected.

Isolated live comparison used private loopback HTTP proxies, temporary configs removed after each child stopped, and no system route/proxy/profile changes. Current TLS default: 6/6 with mux and 12/12 without mux HTTP requests passed, with 3 and 2 logged EOFs respectively. Native TLS experiment: 6/6 and 11/12 passed, with zero logged EOFs; one curl TLS failure remained. Bundled NetMod core/config: 5/6 passed, with 2 EOFs and one HTTP 500. This short, sequential sample does not establish a reliable advantage or billing effect. No TLS fingerprint or stored transport settings were changed. The server/path can fail under both versions; client repairs cannot guarantee removal of remote HTTP 500 responses. Gaming-quota attribution remains unverified.

Regression coverage includes transient certificate recovery, bounded permanent failure, entire-operation TUN timeout, health status invalidation/recovery, reconnect preference and cancellation on disconnect.


### Certificate probe follow-up

The first post-fix live test still failed: all three Schannel certificate discovery attempts ended with EOF. Inspection found that the probe did not offer ALPN while Xray's WebSocket dialer offers HTTP/1.1. Enabled native-tls ALPN support and made Windows certificate discovery offer HTTP/1.1 for WebSocket, or the profile's ALPN for other transports. The actual Xray transport settings remain unchanged. A separate six-handshake Schannel sample succeeded 1/3 without ALPN and 2/3 with HTTP/1.1; this small sample does not prove that ALPN alone cures the remote closures. Final live retest is recorded below.


Final validation after ALPN alignment: 102 GUI-feature unit tests passed; the seven workflow integration tests also passed during this repair. The opt-in live certificate/HTTPS forwarding test passed after the alignment: session certificate acquisition succeeded and ipify/Cloudflare both reported 51.91.248.209 through the explicit local proxy. The earlier failed live attempt remains documented above. Full device-wide TUN routing and sustained server stability were not retested; quota billing is not measured by these tests.


## 2026-10-21 actual TLS comparison and NetMod compatibility experiment

Captured ClientHello messages from the installed binaries into an isolated loopback listener (not a packet capture of WE's network). Used the captured NetMod effective configuration and SNI Tunnel's last runtime configuration, changing only the dial destination to the local capture socket and disabling mux for isolated samples. Credentials were never transmitted because the listener stops after ClientHello. Two samples each showed:
- NetMod's installed Xray 1.6.1: 264-byte handshake; SNI ea.com, ALPN http/1.1, X25519 key share.
- Pinned Xray 26.3.27 with omitted fingerprint: 1740 / 1772-byte handshakes, shuffled Chrome-style extensions and a 1216-byte hybrid key share.
- Pinned Xray 26.3.27 native TLS experiment: 1508-byte handshake; still not equivalent to NetMod's old Go TLS.
These prove a transport mismatch, not the ISP's billing decision. Raw structured summaries are in outputs/tls-handshake-comparison.json.

Added explicit, reversible `netmod_compatibility` setting (off by default for other installations). For this VMess/WebSocket/TLS test, modern pinned Xray still owns TUN, while a managed loopback SOCKS connection forwards TCP/UDP to NetMod's locally installed Xray 1.6.1. No older binary is downloaded and the pinned package is not replaced. The installed backend version must match 1.6.1. The remote server IP is resolved before DNS redirection and is selected from the route exclusions; the remote socket binds the physical uplink address. Credentials, SNI, Host and path are preserved; saved server profiles are unchanged. This tests the actual known-working core instead of guessing a browser fingerprint.

Compatibility is limited to TUN + VMess/WebSocket/TLS. Unsupported transport/XUDP settings and explicit certificate pins fail rather than being silently discarded. Allow insecure is respected using the old core's native option only when already enabled. Both core processes are monitored and managed; temporary credential-bearing backend configs are removed on normal/error cleanup. A crash can leave a session config; diagnostics associate a backend only with an unambiguous matching loopback port, and label the runtime snapshot as potentially stale. Diagnostics show both the TUN frontend and the remote backend, and compare the latter against NetMod exports.

Validation: isolated modern frontend -> old backend HTTPS passed; contained TUN HTTPS plus UDP DNS passed; full-device TUN test passed with the same bound/ordinary exit (51.91.248.209). The full-route test cleaned up: no Xray/v2ray processes, /1 routes, or netmod-session JSON files remained. 106 unit tests passed, including preservation of independent transport fields, pin/unsupported-mode rejection, local-only bridge and diagnostics selection. Workflow test result and final build recorded below. None of these checks measures quota billing.

## NetMod TAP experiment (2026-10-21)

The compatibility switch now selects the installed NetMod TAP-Windows V9 adapter and badvpn-tun2socks executable, with SOCKS5 UDP, 10.4.2.1/24 and peer 10.4.2.2, MTU 1500, and adapter DNS 8.8.8.8 / 8.8.4.4. Physical-adapter DNS is left as NetMod leaves it. Xray 1.6.1 still handles the remote connection; modern native TUN is retained for ordinary mode. IPv6 blocking routes are retained as an intentional difference from NetMod. Requires exactly one existing TAP adapter and NetMod disconnected. No driver is installed.

Adapter IPv4/DNS/MTU state is journalled before changes and restored on stop/setup failure; recovery runs before the next TAP start. Both subprocesses are monitored. Forwarding checks and diagnostics use the TAP source address, gateway and counters. Active-connection counts are unavailable for the legacy forwarding helper. The remote dial address is still resolved before routing and bound to the physical uplink. This is a datapath experiment, not evidence of quota classification. User reported the previous modern-TUN compatibility build still consumed main quota while NetMod consumed gaming quota.

### NetMod TAP experiment validation

Contained TAP route test passed for HTTPS and UDP DNS. Full-device TAP test passed: bound and ordinary HTTPS both exited via 51.91.248.209, and the application's full-tunnel route/counter diagnostics passed. Cleanup inspection found no Xray/v2ray/tun2socks subprocesses, no split /1 routes, and no TAP recovery journal; adapter DHCP enabled, MTU 1500, DNS reset to its original empty/DHCP state. Setup failures during development were restored as well. Final unit/workflow suite and release build results follow. Billing remains unmeasured by the agent.
Final validation: 106 unit tests and 7 workflow integration tests passed; release build and delivered executable --help smoke test passed. The existing sni-tunnel-windows-logs.exe was overwritten and its hash matched the build. Saved compatibility remains enabled.
