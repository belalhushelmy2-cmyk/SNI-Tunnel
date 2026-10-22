//! Network diagnostics used both by the CLI stage and, later, the GUI.
//! Every function performs a real network operation.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

pub struct DnsReport {
    pub host: String,
    pub addrs: Vec<IpAddr>,
    pub ms: u128,
}

pub async fn resolve(host: &str, port: u16) -> Result<DnsReport> {
    let t = Instant::now();
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("DNS resolution of {host} failed"))?
        .map(|s| s.ip())
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        anyhow::bail!("DNS returned no addresses for {host}");
    }
    Ok(DnsReport {
        host: host.to_string(),
        addrs,
        ms: t.elapsed().as_millis(),
    })
}

pub async fn tcp_connect(host: &str, port: u16, timeout: Duration) -> Result<(SocketAddr, u128)> {
    let t = Instant::now();
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host, port)))
        .await
        .with_context(|| format!("TCP connect to {host}:{port} timed out"))
        .context("TCP connect failed")??;
    Ok((stream.peer_addr()?, t.elapsed().as_millis()))
}

/// Actionable hint for a TLS failure.
///
/// The most common case is a fronted server whose certificate does not cover
/// the SNI the user configured (for example a CDN host). Certificate
/// verification stays on; the user decides whether to trust the setup.
fn tls_hint(err_text: &str, sni: &str) -> String {
    let text = err_text.to_ascii_lowercase();
    if text.contains("cn name does not match")
        || text.contains("hostname mismatch")
        || text.contains("cert_e_cn_no_match")
        || text.contains("name mismatch")
    {
        return format!(
            " — the server certificate does not cover '{sni}'. \
             Use the SNI the certificate was issued for, or enable \
             'Allow insecure' in the server editor if you trust this setup."
        );
    }
    if text.contains("expired") || text.contains("not yet valid") {
        return " — the server certificate is expired or not yet valid.".to_string();
    }
    if text.contains("untrusted") || text.contains("unknown ca") {
        return " — the certificate was not issued by a trusted authority.".to_string();
    }
    String::new()
}

#[cfg(test)]
mod tls_tests {
    use super::*;

    #[test]
    fn cn_mismatch_gets_a_hint() {
        let h = tls_hint(
            "The certificate's CN name does not match the passed value.",
            "ea.com",
        );
        assert!(h.contains("does not cover 'ea.com'"), "{h}");
        assert!(h.contains("Allow insecure"), "{h}");
    }

    #[test]
    fn other_errors_get_no_hint() {
        assert!(tls_hint("connection reset", "ea.com").is_empty());
    }
}

pub struct TlsReport {
    pub sni: String,
    pub cert_sha256: String,
    pub ms: u128,
}

/// Probe the TLS endpoint and return the leaf certificate's SHA-256.
///
/// Windows uses Schannel (native-tls); other platforms use rustls with the
/// system trust store, so no OpenSSL is needed.
#[cfg(windows)]
pub async fn tls_probe(
    host: &str,
    port: u16,
    sni: &str,
    allow_insecure: bool,
    timeout: Duration,
) -> Result<TlsReport> {
    tls_probe_alpn(host, port, sni, allow_insecure, timeout, &[]).await
}

#[cfg(windows)]
async fn tls_probe_alpn(
    host: &str,
    port: u16,
    sni: &str,
    allow_insecure: bool,
    timeout: Duration,
    alpn: &[&str],
) -> Result<TlsReport> {
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host, port)))
        .await
        .context("TLS: TCP connect timed out")?
        .context("TLS: TCP connect failed")?;
    let connector = native_tls::TlsConnector::builder()
        .request_alpns(alpn)
        .danger_accept_invalid_certs(allow_insecure)
        .danger_accept_invalid_hostnames(allow_insecure)
        .build()
        .context("build TLS connector")?;
    let connector = tokio_native_tls::TlsConnector::from(connector);
    let name = if sni.trim().is_empty() { host } else { sni };
    let t = Instant::now();
    let tls = tokio::time::timeout(timeout, connector.connect(name, stream))
        .await
        .context("TLS handshake timed out")?;
    let tls = match tls {
        Ok(s) => s,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "TLS handshake failed for SNI '{name}': {e}{}",
                tls_hint(&e.to_string(), name)
            ))
        }
    };
    let der = tls
        .get_ref()
        .peer_certificate()
        .ok()
        .flatten()
        .and_then(|c| c.to_der().ok())
        .context("TLS peer did not provide a readable certificate")?;
    Ok(TlsReport {
        sni: name.to_string(),
        cert_sha256: hex::encode(Sha256::digest(&der)),
        ms: t.elapsed().as_millis(),
    })
}

