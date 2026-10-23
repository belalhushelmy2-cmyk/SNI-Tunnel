//! Bounded loopback-only proxy benchmark. No real profiles or external network.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use sni_tunnel::{localproxy::LocalProxy, logging::Logger, stats::Traffic};
use std::{sync::Arc, time::Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};

async fn core(mut socket: TcpStream) -> Result<()> {
    socket.set_nodelay(true)?;
    let mut greeting = [0; 3];
    socket.read_exact(&mut greeting).await?;
    socket.write_all(&[5, 0]).await?;
    let mut request = [0; 4];
    socket.read_exact(&mut request).await?;
    let length = match request[3] {
        1 => 4,
        4 => 16,
        3 => socket.read_u8().await? as usize,
        _ => anyhow::bail!("SOCKS address"),
    };
    let mut rest = vec![0; length + 2];
    socket.read_exact(&mut rest).await?;
    socket.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80]).await?;
    let mut received = Vec::with_capacity(1024);
    let mut block = [0; 8192];
    let header_end = loop {
        let n = socket.read(&mut block).await?;
        if n == 0 {
            anyhow::bail!("HTTP header EOF")
        };
        received.extend_from_slice(&block[..n]);
        if let Some(i) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let text = String::from_utf8_lossy(&received[..header_end]);
    let upload = text
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(0);
    let download = if text.contains("GET /download ") {
        8 * 1024 * 1024
    } else {
        64
    };
    let mut remaining = upload.saturating_sub(received.len() - header_end);
    while remaining > 0 {
        let n = socket.read(&mut block).await?;
        if n == 0 {
            anyhow::bail!("upload EOF")
        };
        remaining = remaining.saturating_sub(n);
    }
    socket
        .write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Length: {download}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let block = [b'x'; 65536];
    let mut left = download;
    while left > 0 {
        let n = left.min(block.len());
        socket.write_all(&block[..n]).await?;
        left -= n;
    }
    socket.shutdown().await?;
    Ok(())
}
async fn request(port: u16, path: &str, upload: usize) -> Result<(f64, usize)> {
    let started = Instant::now();
    let mut socket = TcpStream::connect(("127.0.0.1", port)).await?;
    socket.set_nodelay(true)?;
    let mut first=format!("{} http://loopback.test{path} HTTP/1.1\r\nHost: loopback.test\r\nContent-Length: {upload}\r\nConnection: close\r\n\r\n",if upload>0{"POST"}else{"GET"}).into_bytes();
    if path == "/interactive" {
        socket
            .write_all(b"CONNECT loopback.test:80 HTTP/1.1\r\nHost: loopback.test\r\n\r\n")
            .await?;
        let mut reply = Vec::new();
        while !reply.ends_with(b"\r\n\r\n") {
            reply.push(socket.read_u8().await?);
        }
        anyhow::ensure!(reply.starts_with(b"HTTP/1.1 200"), "CONNECT failed");
        socket.write_all(&first).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        first.clear();
    }
    let initial = upload.min(512);
    first.extend(std::iter::repeat_n(b'p', initial));
    socket.write_all(&first).await?;
    let block = [b'p'; 65536];
    let mut left = upload - initial;
    while left > 0 {
        let n = left.min(block.len());
        socket.write_all(&block[..n]).await?;
        left -= n;
    }
    let mut bytes = 0;
    let mut response = Vec::new();
    let mut buffer = [0; 65536];
    loop {
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        bytes += n;
        if response.len() < 128 {
            response.extend_from_slice(&buffer[..n.min(128 - response.len())]);
        }
    }
    anyhow::ensure!(
        response.starts_with(b"HTTP/1.1 200"),
        "invalid benchmark response"
    );
    let end = response
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .context("Incomplete HTTP response header")?
        + 4;
    let expected = if path == "/download" {
        8 * 1024 * 1024
    } else {
        64
    };
    anyhow::ensure!(
        bytes == end + expected,
        "Incomplete benchmark payload: received {bytes}, expected {}",
        end + expected
    );
    Ok((started.elapsed().as_secs_f64() * 1000., bytes))
}
fn distribution(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    let mean = values.iter().sum::<f64>() / n as f64;
    let jitter = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
    json!({"samples":n,"p50_ms":values[(n-1)/2],"p95_ms":values[((n as f64*0.95).ceil() as usize-1).min(n-1)],"stddev_ms":jitter})
}
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let directory = std::env::temp_dir().join(format!("sni-perf-{}", std::process::id()));
    std::env::set_var("SNI_TUNNEL_DATA_DIR", &directory);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let upstream = listener.local_addr()?;
    let server = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = core(socket).await;
            });
        }
    });
    let http = sni_tunnel::util::free_port();
    let socks = sni_tunnel::util::free_port();
    let mode = match std::env::args().nth(1).as_deref() {
        Some("low_latency") => sni_tunnel::performance::PerformanceMode::LowLatency,
        Some("maximum_throughput") => sni_tunnel::performance::PerformanceMode::MaximumThroughput,
        _ => sni_tunnel::performance::PerformanceMode::Balanced,
    };
    let mut proxy = LocalProxy::start_with_mode(
        http,
        socks,
        upstream,
        Arc::new(Traffic::default()),
        Logger::new(),
        mode,
    )
    .await?;
    for _ in 0..5 {
        request(http, "/small", 0).await?;
    }
    let mut trials = Vec::new();
    for trial in 0..5 {
        let mut row = json!({"trial":trial});
        for (label, path, body) in [
            ("small_get", "/small", 0),
            ("small_post", "/small", 512),
            ("interactive_connect", "/interactive", 512),
        ] {
            let mut values = Vec::new();
            for _ in 0..30 {
                values.push(request(http, path, body).await?.0);
            }
            row[label] = distribution(values);
        }
        let mut tasks = JoinSet::new();
        let start = Instant::now();
        for _ in 0..16 {
            tasks.spawn(request(http, "/small", 512));
        }
        let mut values = Vec::new();
        while let Some(result) = tasks.join_next().await {
            values.push(result??.0);
        }
        row["concurrent_post"] = distribution(values);
        row["concurrent_wall_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
        for (label, path, body) in [
            ("download", "/download", 0),
            ("upload", "/small", 8 * 1024 * 1024),
        ] {
            let start = Instant::now();
            let mut total = 0;
            for _ in 0..3 {
                total += request(http, path, body).await?.1;
            }
            let seconds = start.elapsed().as_secs_f64();
            row[label] = json!({"seconds":seconds,"payload_mib":24,"mib_per_second":24./seconds,"response_bytes":total});
        }
        trials.push(row);
    }
    proxy.stop();
    server.abort();
    let _ = std::fs::remove_dir_all(directory);
    println!(
        "{}",
        json!({"scope":"loopback HTTP proxy + synthetic SOCKS core; no remote traffic or TUN", "mode":format!("{mode:?}"),"trials":trials})
    );
    Ok(())
}
