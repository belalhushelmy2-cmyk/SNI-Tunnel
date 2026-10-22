//! End-to-end tests for the server management workflow.
//!
//! These are *offline* tests: they exercise import → edit → save → reload →
//! export without touching the network. The live networking checks live in
//! `scripts/smoke-test.ps1` (they need a real server).

use sni_tunnel::editor::{duplicate_profile, EditorFields};
use sni_tunnel::model::{resolve_endpoint, AppConfig, ServerProfile};
use sni_tunnel::vmess;

const UUID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

fn sample() -> ServerProfile {
    ServerProfile {
        id: "id-1".into(),
        name: "Tokyo".into(),
        address: "sp3.example.com".into(),
        port: 10001,
        uuid: UUID.into(),
        protocol: "vmess".into(),
        network: "ws".into(),
        tls: true,
        sni: "ea.com".into(),
        ws_host: "ea.com".into(),
        ws_path: "/vpnjantit".into(),
        alpn: vec!["h2".into()],
        fingerprint: "chrome".into(),
        ..Default::default()
    }
}

#[test]
fn import_edit_save_reload_keeps_everything() {
    // 1. import a vmess:// link
    let link = vmess::to_vmess_link(&sample());
    let mut cfg = AppConfig::default();
    let imported = vmess::parse_any(&link).expect("import");
    assert_eq!(imported.len(), 1);
    cfg.servers.push(imported[0].clone());

    // 2. edit the SNI through the editor fields
    let mut fields = EditorFields::from_profile(&cfg.servers[0]);
    fields.sni = "new-sni.example".into();
    let edited = fields.to_profile(&cfg.servers[0]).expect("validate");
    cfg.servers[0] = edited;

    // 3. the values are what we expect
    assert_eq!(cfg.servers[0].sni, "new-sni.example");
    assert_eq!(cfg.servers[0].address, "sp3.example.com");
    assert_eq!(cfg.servers[0].ws_host, "ea.com");
    assert_eq!(cfg.servers[0].ws_path, "/vpnjantit");
    assert_eq!(cfg.servers[0].uuid, UUID);

    // 4. the profile survives a store round trip (the store seals the UUID)
    let sealed = sni_tunnel::store::ServerFile::from(&cfg.servers[0]);
    let restored: ServerProfile = sealed.into();
    assert_eq!(restored.sni, "new-sni.example");
    assert_eq!(restored.address, "sp3.example.com");
    assert_eq!(restored.uuid, UUID);
    assert_eq!(restored.ws_host, "ea.com");
}

#[test]
fn editing_the_sni_never_changes_address_or_host() {
    let p = sample();
    let mut fields = EditorFields::from_profile(&p);
    fields.sni = "totally-different.example".into();
    let edited = fields.to_profile(&p).unwrap();
    assert_eq!(edited.sni, "totally-different.example");
    assert_eq!(edited.address, p.address);
    assert_eq!(edited.ws_host, p.ws_host);
    assert_eq!(edited.ws_path, p.ws_path);
    assert_eq!(edited.uuid, p.uuid);
}

#[test]
fn an_imported_sni_is_detected_and_shown() {
    let link = vmess::to_vmess_link(&sample());
    let imported = vmess::parse_any(&link).unwrap();
    assert_eq!(imported[0].sni, "ea.com");
    let fields = EditorFields::from_profile(&imported[0]);
    assert_eq!(fields.sni, "ea.com");
}

#[test]
fn export_import_round_trip_for_every_protocol() {
    for protocol in ["vmess", "vless", "trojan"] {
        let mut p = sample();
        p.protocol = protocol.into();
        if protocol == "trojan" {
            p.uuid = "the-password".into();
        }
        let link = vmess::to_vmess_link(&p);
        let back = vmess::parse_any(&link).unwrap_or_else(|e| panic!("{protocol}: {e}"));
        assert_eq!(back.len(), 1, "{protocol}");
        let b = &back[0];
        assert_eq!(b.protocol, protocol);
        assert_eq!(b.address, p.address, "{protocol}");
        assert_eq!(b.port, p.port, "{protocol}");
        assert_eq!(b.uuid, p.uuid, "{protocol}");
        assert_eq!(b.sni, p.sni, "{protocol}");
        assert_eq!(b.ws_host, p.ws_host, "{protocol}");
        assert_eq!(b.ws_path, p.ws_path, "{protocol}");
        assert_eq!(b.network, p.network, "{protocol}");
        assert_eq!(b.tls, p.tls, "{protocol}");
    }
}

#[test]
fn the_generated_xray_config_matches_the_profile() {
    let p = sample();
    let ep = resolve_endpoint(&p, None);
    let cfg = sni_tunnel::xray::build_config(&ep, &Default::default(), 1080);
    let ob = &cfg["outbounds"][0];
    assert_eq!(ob["protocol"], "vmess");
    assert_eq!(ob["settings"]["vnext"][0]["address"], p.address);
    assert_eq!(ob["settings"]["vnext"][0]["port"], p.port);
    assert_eq!(ob["settings"]["vnext"][0]["users"][0]["id"], p.uuid);
    let stream = &ob["streamSettings"];
    assert_eq!(stream["network"], "ws");
    assert_eq!(stream["security"], "tls");
    assert_eq!(stream["tlsSettings"]["serverName"], "ea.com");
    assert_eq!(stream["wsSettings"]["host"], "ea.com");
    assert_eq!(stream["wsSettings"]["path"], "/vpnjantit");
    // the removed Xray 26 option must never be emitted
    assert!(stream["tlsSettings"].get("allowInsecure").is_none());
}

#[test]
fn an_invalid_profile_is_rejected_with_a_reason() {
    let p = sample();
    let mut fields = EditorFields::from_profile(&p);
    fields.port = "0".into();
    assert!(fields.to_profile(&p).is_err());

    let mut fields = EditorFields::from_profile(&p);
    fields.uuid = "not-a-uuid".into();
    assert!(fields.to_profile(&p).is_err());

    let mut fields = EditorFields::from_profile(&p);
    fields.address = "".into();
    assert!(fields.to_profile(&p).is_err());

    let mut fields = EditorFields::from_profile(&p);
    fields.network = "carrier-pigeon".into();
    assert!(fields.to_profile(&p).is_err());
}

#[test]
fn a_duplicate_is_independent_of_the_original() {
    let p = sample();
    let copy = duplicate_profile(&p);
    assert_ne!(copy.id, p.id);
    assert_eq!(copy.uuid, p.uuid);
    assert_eq!(copy.sni, p.sni);
    assert_ne!(copy.name, p.name);
}
