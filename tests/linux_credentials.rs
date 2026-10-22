//! Each integration test binary has its own process environment.
#![cfg(not(windows))]

#[test]
fn simultaneous_first_saves_share_one_complete_key_and_never_replace_a_bad_key() {
    let directory = std::env::temp_dir().join(format!("sni-key-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let previous = std::env::var_os("XDG_DATA_HOME");
    std::env::set_var("XDG_DATA_HOME", &directory);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(32));
    let workers: Vec<_> = (0..32)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                sni_tunnel::dpapi::protect(b"test credential").unwrap()
            })
        })
        .collect();
    let ciphertexts: Vec<_> = workers.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(ciphertexts.windows(2).all(|p| p[0] == p[1]));
    assert_eq!(
        sni_tunnel::dpapi::unprotect(&ciphertexts[0]).unwrap(),
        b"test credential"
    );
    let key = directory.join("sni-tunnel-v2/secret.key");
    assert_eq!(std::fs::read(&key).unwrap().len(), 32);
    std::fs::write(&key, b"damaged key").unwrap();
    assert!(sni_tunnel::dpapi::protect(b"test credential").is_err());
    assert_eq!(std::fs::read(&key).unwrap(), b"damaged key");
    match previous {
        Some(value) => std::env::set_var("XDG_DATA_HOME", value),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    std::fs::remove_dir_all(directory).unwrap();
}
