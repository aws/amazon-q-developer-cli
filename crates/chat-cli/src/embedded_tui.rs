use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;
use std::time::Instant;

use eyre::{
    Context as _,
    Result,
};
use tracing::{
    debug,
    info,
};

use crate::os::Os;
use crate::util::paths::{
    bun_path,
    bun_sha256_path,
    tui_js_path,
    tui_js_sha256_path,
};

/// Paths to the bun executable and TUI JS file to use
#[derive(Debug, Clone)]
pub struct TuiAssetPaths {
    pub bun_path: PathBuf,
    pub tui_js_path: PathBuf,
}

const BUN_RUNTIME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bun_embedded"));
const BUN_RUNTIME_SHA256: &[u8] = match option_env!("BUN_RUNTIME_SHA256") {
    Some(s) => s.as_bytes(),
    None => b"",
};

const TUI_JS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tui_embedded.js"));
const TUI_JS_SHA256: &[u8] = match option_env!("TUI_JS_SHA256") {
    Some(s) => s.as_bytes(),
    None => b"",
};

/// Extract the embedded bun executable and TUI JS file only if they don't exist or content has
/// changed. Returns the paths to use for bun and TUI JS.
/// If KIRO_TEST_TUI_JS_PATH is set, returns the provided JS path with the embedded bun.
pub async fn extract_tui_assets_if_needed(os: &Os) -> Result<TuiAssetPaths> {
    if let Ok(test_tui_path) = std::env::var("KIRO_TEST_TUI_JS_PATH") {
        info!(
            "KIRO_TEST_TUI_JS_PATH is set, using provided TUI path: {}",
            test_tui_path
        );
        return Ok(TuiAssetPaths {
            bun_path: PathBuf::from("bun"), // Use system bun
            tui_js_path: PathBuf::from(test_tui_path),
        });
    }

    extract_tui_assets_if_needed_impl(
        os,
        bun_path()?,
        bun_sha256_path()?,
        tui_js_path()?,
        tui_js_sha256_path()?,
    )
    .await?;

    Ok(TuiAssetPaths {
        bun_path: bun_path()?,
        tui_js_path: tui_js_path()?,
    })
}

/// Launch the V2 TUI. Extracts embedded assets and spawns bun with the TUI JS bundle.
pub async fn launch_v2(os: &Os) -> Result<ExitCode> {
    let asset_paths = extract_tui_assets_if_needed(os).await?;

    let args: Vec<String> = std::env::args().collect();
    let current_exe = std::env::current_exe()?;

    let exit_code = tokio::process::Command::new(&asset_paths.bun_path)
        .arg(&asset_paths.tui_js_path)
        .args(&args[1..])
        .env("KIRO_AGENT_PATH", &current_exe)
        .status()
        .await?
        .code()
        .map_or(ExitCode::FAILURE, |e| ExitCode::from(e as u8));

    Ok(exit_code)
}

async fn extract_tui_assets_if_needed_impl(
    os: &Os,
    bun_extract_path: impl AsRef<Path>,
    bun_sha_extract_path: impl AsRef<Path>,
    tui_extract_path: impl AsRef<Path>,
    tui_sha_extract_path: impl AsRef<Path>,
) -> Result<()> {
    let bun_extract_path = bun_extract_path.as_ref();
    let bun_sha_extract_path = bun_sha_extract_path.as_ref();
    let tui_extract_path = tui_extract_path.as_ref();
    let tui_sha_extract_path = tui_sha_extract_path.as_ref();

    if !are_assets_embedded(os) {
        info!("tui assets not embedded, skipping extraction");
        return Ok(());
    }

    info!(
        "Extracting bun and tui to: {}, {}",
        bun_extract_path.to_string_lossy(),
        tui_extract_path.to_string_lossy(),
    );

    let start_time = Instant::now();

    let bun_extracted = extract_asset_if_needed(
        os,
        bun_extract_path,
        bun_sha_extract_path,
        BUN_RUNTIME,
        BUN_RUNTIME_SHA256,
    )
    .await?;
    if bun_extracted {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = os.fs.symlink_metadata(bun_extract_path).await?.permissions();
            perms.set_mode(0o755);
            os.fs.set_permissions(bun_extract_path, perms).await?;
        }
    }

    let tui_extracted =
        extract_asset_if_needed(os, tui_extract_path, tui_sha_extract_path, TUI_JS, TUI_JS_SHA256).await?;

    info!(
        bun_extracted,
        tui_extracted,
        time_elapsed_ms = start_time.elapsed().as_millis(),
        "asset extraction complete",
    );

    Ok(())
}

#[cfg(not(test))]
pub fn are_assets_embedded(_os: &Os) -> bool {
    !BUN_RUNTIME.is_empty() && !TUI_JS.is_empty()
}

#[cfg(test)]
pub fn are_assets_embedded(os: &Os) -> bool {
    !os.env.get("BUN_RUNTIME").unwrap().is_empty() && !os.env.get("TUI_JS").unwrap().is_empty()
}

