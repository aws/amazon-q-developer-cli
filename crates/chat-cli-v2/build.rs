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

    // Download feed.json if FETCH_FEED environment variable is set
    if std::env::var("FETCH_FEED").is_ok() {
        download_feed_json();
    }

    #[cfg(target_os = "macos")]
    write_plist();

    // Conditionally embed Bun runtime and TUI
    embed_bun_and_tui();
}

/// Downloads the latest feed.json from the autocomplete repository.
/// This ensures official builds have the most up-to-date changelog information.
///
/// # Errors
///
/// Prints cargo warnings if:
/// - `curl` command is not available
/// - Network request fails
/// - File write operation fails
fn download_feed_json() {
    use std::process::Command;

    println!("cargo:warning=Downloading latest feed.json from autocomplete repo...");

    // Check if curl is available first
    let curl_check = Command::new("curl").arg("--version").output();

    if curl_check.is_err() {
        panic!(
            "curl command not found. Cannot download latest feed.json. Please install curl or build without FETCH_FEED=1 to use existing feed.json."
        );
    }

    let output = Command::new("curl")
        .args([
            "-H",
            "Accept: application/vnd.github.v3.raw",
            "-f",           // fail on HTTP errors
            "-s",           // silent
            "-v",           // verbose output printed to stderr
            "--show-error", // print error message to stderr (since -s is used)
            "https://api.github.com/repos/aws/amazon-q-developer-cli-autocomplete/contents/feed.json",
        ])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            if let Err(e) = std::fs::write("src/cli/feed.json", result.stdout) {
                panic!("Failed to write feed.json: {e}");
            } else {
                println!("cargo:warning=Successfully downloaded latest feed.json");
            }
        },
        Ok(result) => {
            let error_msg = if !result.stderr.is_empty() {
                format!("{}", String::from_utf8_lossy(&result.stderr))
            } else {
                "An unknown error occurred".to_string()
            };
            panic!("Failed to download feed.json: {error_msg}");
        },
        Err(e) => {
            panic!("Failed to execute curl: {e}");
        },
    }
}

/// Conditionally embeds Bun runtime and TUI bundle if environment variables are set.
/// For cross-compilation compatibility, files are copied to OUT_DIR which is always
/// accessible inside the build container.
fn embed_bun_and_tui() {
    // Always declare the cfg flags so rustc knows about them
    println!("cargo:rustc-check-cfg=cfg(bun_executable_path)");
    println!("cargo:rustc-check-cfg=cfg(tui_js_path)");

    let disable_embedding = std::env::var("DISABLE_V2_BUN").ok();
    if disable_embedding.is_some() {
        println!("cargo:warning=DISABLE_V2_BUN is set, not embedding bun for chat_cli_v2");
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let bun_path = std::env::var("BUN_EXECUTABLE_PATH").ok();
    let tui_path = std::env::var("TUI_JS_PATH").ok();

    if let Some(path) = bun_path {
        println!("cargo:warning=Embedding Bun executable");
        println!("cargo:rerun-if-changed={path}");

        // Copy to OUT_DIR for cross-compilation compatibility
        // Remove existing file first — the bun binary is often installed read-only,
        // and std::fs::copy preserves permissions, so overwriting fails on subsequent builds.
        let dest_path = std::path::Path::new(&out_dir).join("bun_embedded");
        let _ = std::fs::remove_file(&dest_path);
        std::fs::copy(&path, &dest_path).expect("Failed to copy bun executable to OUT_DIR");

        let sha = sha256_hex(&dest_path);
        println!("cargo:rustc-cfg=bun_executable_path");
        println!("cargo:rustc-env=BUN_EXECUTABLE_PATH={}", dest_path.display());
        println!("cargo:rustc-env=BUN_RUNTIME_SHA256={sha}");
    }

    if let Some(path) = tui_path {
        println!("cargo:warning=Embedding TUI js file");
        println!("cargo:rerun-if-changed={path}");

        // Copy to OUT_DIR for cross-compilation compatibility
        let dest_path = std::path::Path::new(&out_dir).join("tui_embedded.js");
        let _ = std::fs::remove_file(&dest_path);
        std::fs::copy(&path, &dest_path).expect("Failed to copy TUI js to OUT_DIR");

        let sha = sha256_hex(&dest_path);
        println!("cargo:rustc-cfg=tui_js_path");
        println!("cargo:rustc-env=TUI_JS_PATH={}", dest_path.display());
        println!("cargo:rustc-env=TUI_JS_SHA256={sha}");
    }
}

fn sha256_hex(path: &std::path::Path) -> String {
    use std::io::Read;

    use sha2::{
        Digest,
        Sha256,
    };
    let mut file = std::fs::File::open(path).expect("Failed to open file for SHA256");
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).expect("Failed to read file for SHA256");
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    format!("{:x}", hasher.finalize())
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
