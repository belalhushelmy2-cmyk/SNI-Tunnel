use sni_tunnel::{
    model::{resolve_endpoint, ServerProfile, Settings},
    performance::PerformanceMode,
    xray,
};
#[test]
fn performance_modes_never_change_the_server_transport_or_security_config() {
    let server = ServerProfile {
        address: "example.invalid".into(),
        port: 443,
        uuid: "00000000-0000-4000-8000-000000000001".into(),
        sni: "ea.com".into(),
        ws_host: "host.example.invalid".into(),
        ws_path: "/path".into(),
        network: "ws".into(),
        tls: true,
        mux_enabled: true,
        mux_concurrency: 4,
        ..Default::default()
    };
    let ep = resolve_endpoint(&server, None);
    let mut settings = Settings::default();
    let baseline = xray::build_config(&ep, &settings, 12345);
    for mode in [
        PerformanceMode::Balanced,
        PerformanceMode::LowLatency,
        PerformanceMode::MaximumThroughput,
    ] {
        settings.performance_mode = mode;
        assert_eq!(xray::build_config(&ep, &settings, 12345), baseline);
        let stored = serde_json::to_vec(&(server.clone(), settings.clone())).unwrap();
        let (restored, preferences): (ServerProfile, Settings) =
            serde_json::from_slice(&stored).unwrap();
        assert_eq!(restored, server);
        assert_eq!(preferences.performance_mode, mode);
    }
}