/// Check if file needs extraction by comparing SHA256 hashes
async fn extract_asset_if_needed(
    os: &Os,
    asset_path: &Path,
    asset_sha_path: &Path,
    embedded_asset_content: &[u8],
    embedded_asset_sha: &[u8],
) -> Result<bool> {
    let should_extract = {
        if !os.fs.exists(asset_path) {
            debug!(?asset_path, "path does not exist, extracting");
            true
        } else if !os.fs.exists(asset_sha_path) {
            debug!(?asset_sha_path, "sha file does not exist, extracting");
            true
        } else {
            let existing_sha = os
                .fs
                .read(asset_sha_path)
                .await
                .with_context(|| format!("failed to read sha file: {}", asset_sha_path.display()))?;
            if existing_sha != embedded_asset_sha {
                debug!(
                    ?asset_sha_path,
                    "existing hash is different from embedded hash, extracting"
                );
                true
            } else {
                false
            }
        }
    };

    if !should_extract {
        info!(?asset_path, "asset does not need to be extracted");
        return Ok(false);
    }

    info!(?asset_path, "extracting asset");
    let start_time = Instant::now();
    if let Some(parent) = asset_path.parent() {
        os.fs.create_dir_all(parent).await?;
    }
    // Write to a temp file then rename to avoid ETXTBSY (Text file busy) on Linux.
    // rename() works even if the destination is being executed — the old inode stays
    // alive for the running process while new executions pick up the new file.
    let tmp_path = asset_path.with_extension(format!("tmp.{}", std::process::id()));
    os.fs.write(&tmp_path, embedded_asset_content).await?;
    os.fs.rename(&tmp_path, asset_path).await?;
    os.fs.write(asset_sha_path, embedded_asset_sha).await?;
    info!(
        elapsed_ms = start_time.elapsed().as_millis(),
        "asset extracted successfully"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::os::Env;

    async fn create_test_os_with_assets() -> Os {
        let env = Env::from_slice(&[("BUN_RUNTIME", "fake_bun_content"), ("TUI_JS", "fake_tui_content")]);
        let mut os = Os::new().await.unwrap();
        os.env = env;
        os
    }

    async fn create_test_os_without_assets() -> Os {
        let env = Env::from_slice(&[("BUN_RUNTIME", ""), ("TUI_JS", "")]);
        let mut os = Os::new().await.unwrap();
        os.env = env;
        os
    }

    #[tokio::test]
    async fn test_extract_tui_assets_if_needed_impl_skips_when_not_embedded() {
        let os = create_test_os_without_assets().await;
        let temp_dir = TempDir::new().unwrap();
        let bun_path = temp_dir.path().join("test_bun");
        let bun_sha_path = temp_dir.path().join("test_bun.sha256");
        let tui_path = temp_dir.path().join("test_tui.js");
        let tui_sha_path = temp_dir.path().join("test_tui.js.sha256");

        let result = extract_tui_assets_if_needed_impl(&os, &bun_path, &bun_sha_path, &tui_path, &tui_sha_path).await;

        assert!(result.is_ok());
        assert!(!os.fs.exists(&bun_path));
        assert!(!os.fs.exists(&tui_path));
    }

    #[tokio::test]
    async fn test_extract_tui_assets_if_needed_impl_extracts_when_embedded() {
        let os = create_test_os_with_assets().await;
        let temp_dir = TempDir::new().unwrap();
        let bun_path = temp_dir.path().join("test_bun_extract");
        let bun_sha_path = temp_dir.path().join("test_bun_extract.sha256");
        let tui_path = temp_dir.path().join("test_tui_extract.js");
        let tui_sha_path = temp_dir.path().join("test_tui_extract.js.sha256");

        let result = extract_tui_assets_if_needed_impl(&os, &bun_path, &bun_sha_path, &tui_path, &tui_sha_path).await;

        assert!(result.is_ok());
        assert!(os.fs.exists(&bun_path));
        assert!(os.fs.exists(&tui_path));

        let bun_content = os.fs.read(&bun_path).await.unwrap();
        let tui_content = os.fs.read(&tui_path).await.unwrap();
        assert_eq!(bun_content, BUN_RUNTIME);
        assert_eq!(tui_content, TUI_JS);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = os.fs.symlink_metadata(&bun_path).await.unwrap();
            let mode = metadata.permissions().mode();
            assert_eq!(mode & 0o111, 0o111); // Check executable bits
        }
    }

    #[tokio::test]
    async fn test_extract_tui_assets_if_needed_impl_reextracts_when_hash_differs() {
        let os = create_test_os_with_assets().await;
        let temp_dir = TempDir::new().unwrap();
        let bun_path = temp_dir.path().join("test_bun_hash");
        let bun_sha_path = temp_dir.path().join("test_bun_hash.sha256");
        let tui_path = temp_dir.path().join("test_tui_hash.js");
        let tui_sha_path = temp_dir.path().join("test_tui_hash.js.sha256");

        // Pre-populate files with different content and different SHA
        os.fs.create_dir_all(temp_dir.path()).await.unwrap();
        os.fs.write(&bun_path, b"old_bun_content").await.unwrap();
        os.fs.write(&bun_sha_path, b"old_bun_sha").await.unwrap();
        os.fs.write(&tui_path, b"old_tui_content").await.unwrap();
        os.fs.write(&tui_sha_path, b"old_tui_sha").await.unwrap();

        let result = extract_tui_assets_if_needed_impl(&os, &bun_path, &bun_sha_path, &tui_path, &tui_sha_path).await;

        assert!(result.is_ok());

        // Verify files were overwritten with correct content
        let bun_content = os.fs.read(&bun_path).await.unwrap();
        let bun_sha_content = os.fs.read(&bun_sha_path).await.unwrap();
        let tui_content = os.fs.read(&tui_path).await.unwrap();
        let tui_sha_content = os.fs.read(&tui_sha_path).await.unwrap();
        assert_eq!(bun_content, BUN_RUNTIME);
        assert_eq!(bun_sha_content, BUN_RUNTIME_SHA256);
        assert_eq!(tui_content, TUI_JS);
        assert_eq!(tui_sha_content, TUI_JS_SHA256);
    }
}
