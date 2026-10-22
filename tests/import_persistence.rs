//! Real process boundaries exercise the CLI/store path without a live server.
#[test]
fn imported_mux_and_explicit_flags_survive_restart() {
    let directory = std::env::temp_dir().join(format!("sni-import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let profile = directory.join("profile.json");
    std::fs::write(&profile, r#"{"add":"192.0.2.10","port":443,"id":"b831381d-6324-4d53-ad4f-8cda48b30811","net":"ws","tls":"tls","sni":"ea.com","mux_enabled":true,"mux_concurrency":4}"#).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_sni-tunnel"))
            .env("XDG_DATA_HOME", &directory)
            .env("LOCALAPPDATA", &directory)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    run(&[
        "add",
        profile.to_str().unwrap(),
        "--name",
        "Imported",
        "--select",
        "--mux-concurrency",
        "5",
    ]);
    let output = run(&["mux", "Imported", "--mux-concurrency", "6"]);
    assert!(output.contains("mux true (concurrency 6)"));
    let listing = run(&["list"]);
    assert!(listing.contains("ea.com"));
    // Export is deliberately scrubbed, but still proves edited metadata survived.
    let exported: serde_json::Value = serde_json::from_str(&run(&["export"])).unwrap();
    assert_eq!(exported[0]["mux_enabled"], true);
    assert_eq!(exported[0]["mux_concurrency"], 6);
    assert_eq!(exported[0]["sni"], "ea.com");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn repeated_imports_append_across_processes_without_changing_existing_profiles() {
    let directory = std::env::temp_dir().join(format!("sni-append-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let profile = directory.join("profile.json");
    std::fs::write(&profile, r#"{"id":"fixed-import-id","name":"Same server","address":"192.0.2.10","port":443,"uuid":"b831381d-6324-4d53-ad4f-8cda48b30811","network":"ws"}"#).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_sni-tunnel"))
            .env("SNI_TUNNEL_DATA_DIR", &directory)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    run(&["import", profile.to_str().unwrap()]);
    let first: serde_json::Value = serde_json::from_str(&run(&["export"])).unwrap();
    run(&["import", profile.to_str().unwrap()]);
    // A different account on the identical endpoint must also append.
    let text = std::fs::read_to_string(&profile).unwrap().replace(
        "b831381d-6324-4d53-ad4f-8cda48b30811",
        "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    );
    std::fs::write(&profile, text).unwrap();
    run(&["import", profile.to_str().unwrap()]);
    let all: serde_json::Value = serde_json::from_str(&run(&["export"])).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 3);
    assert_eq!(
        all[0], first[0],
        "the existing profile must stay byte-for-byte equivalent"
    );
    assert_eq!(all[1]["name"], "Same server (2)");
    assert_eq!(all[2]["name"], "Same server (3)");
    assert_ne!(all[0]["id"], all[1]["id"]);
    assert_ne!(all[1]["id"], all[2]["id"]);
    std::fs::remove_dir_all(directory).unwrap();
}
