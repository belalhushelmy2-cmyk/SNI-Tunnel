use sni_tunnel::{
    model::{resolve_endpoint, ServerProfile, Settings},
    tun, xray,
};

#[test]
#[ignore = "requires SNI_TUNNEL_TEST_XRAY pointing to installed Xray 26.3.27"]
fn generated_documents_are_accepted_by_installed_core() {
    let exe =
        std::path::PathBuf::from(std::env::var_os("SNI_TUNNEL_TEST_XRAY").expect("set core path"));
    let profile = ServerProfile {
        address: "server.example".into(),
        port: 443,
        uuid: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
        sni: "ea.com".into(),
        ws_host: "host.example".into(),
        ws_path: "/ws".into(),
        network: "ws".into(),
        tls: true,
        ..Default::default()
    };
    let ep = resolve_endpoint(&profile, None);
    let settings = Settings {
        custom_dns: "1.1.1.1".into(),
        ..Default::default()
    };
    let proxy = xray::build_config(&ep, &settings, 10899);
    assert_eq!(
        proxy["outbounds"][0]["streamSettings"]["tlsSettings"]["serverName"],
        "ea.com"
    );
    let tun = tun::TunSession::validation_config(&tun::tun_config(
        tun::DEVICE_NAME,
        tun::TUN_MTU,
        xray::build_outbound(&ep),
        "warning",
    ));
    let path = std::env::temp_dir().join(format!(
        "sni-config-validation-{}.json",
        uuid::Uuid::new_v4()
    ));
    for config in [proxy, tun] {
        xray::write_config(&config, &path).unwrap();
        let result = xray::validate_config(&exe, &path);
        let _ = std::fs::remove_file(&path);
        result.unwrap();
    }
}
