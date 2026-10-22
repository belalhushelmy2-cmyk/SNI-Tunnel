fn main() {
    // The Slint UI is only built for the `gui` feature (the headless build
    // for servers/routers has no UI).
    #[cfg(feature = "gui")]
    build_ui();

    // The icon resource is Windows only. The check has to be about the
    // *target*, not the host: the build script itself always runs on the host.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon::embed();
    }
}

/// Compile the Slint UI (only with the `gui` feature).
#[cfg(feature = "gui")]
fn build_ui() {
    // Without this the build script would not run again when only the UI
    // file changed, and the old interface would be compiled.
    println!("cargo:rerun-if-changed=ui/app.slint");
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .expect("failed to compile Slint UI");
}

/// Embed `assets/icon.ico` as the executable icon (Windows targets only).
///
/// The GNU toolchain has no `rc.exe`, so we use `windres` from w64devkit. If
/// it is not available the build still succeeds — the program just keeps the
/// default executable icon.
mod windows_icon {
    use std::path::PathBuf;
    use std::process::Command;

    pub fn embed() {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        println!("cargo:rerun-if-changed=assets/app.rc");

        let root =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
        let rc = root.join("assets/app.rc");
        if !rc.is_file() {
            return;
        }

        let Some(windres) = find_windres() else {
            println!("cargo:warning=windres not found - the executable icon is skipped");
            return;
        };

        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string()));
        let obj = out_dir.join("app-icon.o");
        let mut command = Command::new(&windres);
        if std::env::var("HOST").ok() != std::env::var("TARGET").ok() {
            command
                .arg("--preprocessor=clang")
                .arg("--preprocessor-arg=-E")
                .arg("--preprocessor-arg=-xc");
        }
        let status = command
            // The .rc refers to "icon.ico" relative to its own directory.
            .current_dir(root.join("assets"))
            .arg("app.rc")
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&obj)
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("cargo:rustc-link-arg={}", obj.display());
            }
            Ok(s) => {
                println!("cargo:warning=windres failed ({s}) - the executable icon is skipped");
            }
            Err(e) => {
                println!(
                    "cargo:warning=could not run windres ({e}) - the executable icon is skipped"
                );
            }
        }
    }

    /// Look for `windres` on PATH and in the usual w64devkit location.
    fn find_windres() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("WINDRES") {
            return Some(path.into());
        }
        if let Some(path) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&path) {
                for name in ["windres.exe", "windres", "x86_64-w64-mingw32-windres"] {
                    let candidate = directory.join(name);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        let fixed = PathBuf::from(r"C:\tools\w64devkit\bin\windres.exe");
        if fixed.is_file() {
            return Some(fixed);
        }
        None
    }
}
