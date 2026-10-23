//! Local HTTP + SOCKS5 proxy listeners.
//!
//! Applications are pointed at these listeners (system-proxy mode) or routed
//! to them by the TUN device. Every connection is chained into Xray's private
//! loopback SOCKS inbound, which performs the actual VMess/TLS transport.
//!
//! Because the relay lives here, the byte counters are real measurements of
//! user traffic, not estimates.

use crate::logging::Logger;
use crate::performance::PerformanceMode;
use crate::stats::Traffic;
use anyhow::{bail, Context, Result};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskCtx, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Http,
    Socks,
}

pub struct LocalProxy {
    handles: Vec<JoinHandle<()>>,
    #[allow(dead_code)]
    pub http_port: u16,
    #[allow(dead_code)]
    pub socks_port: u16,
}

impl LocalProxy {
    pub async fn start(
        http_port: u16,
        socks_port: u16,
        upstream: SocketAddr,
        traffic: Arc<Traffic>,
        logger: Logger,
    ) -> Result<Self> {
        Self::start_with_mode(
            http_port,
            socks_port,
            upstream,
            traffic,
            logger,
            PerformanceMode::Balanced,
        )
        .await
    }

    pub async fn start_with_mode(
        http_port: u16,
        socks_port: u16,
        upstream: SocketAddr,
        traffic: Arc<Traffic>,
        logger: Logger,
        mode: PerformanceMode,
    ) -> Result<Self> {
        let http = TcpListener::bind(("127.0.0.1", http_port))
            .await
            .with_context(|| format!("bind HTTP proxy on 127.0.0.1:{http_port}"))?;
        let socks = TcpListener::bind(("127.0.0.1", socks_port))
            .await
            .with_context(|| format!("bind SOCKS5 proxy on 127.0.0.1:{socks_port}"))?;

        let mut handles = vec![spawn_accept(
            http,
            Kind::Http,
            upstream,
            traffic.clone(),
            logger.clone(),
            mode,
        )];
        handles.push(spawn_accept(
            socks,
            Kind::Socks,
            upstream,
            traffic.clone(),
            logger.clone(),
            mode,
        ));

        logger.info(format!(
            "Local proxy listening: HTTP 127.0.0.1:{http_port}, SOCKS5 127.0.0.1:{socks_port}"
        ));

        Ok(Self {
            handles,
            http_port,
            socks_port,
        })
    }

    pub fn stop(&mut self) {
        for h in self.handles.drain(..) {
            h.abort();
        }
    }
}

fn spawn_accept(
    listener: TcpListener,
    kind: Kind,
    upstream: SocketAddr,
    traffic: Arc<Traffic>,
    logger: Logger,
    mode: PerformanceMode,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Dropping the accept task aborts every accepted connection, including
        // peers stalled in a handshake. Completed tasks are reaped continuously.
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, _peer)) => {
                        if let Err(error) = stream.set_nodelay(mode.nodelay()) {
                            logger.warn(format!("local proxy socket: {error}"));continue;
                        }
                        let t = traffic.clone();let l = logger.clone();
                        connections.spawn(async move {
                            let result = match kind {
                                Kind::Http => handle_http(stream, upstream, t, l.clone(), mode).await,
                                Kind::Socks => handle_socks(stream, upstream, t, mode).await,
                            };
                            if let Err(error) = result {l.log("DEBUG", format!("local proxy: {error}"));}
                        });
                    }
                    Err(error) => {
                        logger.warn(format!("local proxy accept error: {error}"));
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                },
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Byte-counting stream wrapper
// ---------------------------------------------------------------------------

struct Counted {
    inner: TcpStream,
    traffic: Arc<Traffic>,
}

impl Counted {
    fn new(inner: TcpStream, traffic: Arc<Traffic>) -> Self {
        Self { inner, traffic }
    }
}

impl AsyncRead for Counted {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskCtx<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let me = self.get_mut();
        let before = buf.filled().len();
        let r = Pin::new(&mut me.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &r {
            let n = buf.filled().len().saturating_sub(before);
            if n > 0 {
                me.traffic.add_up(n as u64);
            }
        }
        r
    }
}

