// TODO(brandonskiser): update bundle identifier for signed builds
#[cfg(target_os = "macos")]
const MACOS_BUNDLE_IDENTIFIER: &str = "com.amazon.codewhisperer";

/// Writes a generated Info.plist for the qchat executable under src/.
///
/// This is required for signing the executable since we must embed the Info.plist directly within
/// the binary.
#[cfg(target_os = "macos")]
fn write_plist() {
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleIdentifier</key>
	<string>{}</string>
	<key>CFBundleName</key>
	<string>{}</string>
	<key>CFBundleVersion</key>
	<string>{}</string>
	<key>CFBundleShortVersionString</key>
	<string>{}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>NSHumanReadableCopyright</key>
	<string>Copyright © 2022 Amazon Q CLI Team (q-cli@amazon.com):Chay Nabors (nabochay@amazon.com):Brandon Kiser (bskiser@amazon.com) All rights reserved.</string>
</dict>
</plist>
"#,
        MACOS_BUNDLE_IDENTIFIER,
        option_env!("AMAZON_Q_BUILD_HASH").unwrap_or("unknown"),
        option_env!("AMAZON_Q_BUILD_DATETIME").unwrap_or("unknown"),
        env!("CARGO_PKG_VERSION")
    );

    std::fs::write("src/Info.plist", plist).expect("writing the Info.plist should not fail");
}

fn main() {
    inject_kiro_version();

    #[cfg(target_os = "macos")]
    write_plist();

    embed_bun_and_tui();
}

/// Writes bun and TUI assets to OUT_DIR for embedding via include_bytes!.
/// If env vars are not set, writes empty files so the build always succeeds.
///
/// On macOS, per-arch bun binaries are provided via BUN_EXECUTABLE_PATH_X86_64 and
/// BUN_EXECUTABLE_PATH_AARCH64. CARGO_CFG_TARGET_ARCH selects the correct one since
/// build scripts run once per target. This avoids lipo-ing bun into a universal binary
/// which would invalidate its code signature and require re-signing.
fn embed_bun_and_tui() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    let bun_dest = std::path::Path::new(&out_dir).join("bun_embedded");
    let tui_dest = std::path::Path::new(&out_dir).join("tui_embedded.js");
    let node_dest = std::path::Path::new(&out_dir).join("node_embedded");
    let kas_dest = std::path::Path::new(&out_dir).join("kas_bundle_embedded.tar.gz");

    // Per-arch bun paths (macOS): select based on CARGO_CFG_TARGET_ARCH
    let bun_path = if std::env::var("BUN_EXECUTABLE_PATH_X86_64").is_ok()
        || std::env::var("BUN_EXECUTABLE_PATH_AARCH64").is_ok()
    {
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
        let (path_var, sha_var) = match arch.as_str() {
            "x86_64" => ("BUN_EXECUTABLE_PATH_X86_64", "BUN_RUNTIME_SHA256_X86_64"),
            "aarch64" => ("BUN_EXECUTABLE_PATH_AARCH64", "BUN_RUNTIME_SHA256_AARCH64"),
            other => panic!("Unsupported target arch for per-arch bun: {other}"),
        };
        // Re-export the arch-specific SHA as BUN_RUNTIME_SHA256 for option_env! in embedded_tui.rs
        if let Ok(sha) = std::env::var(sha_var) {
            println!("cargo:rustc-env=BUN_RUNTIME_SHA256={sha}");
        }
        std::env::var(path_var).ok()
    } else {
        std::env::var("BUN_EXECUTABLE_PATH").ok()
    };

    // Remove existing files first — the bun binary is often installed read-only,
    // and std::fs::copy preserves permissions, so overwriting fails on subsequent builds.
    let _ = std::fs::remove_file(&bun_dest);
    if let Some(path) = bun_path {
        println!("cargo:rerun-if-changed={path}");
        std::fs::copy(&path, &bun_dest).expect("Failed to copy bun executable to OUT_DIR");
    } else {
        std::fs::write(&bun_dest, b"").unwrap();
    }

    let _ = std::fs::remove_file(&tui_dest);
    if let Ok(path) = std::env::var("TUI_JS_PATH") {
        println!("cargo:rerun-if-changed={path}");
        std::fs::copy(&path, &tui_dest).expect("Failed to copy TUI js to OUT_DIR");
    } else {
        std::fs::write(&tui_dest, b"").unwrap();
    }

    // Node binary for KAS agent (per-arch on macOS, single path otherwise)
    let node_path = if std::env::var("NODE_EXECUTABLE_PATH_X86_64").is_ok()
        || std::env::var("NODE_EXECUTABLE_PATH_AARCH64").is_ok()
    {
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
        let (path_var, sha_var) = match arch.as_str() {
            "x86_64" => ("NODE_EXECUTABLE_PATH_X86_64", "NODE_RUNTIME_SHA256_X86_64"),
            "aarch64" => ("NODE_EXECUTABLE_PATH_AARCH64", "NODE_RUNTIME_SHA256_AARCH64"),
            other => panic!("Unsupported target arch for per-arch node: {other}"),
        };
        if let Ok(sha) = std::env::var(sha_var) {
            println!("cargo:rustc-env=NODE_RUNTIME_SHA256={sha}");
        }
        std::env::var(path_var).ok()
    } else {
        std::env::var("NODE_EXECUTABLE_PATH").ok()
    };

    if let Some(path) = node_path {
        println!("cargo:rerun-if-changed={path}");
        let _ = std::fs::remove_file(&node_dest);
        std::fs::copy(&path, &node_dest).expect("Failed to copy node executable to OUT_DIR");
    } else {
        std::fs::write(&node_dest, b"").unwrap();
    }

    // KAS bundle (tar.gz of acp-server.js + node_modules)
    if let Ok(path) = std::env::var("KAS_BUNDLE_PATH") {
        println!("cargo:rerun-if-changed={path}");
        let _ = std::fs::remove_file(&kas_dest);
        std::fs::copy(&path, &kas_dest).expect("Failed to copy KAS bundle to OUT_DIR");
    } else {
        std::fs::write(&kas_dest, b"").unwrap();
    }
}

/// When `KIRO_VERSION` env var is set, override `CARGO_PKG_VERSION` so all
/// `env!("CARGO_PKG_VERSION")` callsites report the release version.
///
/// Otherwise, when `Cargo.toml` still carries the `0.0.0-dev` placeholder
/// (local dev — CI replaces it via `KIRO_VERSION` and/or sed), default to
/// `99.99.99-dev`. KRS gates "thinking" on `appVersion >= 2.4.0`, so a
/// `0.0.0-dev` build would silently disable thinking on every dev session.
/// The placeholder check ensures a real sed'd version is never clobbered.
fn inject_kiro_version() {
    println!("cargo:rerun-if-env-changed=KIRO_VERSION");
    if let Ok(version) = std::env::var("KIRO_VERSION")
        && !version.is_empty()
    {
        println!("cargo:rustc-env=CARGO_PKG_VERSION={version}");
    } else if matches!(std::env::var("CARGO_PKG_VERSION").as_deref(), Ok("0.0.0-dev")) {
        println!("cargo:rustc-env=CARGO_PKG_VERSION=99.99.99-dev");
    }
}