/// TLS probe for platforms without Schannel (rustls + system trust store).
///
/// The certificate name is always verified. A server whose certificate does
/// not cover the configured SNI therefore fails here - on those platforms the
/// certificate has to be pinned manually in the server editor.
#[cfg(not(windows))]
pub async fn tls_probe(
    host: &str,
    port: u16,
    sni: &str,
    allow_insecure: bool,
    timeout: Duration,
) -> Result<TlsReport> {
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, RootCertStore};

    let name = if sni.trim().is_empty() { host } else { sni };
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host, port)))
        .await
        .context("TLS: TCP connect timed out")?
        .context("TLS: TCP connect failed")?;

    let mut roots = RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for cert in loaded.certs {
        let _ = roots.add(cert);
    }
    if !loaded.errors.is_empty() {
        log::warn!(
            "{} error(s) while loading the system trust store",
            loaded.errors.len()
        );
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));

    let server_name = ServerName::try_from(name.to_string())
        .map_err(|_| anyhow::anyhow!("'{name}' is not a valid server name"))?;
    let t = Instant::now();
    let tls = tokio::time::timeout(timeout, connector.connect(server_name, stream))
        .await
        .context("TLS handshake timed out")?;
    let tls = match tls {
        Ok(s) => s,
        Err(e) => {
            let text = e.to_string();
            return Err(anyhow::anyhow!(
                "TLS handshake failed for SNI '{name}': {text}{}{}",
                tls_hint(&text, name),
                if allow_insecure {
                    " — pin the certificate in the server editor to trust it"
                } else {
                    ""
                }
            ));
        }
    };
    let der = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|c| c.first())
        .map(|c| c.as_ref())
        .unwrap_or_default();
    Ok(TlsReport {
        sni: name.to_string(),
        cert_sha256: hex::encode(Sha256::digest(der)),
        ms: t.elapsed().as_millis(),
    })
}

pub struct HttpReport {
    pub status: u16,
    pub body: String,
    pub ms: u128,
}

fn http_client(proxy_port: Option<u16>, timeout: Duration) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("SniTunnel/0.1");
    if let Some(p) = proxy_port {
        b = b.proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{p}"))?);
    }
    Ok(b.build()?)
}

async fn get(client: &reqwest::Client, url: &str) -> Result<HttpReport> {
    let t = Instant::now();
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url} failed"))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    Ok(HttpReport {
        status,
        body: body.trim().to_string(),
        ms: t.elapsed().as_millis(),
    })
}

/// Perform an HTTP(S) request *through* the local proxy — this is what proves
/// traffic is actually being forwarded through the tunnel.
pub async fn proxied_get(proxy_port: u16, url: &str, timeout: Duration) -> Result<HttpReport> {
    let client = http_client(Some(proxy_port), timeout)?;
    get(&client, url).await
}

/// Direct (no proxy) request, used as a baseline for comparison.
#[allow(dead_code)]
pub async fn direct_get(url: &str, timeout: Duration) -> Result<HttpReport> {
    let client = http_client(None, timeout)?;
    get(&client, url).await
}

pub async fn external_ip(proxy_port: u16, timeout: Duration) -> Result<String> {
    let r = proxied_get(proxy_port, "https://api.ipify.org", timeout).await?;
    if r.status != 200 {
        anyhow::bail!("ip service returned HTTP {}", r.status);
    }
    let ip = r.body.trim();
    if ip.is_empty() {
        anyhow::bail!("ip service returned an empty body");
    }
    Ok(ip.to_string())
}