impl AsyncWrite for Counted {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskCtx<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let me = self.get_mut();
        let r = Pin::new(&mut me.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &r {
            me.traffic.add_down(*n as u64);
        }
        r
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskCtx<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskCtx<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

async fn relay(
    client: TcpStream,
    mut upstream: TcpStream,
    traffic: Arc<Traffic>,
    mode: PerformanceMode,
) {
    traffic.conn_opened();
    struct ConnectionGuard(Arc<Traffic>);
    impl Drop for ConnectionGuard {
        fn drop(&mut self) {
            self.0.conn_closed();
        }
    }
    let _connection = ConnectionGuard(traffic.clone());
    let mut counted = Counted::new(client, traffic.clone());
    let _ = tokio::io::copy_bidirectional_with_sizes(
        &mut counted,
        &mut upstream,
        mode.buffer_size(),
        mode.buffer_size(),
    )
    .await;
}

// ---------------------------------------------------------------------------
// SOCKS5 -> upstream SOCKS5 chaining
// ---------------------------------------------------------------------------

async fn socks5_connect(
    upstream: SocketAddr,
    host: &str,
    port: u16,
    mode: PerformanceMode,
) -> Result<TcpStream> {
    let mut s = TcpStream::connect(upstream)
        .await
        .with_context(|| format!("connect to local core at {upstream}"))?;
    s.set_nodelay(mode.nodelay())?;

    s.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut resp = [0u8; 2];
    s.read_exact(&mut resp).await?;
    if resp[0] != 0x05 || resp[1] != 0x00 {
        bail!("local core rejected SOCKS auth (method {:#04x})", resp[1]);
    }

    let mut req = vec![0x05u8, 0x01, 0x00];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            req.push(0x01);
            req.extend_from_slice(&v4.octets());
        }
        Ok(IpAddr::V6(v6)) => {
            req.push(0x04);
            req.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            let b = host.as_bytes();
            if b.len() > 255 {
                bail!("hostname too long");
            }
            req.push(0x03);
            req.push(b.len() as u8);
            req.extend_from_slice(b);
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0x00 {
        bail!("local core refused connection (reply code {})", head[1]);
    }
    match head[3] {
        0x01 => {
            let mut a = [0u8; 4];
            s.read_exact(&mut a).await?;
        }
        0x04 => {
            let mut a = [0u8; 16];
            s.read_exact(&mut a).await?;
        }
        0x03 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            let mut d = vec![0u8; l[0] as usize];
            s.read_exact(&mut d).await?;
        }
        _ => bail!("local core sent unknown address type"),
    }
    let mut p = [0u8; 2];
    s.read_exact(&mut p).await?;
    Ok(s)
}

// ---------------------------------------------------------------------------
// SOCKS5 server
// ---------------------------------------------------------------------------

async fn handle_socks(
    mut c: TcpStream,
    upstream: SocketAddr,
    traffic: Arc<Traffic>,
    mode: PerformanceMode,
) -> Result<()> {
    let mut ver = [0u8; 1];
    c.read_exact(&mut ver).await?;
    if ver[0] != 0x05 {
        bail!("not a SOCKS5 client (version {})", ver[0]);
    }
    let mut nm = [0u8; 1];
    c.read_exact(&mut nm).await?;
    let mut methods = vec![0u8; nm[0] as usize];
    c.read_exact(&mut methods).await?;
    c.write_all(&[0x05, 0x00]).await?;

    let mut req = [0u8; 4];
    c.read_exact(&mut req).await?;
    if req[1] != 0x01 {
        // Only CONNECT is supported.
        c.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        bail!("unsupported SOCKS command {:#04x}", req[1]);
    }
    let host = match req[3] {
        0x01 => {
            let mut a = [0u8; 4];
            c.read_exact(&mut a).await?;
            IpAddr::from(a).to_string()
        }
        0x04 => {
            let mut a = [0u8; 16];
            c.read_exact(&mut a).await?;
            IpAddr::from(a).to_string()
        }
        0x03 => {
            let mut l = [0u8; 1];
            c.read_exact(&mut l).await?;
            let mut d = vec![0u8; l[0] as usize];
            c.read_exact(&mut d).await?;
            String::from_utf8_lossy(&d).into_owned()
        }
        _ => {
            c.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            bail!("unsupported SOCKS address type");
        }
    };
    let mut p = [0u8; 2];
    c.read_exact(&mut p).await?;
    let port = u16::from_be_bytes(p);

    match socks5_connect(upstream, &host, port, mode).await {
        Ok(up) => {
            c.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            relay(c, up, traffic, mode).await;
            Ok(())
        }
        Err(e) => {
            c.write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP proxy server
// ---------------------------------------------------------------------------

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn split_host_port(authority: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: [::1]:8080
        if let Some(end) = rest.find(']') {
            let host = rest[..end].to_string();
            let port = rest[end + 1..]
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_port);
            return (host, port);
        }
    }
    match authority.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse().unwrap_or(default_port))
        }
        _ => (authority.to_string(), default_port),
    }
}

