//! Opt-in smoke check through an EXISTING local Xray SOCKS listener.
//! Three tiny HTTPS responses; no profile, Xray process, TUN, DNS or route edits.
use anyhow::{Context, Result};
use sni_tunnel::{
    localproxy::LocalProxy, logging::Logger, netcheck, performance::PerformanceMode,
    stats::Traffic, util,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
#[tokio::main]
async fn main() -> Result<()> {
    let port:u16=std::env::args().nth(1).context("Explicit existing loopback SOCKS port required; this test uses a small amount of live traffic")?.parse()?;
    anyhow::ensure!(port > 0, "Invalid SOCKS port");
    let directory = std::env::temp_dir().join(format!("sni-live-smoke-{}", std::process::id()));
    std::env::set_var("SNI_TUNNEL_DATA_DIR", &directory);
    let upstream = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut failures = 0;
    for mode in [
        PerformanceMode::Balanced,
        PerformanceMode::LowLatency,
        PerformanceMode::MaximumThroughput,
    ] {
        let http = util::free_port();
        let socks = util::free_port();
        let traffic = Arc::new(Traffic::default());
        let mut proxy = LocalProxy::start_with_mode(
            http,
            socks,
            upstream,
            traffic.clone(),
            Logger::new(),
            mode,
        )
        .await?;
        let response = netcheck::proxied_get(
            http,
            "https://1.1.1.1/cdn-cgi/trace",
            Duration::from_secs(10),
        )
        .await;
        let counters = traffic.snapshot();
        let passed = matches!(&response,Ok(report) if report.status==200 && report.body.lines().any(|line|line.starts_with("ip=")))
            && counters.up > 0
            && counters.down > 0;
        if !passed {
            failures += 1;
        }
        println!(
            "{}",
            serde_json::json!({"mode":format!("{mode:?}"),"forwarding":passed,"https_certificate_verification":true,"elapsed_ms":response.as_ref().ok().map(|r|r.ms)})
        );
        proxy.stop();
    }
    let _ = std::fs::remove_dir_all(directory);
    anyhow::ensure!(failures==0,"{failures} live checks failed; inspect the connection before attributing this to a performance mode");
    Ok(())
}
