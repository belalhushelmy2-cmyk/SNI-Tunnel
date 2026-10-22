use std::{path::Path, process::Command};
fn cli(data: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sni-tunnel"))
        .env("SNI_TUNNEL_DATA_DIR", data)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn migration_preserves_source_credentials_and_refuses_overwrite() {
    let directory = std::env::temp_dir().join(format!("sni-v2-migrate-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(directory.join("previous")).unwrap();
    let source = directory.join("previous");
    let destination = directory.join("v2");
    let profile = serde_json::json!({"add":"server.example","port":443,"id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"ws","tls":"tls","sni":"ea.com","path":"/ws","host":"front.example","fp":"chrome","alpn":"http/1.1","dns":"1.1.1.1","ws_headers":{"X-Extra":"retained"}});
    let raw=serde_json::json!({"servers":[{"id":"original","name":"Preserved","vmess":profile.to_string()}],"selected":"original","mux":false,"sni":"ea.com","ws-host":"","ws-path":"","insecure":false,"other_preference":{"keep":true}}).to_string();
    std::fs::write(source.join("profile.json"), &raw).unwrap();
    let migrated = cli(&destination, &["migrate", source.to_str().unwrap()]);
    assert!(
        migrated.status.success(),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(source.join("profile.json")).unwrap(),
        raw
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("migration.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["profiles"], 1);
    assert_eq!(receipt["verified"], true);
    let backup = std::path::PathBuf::from(receipt["backup"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(backup.join("profile.json")).unwrap(),
        raw
    );
    let config = cli(&destination, &["linux-config", "--socks-port", "19081"]);
    assert!(
        config.status.success(),
        "{}",
        String::from_utf8_lossy(&config.stderr)
    );
    let runtime: serde_json::Value = serde_json::from_slice(&config.stdout).unwrap();
    assert_eq!(
        runtime["outbounds"][0]["settings"]["vnext"][0]["users"][0]["id"],
        profile["id"]
    );
    assert_eq!(
        runtime["outbounds"][0]["streamSettings"]["tlsSettings"]["serverName"],
        "ea.com"
    );
    assert_eq!(
        runtime["outbounds"][0]["streamSettings"]["wsSettings"]["headers"]["X-Extra"],
        "retained"
    );
    let saved = std::fs::read(destination.join("config.json")).unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains(profile["id"].as_str().unwrap()));
    assert!(!cli(&destination, &["migrate", source.to_str().unwrap()])
        .status
        .success());
    assert_eq!(
        std::fs::read(destination.join("config.json")).unwrap(),
        saved
    );
    let native = directory.join("v2-copy");
    assert!(cli(&native, &["migrate", destination.to_str().unwrap()])
        .status
        .success());
    assert_eq!(std::fs::read(native.join("config.json")).unwrap(), saved);
    std::fs::remove_dir_all(directory).unwrap();
}
