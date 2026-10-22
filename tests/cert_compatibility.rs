//! Opt-in live check. Uses isolated loopback listeners and never changes system
//! proxy settings, routes, the saved profile or the shared runtime config.
use sni_tunnel::{
    corebin, localproxy::LocalProxy, logging::Logger, model::resolve_endpoint, netcheck,
    stats::Traffic, store, util, xray,
};
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "uses selected real server; run explicitly with --nocapture"]
async fn selected_server_insecure_compatibility_forwards_https() {
    let cfg = store::load();
    let server = cfg.selected_server_profile().expect("selected profile");
    let mut ep = resolve_endpoint(server, cfg.selected_sni());
    let strict = netcheck::tls_probe(
        &ep.address,
        ep.port,
        &ep.sni,
        false,
        Duration::from_secs(10),
    )
    .await;
    println!(
        "Strict certificate check: {}",
        match strict {
            Ok(_) => "passed".into(),
            Err(e) => util::scrub_sensitive(&format!("{e:#}")),
        }
    );
    ep.allow_insecure = true;
    ep.pinned_cert_sha256.clear();
    netcheck::ensure_cert_pin(&mut ep)
        .await
        .expect("insecure certificate discovery");
    println!(
        "Opt-in insecure certificate discovery succeeded; SNI {} preserved",
        ep.sni
    );
    let logger = Logger::new();
    logger.set_secrets(vec![ep.uuid.clone()]);
    xray::ensure_core_caps(&corebin::xray_path(), &logger);
    let internal = util::free_port();
    let mut settings = cfg.settings.clone();
    settings.http_port = util::free_port();
    settings.socks_port = util::free_port();
    let doc = xray::build_config(&ep, &settings, internal);
    let path = std::env::temp_dir().join(format!("sni-live-check-{}.json", uuid::Uuid::new_v4()));
    xray::write_config(&doc, &path).unwrap();
    let validated = xray::validate_config(&corebin::xray_path(), &path);
    if let Err(e) = validated {
        let _ = std::fs::remove_file(&path);
        panic!("config validation: {e}");
    }
    let started = xray::XrayProcess::start(&corebin::xray_path(), path.clone(), &logger).await;
    let mut core = match started {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            panic!("core start: {e}")
        }
    };
    let result = async {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if tokio::net::TcpStream::connect(("127.0.0.1", internal))
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
        let mut proxy = LocalProxy::start(
            settings.http_port,
            settings.socks_port,
            format!("127.0.0.1:{internal}").parse().unwrap(),
            Arc::new(Traffic::default()),
            logger.clone(),
        )
        .await?;
        let ip = netcheck::public_ip_via_proxy(settings.http_port, Duration::from_secs(15)).await;
        let cloudflare = netcheck::proxied_get(
            settings.http_port,
            "https://www.cloudflare.com/cdn-cgi/trace",
            Duration::from_secs(15),
        )
        .await;
        proxy.stop();
        let ip = ip?;
        let response = cloudflare?;
        anyhow::ensure!(
            response.status == 200,
            "Cloudflare HTTP status {}",
            response.status
        );
        let other = sni_tunnel::tun::parse_trace_ip(&response.body)
            .ok_or_else(|| anyhow::anyhow!("Cloudflare omitted IP"))?;
        println!("Explicit proxy forwarding passed: ipify {ip}; Cloudflare {other}");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    core.stop(&logger).await;
    let _ = std::fs::remove_file(&path);
    result.unwrap();
}