/// Preserve the explicitly selected legacy insecure behavior on Xray 26:
/// fetch the presented certificate without authentication and pin it for this
/// session. This is NOT a verified server identity or a persistent trust pin.
/// Strict profiles and explicitly provisioned pins never use this fallback.
async fn retry_certificate_probe<F, Fut>(mut probe: F) -> Result<TlsReport>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<TlsReport>>,
{
    for attempt in 1..=3 {
        match probe().await {
            Ok(report) => return Ok(report),
            Err(error) if attempt == 3 => return Err(error),
            Err(error) => {
                log::warn!("Certificate discovery attempt {attempt}/3 failed; retrying: {error:#}");
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }
    unreachable!()
}

pub async fn ensure_cert_pin(ep: &mut crate::model::Endpoint) -> Result<()> {
    if !ep.tls || !ep.allow_insecure || !ep.pinned_cert_sha256.trim().is_empty() {
        return Ok(());
    }
    log::warn!("Allow insecure enabled: accepting the certificate presented by the server for this session; server identity is not authenticated");
    let report = retry_certificate_probe(|| async {
        #[cfg(windows)]
        let probe = async {
            // Xray's WebSocket dialer offers HTTP/1.1. A probe without ALPN
            // can be rejected before any certificate is sent by that endpoint.
            let alpn: Vec<&str> = if ep.network == "ws" {
                vec!["http/1.1"]
            } else {
                ep.alpn.iter().map(String::as_str).collect()
            };
            tls_probe_alpn(
                &ep.address,
                ep.port,
                &ep.sni,
                true,
                Duration::from_secs(5),
                &alpn,
            )
            .await
        };
        #[cfg(not(windows))]
        let probe = tls_probe(&ep.address, ep.port, &ep.sni, true, Duration::from_secs(5));
        tokio::time::timeout(Duration::from_secs(5), probe)
            .await
            .context("certificate discovery timed out")?
    })
    .await
    .context("Allow insecure: could not obtain the server certificate after 3 attempts")?;
    ep.pinned_cert_sha256 = report.cert_sha256;
    Ok(())
}

/// The public IP seen by a request that goes through the **local proxy**
/// explicitly (the SOCKS5/HTTP listener of the app).
///
/// Comparing it with the exit of an *ordinary* request shows whether the
/// system-wide traffic really uses the tunnel: both must be the same.
pub async fn public_ip_via_proxy(proxy_port: u16, timeout: Duration) -> Result<String> {
    let proxy = format!("http://127.0.0.1:{proxy_port}");
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(proxy).context("build the proxy")?)
        .timeout(timeout)
        .user_agent("SniTunnel/0.1")
        .build()
        .context("build http client")?;
    let resp = client
        .get("https://api.ipify.org")
        .send()
        .await
        .context("the proxied public IP lookup failed")?;
    if !resp.status().is_success() {
        bail!(
            "the proxied public IP lookup returned HTTP {}",
            resp.status()
        );
    }
    let resp = resp.error_for_status()?;
    let ip = resp.text().await.unwrap_or_default().trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        bail!("the proxied public IP lookup returned an empty body");
    }
    Ok(ip)
}

/// The public IP of an **ordinary** system request - no proxy, no source
/// address binding.
///
/// This is what proves that the whole machine (not just our own client) goes
/// through the tunnel: with the TUN routes installed this request leaves
/// through the tunnel adapter and reports the server's exit address.
pub async fn public_ip(timeout: Duration) -> Result<String> {
    // The first request after the tunnel came up can still race with the
    // route installation, so a transient failure is retried.
    let mut last: Option<anyhow::Error> = None;
    for _ in 0..3 {
        match public_ip_once(timeout).await {
            Ok(ip) => return Ok(ip),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("the public IP lookup failed")))
}

async fn public_ip_once(timeout: Duration) -> Result<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .user_agent("SniTunnel/0.1")
        .build()
        .context("build http client")?;
    let resp = client
        .get("https://api.ipify.org")
        .send()
        .await
        .context("public IP lookup failed")?;
    if !resp.status().is_success() {
        bail!("public IP lookup returned HTTP {}", resp.status());
    }
    let resp = resp.error_for_status()?;
    let ip = resp.text().await.unwrap_or_default().trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        bail!("public IP lookup returned an empty body");
    }
    Ok(ip)
}

