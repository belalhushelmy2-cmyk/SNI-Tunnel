//! Download, integrity-check and unpack the external networking binaries
//! (Xray-core, Wintun). Every artifact is pinned to an exact
//! SHA-256 so a compromised mirror cannot swap in a malicious core.

use crate::logging::Logger;
use crate::util;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub struct CorePackage {
    pub label: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    /// (path inside the archive, output filename in the bin dir)
    pub members: &'static [(&'static str, &'static str)],
}

pub const XRAY_PACKAGE: CorePackage = CorePackage {
    label: "Xray-core",
    url: "https://github.com/XTLS/Xray-core/releases/download/v26.3.27/Xray-windows-64.zip",
    sha256: "d004c39288ce9ada487c6f398c7c545f7d749e44bdfdd59dbc9f865afba4e1ad",
    members: &[("xray.exe", "xray.exe"), ("wintun.dll", "wintun.dll")],
};

pub fn package_present(pkg: &CorePackage) -> bool {
    let bin = util::bin_dir();
    pkg.members.iter().all(|(_, out)| bin.join(out).is_file())
}

/// Ensure every member of `pkg` exists in the bin directory, downloading and
/// verifying the archive first if needed.
pub async fn ensure_package(pkg: &CorePackage, verify: bool, logger: &Logger) -> Result<()> {
    // A packaged installation ships the core already - nothing to download.
    if let Some(path) = packaged_core_path() {
        logger.info(format!(
            "using the {} that was installed with the application ({})",
            pkg.label,
            path.display()
        ));
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        if xray_path().is_file() {
            return Ok(());
        }
        bail!("Native Xray binary is missing; install the complete package for this operating system and CPU architecture");
    }
    if package_present(pkg) {
        return Ok(());
    }

    logger.info(format!("Downloading {} ...", pkg.label));
    let client = reqwest::Client::builder()
        .user_agent("SniTunnel/0.1")
        .build()
        .context("build http client")?;
    let resp = client
        .get(pkg.url)
        .send()
        .await
        .with_context(|| format!("GET {}", pkg.url))?;
    if !resp.status().is_success() {
        bail!("download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await.context("read download body")?;
    logger.info(format!(
        "Downloaded {} ({})",
        pkg.label,
        util::format_bytes(bytes.len() as u64)
    ));

    let actual = hex::encode(Sha256::digest(&bytes));
    if verify {
        if actual != pkg.sha256 {
            bail!(
                "{} failed integrity check.\n expected {}\n      got {}",
                pkg.label,
                pkg.sha256,
                actual
            );
        }
        logger.info(format!("{} SHA-256 verified", pkg.label));
    } else {
        logger.warn(format!(
            "{} integrity verification is DISABLED (sha256 = {})",
            pkg.label, actual
        ));
    }

    let reader = std::io::Cursor::new(bytes.to_vec());
    let mut archive = zip::ZipArchive::new(reader).context("open zip archive")?;
    let bin = util::bin_dir();
    for (entry, out) in pkg.members {
        let mut file = archive
            .by_name(entry)
            .with_context(|| format!("archive is missing {entry}"))?;
        let dest = bin.join(out);
        let tmp = dest.with_extension("part");
        {
            let mut f = std::fs::File::create(&tmp).context("create bin file")?;
            std::io::copy(&mut file, &mut f).context("extract bin file")?;
        }
        std::fs::rename(&tmp, &dest).context("commit bin file")?;
        logger.info(format!("Installed {}", dest.display()));
    }
    Ok(())
}

/// The file name of the core on this platform.
pub const CORE_FILE: &str = if cfg!(windows) { "xray.exe" } else { "xray" };

/// A core that was *packaged* with the application (the installers ship it),
/// or `None` when the core has to be downloaded.
pub fn packaged_core_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    // The layout of the installers: <install dir>\core\xray.exe and
    // /usr/lib/sni-tunnel/xray.
    let candidates = [
        dir.join("core").join(CORE_FILE),
        dir.join(CORE_FILE),
        std::path::PathBuf::from("/usr/lib/sni-tunnel").join(CORE_FILE),
    ];
    candidates.into_iter().find(|c| c.is_file())
}

/// The path of the core the application uses.
pub fn xray_path() -> std::path::PathBuf {
    packaged_core_path().unwrap_or_else(|| util::bin_dir().join(CORE_FILE))
}
