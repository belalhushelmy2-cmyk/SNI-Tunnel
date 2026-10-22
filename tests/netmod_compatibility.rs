//! Opt-in local NetMod interoperability checks. Never persist selected settings.
use serde_json::json;
use sni_tunnel::{
    compat, corebin, logging::Logger, model::resolve_endpoint, netcheck, store, tun, util, xray,
};
use std::time::Duration;

#[tokio::test]
#[ignore = "requires installed NetMod and selected live VMess profile"]
async fn modern_frontend_to_netmod_backend_forwards_https() {
    let cfg = store::load();
    let ep = resolve_endpoint(cfg.selected_server_profile().unwrap(), cfg.selected_sni());
    let logger = Logger::new();
    logger.set_secrets(vec![ep.uuid.clone()]);
    let mut backend = compat::CompatibilityCore::start(&ep, "", "warning", &logger)
        .await
        .unwrap();
    let port = util::free_port();
    let doc = json!({"log":{"loglevel":"warning"},
        "inbounds":[{"listen":"127.0.0.1","port":port,"protocol":"http","settings":{}}],
        "outbounds":[compat::bridge_outbound(backend.port)]});
    let path = std::env::temp_dir().join(format!("sni-bridge-{}.json", uuid::Uuid::new_v4()));
    xray::write_config(&doc, &path).unwrap();
    let result = async {
        xray::validate_config(&corebin::xray_path(), &path)?;
        let mut frontend =
            xray::XrayProcess::start(&corebin::xray_path(), path.clone(), &logger).await?;
        let result = async {
            tokio::time::sleep(Duration::from_millis(700)).await;
            let ip = netcheck::public_ip_via_proxy(port, Duration::from_secs(15)).await?;
            let cf = netcheck::proxied_get(
                port,
                "https://www.cloudflare.com/cdn-cgi/trace",
                Duration::from_secs(15),
            )
            .await?;
            anyhow::ensure!(
                cf.status == 200 && tun::parse_trace_ip(&cf.body).as_deref() == Some(&ip),
                "forwarding exits differ"
            );
            println!(
                "Modern frontend -> installed NetMod core: both HTTPS probes passed, exit {ip}"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        frontend.stop(&logger).await;
        result
    }
    .await;
    backend.stop(&logger).await;
    let _ = std::fs::remove_file(path);
    result.unwrap();
}

#[tokio::test]
#[ignore = "Administrator: temporary 1.1.1.1/32 route; requires selected live server"]
async fn contained_tun_to_netmod_backend_forwards_tcp_and_udp() {
    let cfg = store::load();
    let ep = resolve_endpoint(cfg.selected_server_profile().unwrap(), cfg.selected_sni());
    let mut settings = cfg.settings.clone();
    settings.netmod_compatibility = true;
    let logger = Logger::new();
    logger.set_secrets(vec![ep.uuid.clone()]);
    let mut session = tun::TunSession::start(
        ep,
        &settings,
        tun::RouteMode::Hosts(vec!["1.1.1.1".into()]),
        true,
        &logger,
    )
    .await
    .unwrap();
    let result=async {
        let reply=tun::verify_through_tun(tun::VERIFY_URL,Duration::from_secs(20)).await?;
        anyhow::ensure!(reply.status==200 && reply.egress_ip.is_some(),"TUN HTTPS probe failed");
        let socket=tokio::net::UdpSocket::bind((tun::active_tun_ip(),0)).await?;
        let query: &[u8]=b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07example\x03com\x00\x00\x01\x00\x01";
        socket.send_to(query,"1.1.1.1:53").await?;
        let mut buffer=[0u8;512];
        let (n,from)=tokio::time::timeout(Duration::from_secs(8),socket.recv_from(&mut buffer)).await??;
        anyhow::ensure!(n>=12 && buffer[..2]==query[..2] && buffer[2]&0x80!=0 && from.ip().to_string()=="1.1.1.1","invalid UDP DNS response");
        println!("Contained TUN -> NetMod core: HTTPS and UDP DNS passed; exit {:?}",reply.egress_ip);
        Ok::<_,anyhow::Error>(())
    }.await;
    session.stop(&logger, false).await;
    result.unwrap();
}

#[tokio::test]
#[ignore = "Administrator: briefly installs full TUN routes and restores them"]
async fn full_tun_netmod_backend_forwards_ordinary_traffic_and_cleans_up() {
    let cfg = store::load();
    let ep = resolve_endpoint(cfg.selected_server_profile().unwrap(), cfg.selected_sni());
    let addresses = netcheck::resolve(&ep.address, ep.port)
        .await
        .unwrap()
        .addrs
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut settings = cfg.settings.clone();
    settings.netmod_compatibility = true;
    let logger = Logger::new();
    logger.set_secrets(vec![ep.uuid.clone()]);
    let mut session = tun::TunSession::start(
        ep,
        &settings,
        tun::RouteMode::DefaultWithExclusions(addresses),
        true,
        &logger,
    )
    .await
    .unwrap();
    let result = async {
        let bound = tun::verify_through_tun(tun::VERIFY_URL, Duration::from_secs(20)).await?;
        let ordinary = netcheck::public_ip(Duration::from_secs(8)).await?;
        anyhow::ensure!(
            bound.status == 200 && bound.egress_ip.as_deref() == Some(&ordinary),
            "ordinary traffic differs from TUN exit"
        );
        anyhow::ensure!(
            session.core_exit_code().is_none(),
            "one of the cores exited"
        );
        let verdict = sni_tunnel::diag::verify_full_tunnel(None, &session.server_addrs, None, None, Duration::from_secs(15)).await;
        anyhow::ensure!(verdict.ok(), "full-tunnel diagnostics: {}", verdict.report());
        println!("Full TAP -> NetMod core: bound and ordinary HTTPS agree on {ordinary}; route/counter diagnostics passed");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    session.stop(&logger, false).await;
    result.unwrap();
}
