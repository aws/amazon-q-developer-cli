use std::path::{
    Path,
    PathBuf,
};
use std::time::Instant;

use eyre::Result;
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

#[cfg(bun_executable_path)]
const BUN_RUNTIME: &[u8] = include_bytes!(env!("BUN_EXECUTABLE_PATH"));

#[cfg(bun_executable_path)]
const BUN_RUNTIME_SHA256: &[u8] = env!("BUN_RUNTIME_SHA256").as_bytes();

#[cfg(not(bun_executable_path))]
const BUN_RUNTIME: &[u8] = b"dummy";

#[cfg(not(bun_executable_path))]
const BUN_RUNTIME_SHA256: &[u8] = b"dummy";

#[cfg(tui_js_path)]
const TUI_JS: &[u8] = include_bytes!(env!("TUI_JS_PATH"));

#[cfg(tui_js_path)]
const TUI_JS_SHA256: &[u8] = env!("TUI_JS_SHA256").as_bytes();

#[cfg(not(tui_js_path))]
const TUI_JS: &[u8] = b"dummy";

#[cfg(not(tui_js_path))]
const TUI_JS_SHA256: &[u8] = b"dummy";

/// Extract the embedded bun executable and TUI JS file only if they don't exist or content has
/// changed. Returns the paths to use for bun and TUI JS.
/// If KIRO_TEST_TUI_JS_PATH is set, returns system bun and the provided JS path.
pub async fn extract_tui_assets_if_needed(os: &Os) -> Result<TuiAssetPaths> {
    // If KIRO_TEST_TUI_JS_PATH is set, use system bun + provided JS path
    if let Ok(test_tui_path) = std::env::var("KIRO_TEST_TUI_JS_PATH") {
        info!(
            "KIRO_TEST_TUI_JS_PATH is set, using system bun and provided TUI path: {}",
            test_tui_path
        );
        return Ok(TuiAssetPaths {
            bun_path: PathBuf::from("bun"), // Use system bun
            tui_js_path: PathBuf::from(test_tui_path),
        });
    }

    // Normal embedded asset extraction
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
        // Ensure executable is set
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
fn are_assets_embedded(_os: &Os) -> bool {
    !BUN_RUNTIME.is_empty() && !TUI_JS.is_empty()
}

#[cfg(test)]
fn are_assets_embedded(os: &Os) -> bool {
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
        } else {
            let existing_sha = os.fs.read(asset_sha_path).await?;
            if existing_sha != embedded_asset_sha {
                debug!("existing hash is different from embedded hash, extracting");
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
    os.fs.write(asset_path, embedded_asset_content).await?;
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