/// Resolve a name with the *system* resolver (used to check that DNS goes
/// through the tunnel).
pub async fn system_dns_lookup(host: &str) -> Result<Vec<String>> {
    let addrs = tokio::net::lookup_host((host, 443))
        .await
        .with_context(|| format!("DNS resolution of {host} failed"))?
        .map(|s| s.ip().to_string())
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        bail!("DNS returned no addresses for {host}");
    }
    Ok(addrs)
}

/// The public IP over IPv4 only (an ordinary request).
pub async fn public_ip_v4(timeout: Duration) -> Result<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .user_agent("SniTunnel/0.1")
        .build()
        .context("build http client")?;
    let resp = client
        .get("https://api.ipify.org")
        .send()
        .await
        .context("IPv4 public IP lookup failed")?;
    let resp = resp.error_for_status()?;
    let ip = resp.text().await.unwrap_or_default().trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        bail!("IPv4 public IP lookup returned an empty body");
    }
    Ok(ip)
}

/// The public IP over IPv6 only.
pub async fn public_ip_v6(timeout: Duration) -> Result<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .user_agent("SniTunnel/0.1")
        // Force IPv6 so the request cannot fall back to IPv4.
        .local_address(std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED))
        .build()
        .context("build http client")?;
    let resp = client
        .get("https://api64.ipify.org")
        .send()
        .await
        .context("IPv6 public IP lookup failed")?;
    let resp = resp.error_for_status()?;
    let ip = resp.text().await.unwrap_or_default().trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        bail!("IPv6 public IP lookup returned an empty body");
    }
    Ok(ip)
}

// ---------------------------------------------------------------------------
// Staged diagnostics
// ---------------------------------------------------------------------------
/// One diagnostic step and its outcome.
#[derive(Debug, Clone)]
pub struct Step {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
    /// This step is the reason the tunnel does not work.
    pub fatal: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Diagnostic {
    pub steps: Vec<Step>,
}

impl Diagnostic {
    /// The first step that explains the failure, if any.
    pub fn first_failure(&self) -> Option<&Step> {
        self.steps.iter().find(|s| s.fatal)
    }

    /// One line summary suitable for a status bar.
    pub fn summary(&self) -> String {
        match self.first_failure() {
            Some(s) => format!("{} failed: {}", s.name, s.detail),
            None => "all diagnostic stages passed".to_string(),
        }
    }

