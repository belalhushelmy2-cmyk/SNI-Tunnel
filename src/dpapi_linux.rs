//! Secret sealing for platforms without DPAPI (Linux).
//!
//! There is no DPAPI outside Windows, so the value is *obfuscated* with a
//! per-user key file instead: the key lives in the runtime data directory
//! with `0600` permissions and the payload is XORed with a SHA-256 based
//! keystream. This is obfuscation, not authenticated encryption — the file
//! permissions are the real protection. Windows keeps using DPAPI.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Keystream block size (SHA-256 output).
const BLOCK: usize = 32;

fn key_path() -> PathBuf {
    crate::util::app_dir().join("secret.key")
}

/// Read (or create) the per-user obfuscation key.
fn load_key(create: bool) -> Result<Vec<u8>> {
    use std::io::Write;
    let path = key_path();
    match std::fs::read(&path) {
        Ok(bytes) => {
            if bytes.len() != BLOCK {
                bail!("Invalid secret key; refusing to replace it");
            }
            return Ok(bytes);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {}
        Err(e) => return Err(e).context("Read the existing secret key"),
    }
    let mut key = Vec::with_capacity(BLOCK);
    while key.len() < BLOCK {
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    key.truncate(BLOCK);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Publish only a complete key. A second process must neither replace it
    // nor observe an empty file between create_new and write_all.
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<Vec<u8>> {
        let mut stream = options
            .open(&temporary)
            .context("Create private key temporary file")?;
        stream
            .write_all(&key)
            .context("Write the secret key file")?;
        stream.sync_all().context("Flush the secret key file")?;
        drop(stream);
        match std::fs::hard_link(&temporary, &path) {
            Ok(()) => {
                #[cfg(unix)]
                std::fs::File::open(path.parent().unwrap())?.sync_all()?;
                Ok(key)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => load_key(false),
            Err(e) => Err(e).context("Publish the private secret key file"),
        }
    })();
    let _ = std::fs::remove_file(&temporary);
    result
}

/// XOR the payload with the keystream derived from the key.
fn apply(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut counter: u64 = 0;
    let mut block_bytes = Vec::with_capacity(BLOCK);
    let mut i = 0;
    while i < data.len() {
        if block_bytes.is_empty() {
            let mut h = Sha256::new();
            h.update(key);
            h.update(counter.to_le_bytes());
            block_bytes = h.finalize().to_vec();
            counter += 1;
        }
        let take = BLOCK.min(data.len() - i);
        for j in 0..take {
            out.push(data[i + j] ^ block_bytes[j]);
        }
        block_bytes.drain(..take);
        i += take;
    }
    out
}

/// Obfuscate a secret.
pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
    let key = load_key(true)?;
    Ok(apply(&key, plain))
}

/// Recover a secret.
pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>> {
    let key = load_key(false)?;
    if cipher.is_empty() {
        bail!("empty payload");
    }
    Ok(apply(&key, cipher))
}