async fn handle_http(
    mut c: TcpStream,
    upstream: SocketAddr,
    traffic: Arc<Traffic>,
    _logger: Logger,
    mode: PerformanceMode,
) -> Result<()> {
    let mut buf = Vec::with_capacity(2048);
    let mut tmp = [0u8; 1024];
    let header_end;
    loop {
        let n = c.read(&mut tmp).await?;
        if n == 0 {
            bail!("client closed before sending a full request");
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 64 * 1024 {
            bail!("HTTP request header too large");
        }
    }

    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let first_line = head.lines().next().unwrap_or("").to_string();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();

    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = split_host_port(&target, 443);
        let up = socks5_connect(upstream, &host, port, mode).await?;
        c.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        relay(c, up, traffic, mode).await;
        return Ok(());
    }

    let lowered = target.to_ascii_lowercase();
    let (authority, origin_path) =
        if let Some(rest) = lowered.strip_prefix("http://").map(|_| &target[7..]) {
            match rest.find('/') {
                Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
                None => (rest.to_string(), "/".to_string()),
            }
        } else if let Some(rest) = lowered.strip_prefix("https://").map(|_| &target[8..]) {
            match rest.find('/') {
                Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
                None => (rest.to_string(), "/".to_string()),
            }
        } else {
            bail!("HTTP proxy requires an absolute request URI or CONNECT");
        };

    let (host, port) = split_host_port(&authority, 80);
    let mut up = socks5_connect(upstream, &host, port, mode).await?;

    let new_first = format!("{method} {origin_path} {version}");
    let new_head = head.replacen(&first_line, &new_first, 1);
    // Keep an already-buffered body with its rewritten header. Splitting these
    // tiny writes can make the peer wait for a delayed ACK before seeing body.
    let mut first = new_head.into_bytes();
    first.extend_from_slice(&buf[header_end..]);
    up.write_all(&first).await?;
    traffic.add_up(first.len() as u64);
    relay(c, up, traffic, mode).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_port_parsing() {
        assert_eq!(
            split_host_port("example.com:8080", 80),
            ("example.com".into(), 8080)
        );
        assert_eq!(
            split_host_port("example.com", 80),
            ("example.com".into(), 80)
        );
        assert_eq!(split_host_port("[::1]:443", 80), ("::1".into(), 443));
        assert_eq!(
            split_host_port("127.0.0.1:1080", 80),
            ("127.0.0.1".into(), 1080)
        );
    }

    #[test]
    fn finds_header_terminator() {
        assert_eq!(
            find_subslice(b"GET / HTTP/1.1\r\n\r\nX", b"\r\n\r\n"),
            Some(14)
        );
        assert_eq!(find_subslice(b"abc", b"\r\n\r\n"), None);
    }
    #[tokio::test]
    async fn manual_stop_closes_stalled_handshakes() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http = crate::util::free_port();
        let socks = crate::util::free_port();
        let mut proxy = LocalProxy::start(
            http,
            socks,
            upstream.local_addr().unwrap(),
            Arc::new(Traffic::default()),
            Logger::new(),
        )
        .await
        .unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", socks)).await.unwrap();
        client.write_all(&[5, 1, 0]).await.unwrap();
        let mut auth = [0; 2];
        client.read_exact(&mut auth).await.unwrap();
        assert_eq!(auth, [5, 0]);
        proxy.stop();
        let mut byte = [0];
        let read = tokio::time::timeout(std::time::Duration::from_secs(2), client.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(read, Ok(0)) || read.is_err());
    }
    #[tokio::test]
    async fn cancellation_releases_active_connection_accounting() {
        let left = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _client = TcpStream::connect(left.local_addr().unwrap())
            .await
            .unwrap();
        let (client, _) = left.accept().await.unwrap();
        let right = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = TcpStream::connect(right.local_addr().unwrap())
            .await
            .unwrap();
        let _peer = right.accept().await.unwrap();
        let traffic = Arc::new(Traffic::default());
        let expected = traffic.clone();
        let task = tokio::spawn(relay(client, upstream, traffic, PerformanceMode::Balanced));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while expected.snapshot().active_conns == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        assert_eq!(expected.snapshot().active_conns, 0);
        assert_eq!(expected.snapshot().total_conns, 1);
    }
    #[tokio::test]
    async fn buffered_http_body_is_forwarded_and_accounted() {
        let core = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = core.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = core.accept().await.unwrap();
            socket.set_nodelay(true).unwrap();
            let mut auth = [0; 3];
            socket.read_exact(&mut auth).await.unwrap();
            socket.write_all(&[5, 0]).await.unwrap();
            let mut header = [0; 4];
            socket.read_exact(&mut header).await.unwrap();
            assert_eq!(header[3], 3);
            let length = socket.read_u8().await.unwrap() as usize;
            let mut rest = vec![0; length + 2];
            socket.read_exact(&mut rest).await.unwrap();
            socket
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                .await
                .unwrap();
            let expected = b"POST / HTTP/1.1\r\nHost: target\r\nContent-Length: 3\r\n\r\nabc";
            let mut payload = vec![0; expected.len()];
            socket.read_exact(&mut payload).await.unwrap();
            assert_eq!(payload, expected);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            expected.len()
        });
        let traffic = Arc::new(Traffic::default());
        let http = crate::util::free_port();
        let socks = crate::util::free_port();
        let mut proxy = LocalProxy::start(http, socks, address, traffic.clone(), Logger::new())
            .await
            .unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", http)).await.unwrap();
        client
            .write_all(
                b"POST http://target/ HTTP/1.1\r\nHost: target\r\nContent-Length: 3\r\n\r\nabc",
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_to_end(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200"));
        assert_eq!(traffic.snapshot().up, target.await.unwrap() as u64);
        proxy.stop();
    }
}