    /// Multi-line report for the log.
    pub fn report(&self) -> String {
        self.steps
            .iter()
            .map(|s| {
                format!(
                    "  [{}] {:<22} {}",
                    if s.ok { "ok" } else { "FAIL" },
                    s.name,
                    s.detail
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Run the full diagnostic ladder for an endpoint:
/// DNS → TCP → TLS (SNI) → VMess forwarding through the local proxy.
///
/// Every stage is reported, so the log shows *where* a tunnel breaks instead
/// of a single generic "no traffic" message.
pub async fn diagnose(
    ep: &crate::model::Endpoint,
    proxy_port: Option<u16>,
    test_url: &str,
    timeout: Duration,
) -> Diagnostic {
    let mut d = Diagnostic::default();
    let mut fatal = false;

    // 1. DNS
    let dns = resolve(&ep.address, ep.port).await;
    match &dns {
        Ok(r) => d.steps.push(Step {
            name: "DNS resolution",
            ok: true,
            detail: format!("{} -> {:?}", r.host, r.addrs),
            fatal: false,
        }),
        Err(e) => {
            fatal = true;
            d.steps.push(Step {
                name: "DNS resolution",
                ok: false,
                detail: format!("{e:#}"),
                fatal: true,
            });
        }
    }

    // 2. TCP
    match tcp_connect(&ep.address, ep.port, timeout).await {
        Ok((a, ms)) => d.steps.push(Step {
            name: "TCP connect",
            ok: true,
            detail: format!("connected to {a} ({ms} ms)"),
            fatal: false,
        }),
        Err(e) => {
            fatal = true;
            d.steps.push(Step {
                name: "TCP connect",
                ok: false,
                detail: format!("{e:#}"),
                fatal: true,
            });
        }
    }

    // 3. TLS (only meaningful with TLS enabled)
    if ep.tls {
        match tls_probe(&ep.address, ep.port, &ep.sni, ep.allow_insecure, timeout).await {
            Ok(r) => d.steps.push(Step {
                name: "TLS handshake",
                ok: true,
                detail: format!(
                    "SNI '{}' accepted, cert sha256 {} ({} ms)",
                    r.sni,
                    &r.cert_sha256[..16.min(r.cert_sha256.len())],
                    r.ms
                ),
                fatal: false,
            }),
            Err(e) => {
                fatal = true;
                d.steps.push(Step {
                    name: "TLS handshake",
                    ok: false,
                    detail: format!("{e:#}"),
                    fatal: true,
                });
            }
        }
    } else {
        d.steps.push(Step {
            name: "TLS handshake",
            ok: true,
            detail: "TLS disabled for this profile".into(),
            fatal: false,
        });
    }

    // 4. VMess forwarding through the local proxy
    match proxy_port {
        Some(port) => match proxied_get(port, test_url, timeout).await {
            Ok(r) if r.status == 200 => d.steps.push(Step {
                name: "VMess forwarding",
                ok: true,
                detail: format!("proxied GET -> HTTP 200 ({} ms)", r.ms),
                fatal: false,
            }),
            Ok(r) => {
                fatal = true;
                d.steps.push(Step {
                    name: "VMess forwarding",
                    ok: false,
                    detail: format!("proxied GET -> HTTP {} (not 200)", r.status),
                    fatal: true,
                });
            }
            Err(e) => {
                fatal = true;
                d.steps.push(Step {
                    name: "VMess forwarding",
                    ok: false,
                    detail: format!("{e:#}"),
                    fatal: true,
                });
            }
        },
        None => d.steps.push(Step {
            name: "VMess forwarding",
            ok: false,
            detail: "no local proxy to test through".into(),
            fatal: true,
        }),
    }

    let _ = fatal;
    d
}

#[cfg(test)]
mod trust_tests {
    #[tokio::test]
    async fn strict_mode_and_explicit_pins_do_not_fetch_unverified_certificates() {
        let mut ep = crate::model::Endpoint {
            tls: true,
            allow_insecure: false,
            address: "invalid.example".into(),
            ..Default::default()
        };
        super::ensure_cert_pin(&mut ep).await.unwrap();
        assert!(ep.pinned_cert_sha256.is_empty());
        ep.allow_insecure = true;
        ep.pinned_cert_sha256 = "ab".repeat(32);
        super::ensure_cert_pin(&mut ep).await.unwrap();
        assert_eq!(ep.pinned_cert_sha256, "ab".repeat(32));
    }

    #[tokio::test]
    async fn insecure_mode_attempts_handshake_and_never_pins_a_failed_handshake() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(socket);
        });
        let mut ep = crate::model::Endpoint {
            tls: true,
            allow_insecure: true,
            address: "127.0.0.1".into(),
            port,
            sni: "ea.com".into(),
            ..Default::default()
        };
        assert!(super::ensure_cert_pin(&mut ep).await.is_err());
        tokio::time::timeout(std::time::Duration::from_secs(2), accepted)
            .await
            .unwrap()
            .unwrap();
        assert!(ep.pinned_cert_sha256.is_empty());
    }
}

#[cfg(test)]
mod certificate_retry_tests {
    use super::*;
    #[tokio::test]
    async fn recovers_after_transient_eof_and_stops_after_success() {
        let mut calls = 0;
        let report = retry_certificate_probe(|| {
            calls += 1;
            let current = calls;
            async move {
                if current < 2 {
                    anyhow::bail!("unexpected EOF");
                }
                Ok(TlsReport {
                    sni: "test.example".into(),
                    cert_sha256: "verified-result".into(),
                    ms: 1,
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(report.cert_sha256, "verified-result");
    }
    #[tokio::test]
    async fn permanent_failure_is_bounded_and_returns_last_error() {
        let mut calls = 0;
        let result = retry_certificate_probe(|| {
            calls += 1;
            let current = calls;
            async move { anyhow::bail!("failure {current}") }
        })
        .await;
        assert_eq!(calls, 3);
        assert_eq!(result.err().unwrap().to_string(), "failure 3");
    }
}
