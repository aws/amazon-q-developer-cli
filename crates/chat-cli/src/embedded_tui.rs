use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    Duration,
    Instant,
};

pub use chat_cli_v2::launch_options::TuiAssetPaths;
use chat_cli_v2::util::file_lock::{
    FileLockError,
    try_file_lock_at,
    with_file_lock_at,
};
use eyre::{
    Context as _,
    Result,
};
use tracing::{
    debug,
    info,
    warn,
};

use crate::os::Os;
use crate::util::consts::env_var::{
    KIRO_KAS_NODE_PATH,
    KIRO_KAS_SERVER_PATH,
};
use crate::util::paths::{
    bun_path,
    bun_sha256_path,
    kas_bundle_dir,
    node_path,
    node_sha256_path,
    tui_js_path,
    tui_js_sha256_path,
};

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

const NODE_RUNTIME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/node_embedded"));
const NODE_RUNTIME_SHA256: &[u8] = match option_env!("NODE_RUNTIME_SHA256") {
    Some(s) => s.as_bytes(),
    None => b"",
};

const KAS_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kas_bundle_embedded.tar.gz"));
const KAS_BUNDLE_SHA256: &[u8] = match option_env!("KAS_BUNDLE_SHA256") {
    Some(s) => s.as_bytes(),
    None => b"",
};

/// The KAS bundle bytes to use at runtime: the embedded bundle when present,
/// otherwise the file at `KAS_BUNDLE_PATH`. The latter lets dev/test builds
/// (which embed no bundle) run KAS and compute its version without a release
/// build. Returns `None` when neither source yields bytes.
pub fn kas_bundle() -> Option<std::borrow::Cow<'static, [u8]>> {
    if !KAS_BUNDLE.is_empty() {
        return Some(std::borrow::Cow::Borrowed(KAS_BUNDLE));
    }
    let path = std::env::var_os(crate::util::consts::env_var::KAS_BUNDLE_PATH)?;
    let bytes = std::fs::read(path).ok()?;
    (!bytes.is_empty()).then_some(std::borrow::Cow::Owned(bytes))
}

/// Identifier for the running KAS bundle: `{cli-version}-{bundle-sha256}`.
/// Namespaces the extracted bundle directory and keys the `extracted_kas_versions`
/// heartbeat/GC table. Returns `None` when no bundle is available, so callers
/// skip extraction and persistence entirely.
pub fn kas_version() -> Option<String> {
    static VERSION: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| Some(format!("{}-{}", env!("CARGO_PKG_VERSION"), kas_bundle_sha256_hex()?)))
        .clone()
}

/// The bundle's sha256 as lowercase hex: the embedded compile-time hash when
/// present, otherwise computed from the fallback bundle bytes.
fn kas_bundle_sha256_hex() -> Option<String> {
    if !KAS_BUNDLE_SHA256.is_empty() {
        return Some(String::from_utf8_lossy(KAS_BUNDLE_SHA256).into_owned());
    }
    use sha2::{
        Digest,
        Sha256,
    };
    Some(format!("{:x}", Sha256::digest(kas_bundle()?.as_ref())))
}

/// Ensure the KAS `bundle` for `version` is extracted under `kas_root`,
/// returning the versioned directory `kas_root/{version}`.
///
/// Fast path: if the versioned dir already exists it is returned without
/// touching the lock. Otherwise a per-version advisory lock serializes
/// extraction across processes; the bundle is unpacked into a
/// `.incoming-{pid}-{version}` scratch dir and atomically renamed into place,
/// so the versioned dir only ever appears fully populated. If the lock cannot
/// be acquired within `lock_timeout`, extraction proceeds without the lock -
/// the atomic rename keeps the result correct, we only lose work dedup.
pub async fn ensure_kas_extracted(
    kas_root: &Path,
    version: &str,
    bundle: &[u8],
    lock_timeout: Duration,
) -> Result<PathBuf> {
    let versioned = kas_root.join(version);
    if versioned.exists() {
        debug!(version, path = %versioned.display(), "KAS bundle already extracted");
        return Ok(versioned);
    }

    let lock_path = kas_root.join(format!("{version}.lock"));
    match with_file_lock_at(&lock_path, lock_timeout, || {
        extract_to_versioned(kas_root, version, bundle)
    })
    .await
    {
        Ok(result) => result,
        Err(FileLockError::Timeout { .. }) => {
            warn!(version, "KAS extraction lock timed out; extracting without dedup");
            extract_to_versioned(kas_root, version, bundle).await
        },
        Err(FileLockError::Io { source, .. }) => Err(source).context("failed to acquire KAS extraction lock"),
    }
}

/// Unpack `bundle` into `kas_root/{version}` via temp-dir + atomic rename.
/// Re-checks for an existing versioned dir first (a peer may have won the race
/// while we waited on the lock).
async fn extract_to_versioned(kas_root: &Path, version: &str, bundle: &[u8]) -> Result<PathBuf> {
    let versioned = kas_root.join(version);
    if versioned.exists() {
        debug!(version, path = %versioned.display(), "KAS bundle extracted by a peer while waiting for the lock");
        return Ok(versioned);
    }
    std::fs::create_dir_all(kas_root).with_context(|| format!("failed to create KAS root: {}", kas_root.display()))?;

    let tmp = kas_root.join(format!(".incoming-{}-{}", std::process::id(), version));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;

    info!(version, path = %versioned.display(), bundle_bytes = bundle.len(), "extracting KAS bundle");
    let start = Instant::now();
    let decoder = flate2::read::GzDecoder::new(bundle);
    let mut archive = tar::Archive::new(decoder);
    if let Err(err) = archive.unpack(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(err).context("failed to unpack KAS bundle");
    }

    match std::fs::rename(&tmp, &versioned) {
        Ok(()) => {
            info!(
                version,
                path = %versioned.display(),
                elapsed_ms = start.elapsed().as_millis(),
                "extracted KAS bundle"
            );
            Ok(versioned)
        },
        // A peer finished first and the versioned dir now exists; discard ours.
        Err(_) if versioned.exists() => {
            debug!(version, "discarded extraction; a peer won the rename");
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(versioned)
        },
        Err(err) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(err).with_context(|| format!("failed to move KAS bundle into place: {}", versioned.display()))
        },
    }
}

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

/// Lock-acquisition budget for KAS bundle extraction: long enough to wait out a
/// peer's cold extraction. On timeout we extract without the lock - the atomic
/// rename keeps the result correct, only work dedup is lost.
const KAS_EXTRACT_LOCK_TIMEOUT: Duration = Duration::from_secs(40);

/// Extract the embedded node binary if present, returning its canonicalized
/// path. Dev/test builds embed no node binary and return `None`, leaving the
/// caller to fall back to `KIRO_KAS_NODE_PATH`.
async fn extract_kas_node_if_needed(os: &Os) -> Result<Option<PathBuf>> {
    if NODE_RUNTIME.is_empty() {
        return Ok(None);
    }
    let node_extract_path = node_path()?;
    let node_sha_path = node_sha256_path()?;

    let node_extracted = extract_asset_if_needed(
        os,
        &node_extract_path,
        &node_sha_path,
        NODE_RUNTIME,
        NODE_RUNTIME_SHA256,
    )
    .await?;
    if node_extracted {
        debug!(path = %node_extract_path.display(), "extracted KAS node runtime");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = os.fs.symlink_metadata(&node_extract_path).await?.permissions();
            perms.set_mode(0o755);
            os.fs.set_permissions(&node_extract_path, perms).await?;
        }
    } else {
        debug!(path = %node_extract_path.display(), "KAS node runtime already present");
    }

    // dunce::canonicalize avoids Windows \\?\ verbatim paths that break Node ESM resolution.
    let node_extract_path = dunce::canonicalize(&node_extract_path)
        .with_context(|| format!("failed to canonicalize node path: {}", node_extract_path.display()))?;
    Ok(Some(node_extract_path))
}

/// Extract the KAS bundle into its per-version directory and return the
/// canonicalized `acp-server.js` path. Returns `None` when no bundle is
/// available (embedded or via `KAS_BUNDLE_PATH`), leaving the caller to fall
/// back to `KIRO_KAS_SERVER_PATH`.
async fn extract_kas_bundle_if_needed(os: &Os) -> Result<Option<PathBuf>> {
    let (Some(version), Some(bundle)) = (kas_version(), kas_bundle()) else {
        return Ok(None);
    };

    // Record the heartbeat before the directory exists. GC treats a fresh row
    // without a directory as a pending extraction and leaves it alone, so this
    // ordering closes the window where a concurrent GC could see the
    // freshly-renamed directory with no row and reap it as an orphan.
    if let Err(err) = os.database.touch_extracted_kas_version(&version) {
        warn!(
            ?err,
            version, "failed to record KAS version heartbeat before extraction"
        );
    }

    let kas_root = kas_bundle_dir()?;
    let versioned = ensure_kas_extracted(&kas_root, &version, bundle.as_ref(), KAS_EXTRACT_LOCK_TIMEOUT).await?;

    let server_path = versioned
        .join("node_modules")
        .join("@kiro")
        .join("agent")
        .join("dist")
        .join("server")
        .join("acp-server.js");

    // dunce::canonicalize avoids Windows \\?\ verbatim paths that break Node ESM resolution.
    let server_path = dunce::canonicalize(&server_path)
        .with_context(|| format!("failed to canonicalize KAS server path: {}", server_path.display()))?;
    Ok(Some(server_path))
}

/// Maximum idle age before an extracted KAS version is eligible for cleanup.
/// A version's heartbeat is refreshed on extraction, spawn, and every
/// `get-kas-token` callback, so any in-use version stays well inside this.
pub const KAS_VERSION_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// Outcome of a GC pass, for logging and tests.
#[derive(Debug, Default)]
pub struct GcReport {
    pub reaped: Vec<String>,
    pub dropped_rows: Vec<String>,
    pub swept: Vec<String>,
    pub skipped_locked: Vec<String>,
    /// Versions left in place because their heartbeat was refreshed by another
    /// process between the row snapshot and acquiring the lock.
    pub skipped_fresh: Vec<String>,
}

/// What happened to a reap candidate once its lock was held.
enum ReapAction {
    Removed,
    Revived,
    Failed,
}

/// Garbage-collect stale extracted KAS bundles under `kas_root`: delete
/// directories idle longer than `max_age` (or with no heartbeat), prune
/// orphaned rows, and sweep scratch directories left by dead processes.
///
/// Behaviors:
/// - `current_kas_version` is never reaped
/// - Directories idle longer than `max_age` or no corresponding DB row (ie, no "heartbeat") are
///   removed
/// - DB rows with no corresponding directory, AND are older than `max_age`, are removed
/// - Scratch directories (created during asset extraction) left by dead processes are removed
/// - Individual failures are logged, not propagated
///
/// Parameters:
/// - `kas_root` - path to where KAS assets are extracted
/// - `current_kas_version` - version of KAS embedded for the current CLI process
/// - `now_ms` - current timestamp, in milliseconds
/// - `max_age` - max age of extracted assets before being eligible for garbage collection
pub async fn run_gc(
    database: &crate::database::Database,
    kas_root: &Path,
    current_kas_version: Option<&str>,
    now_ms: i64,
    max_age: Duration,
    is_pid_alive: impl Fn(u32) -> bool,
) -> GcReport {
    let mut report = GcReport::default();

    let rows = match database.list_extracted_kas_versions() {
        Ok(rows) => rows,
        Err(err) => {
            warn!(?err, "KAS GC: failed to list versions");
            return report;
        },
    };
    let max_age_ms = max_age.as_millis() as i64;
    let is_fresh = |version: &str| {
        rows.iter()
            .find(|(v, _)| v == version)
            .is_some_and(|(_, t)| now_ms.saturating_sub(*t) <= max_age_ms)
    };

    // Snapshot the KAS root: versioned directories vs in-flight scratch dirs.
    let mut dirs: Vec<String> = Vec::new();
    let mut incoming: Vec<(String, Option<u32>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(kas_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".lock") {
                continue;
            }
            if let Some(rest) = name.strip_prefix(".incoming-") {
                let pid = rest.split_once('-').and_then(|(pid, _)| pid.parse::<u32>().ok());
                incoming.push((name, pid));
                continue;
            }
            if entry.path().is_dir() {
                dirs.push(name);
            }
        }
    }

    // Reap directories with a stale row or no row (orphans), guarded by the
    // per-version lock so a directory mid-extraction is skipped this pass.
    for version in &dirs {
        if Some(version.as_str()) == current_kas_version || is_fresh(version) {
            continue;
        }
        let dir = kas_root.join(version);
        let lock_path = kas_root.join(format!("{version}.lock"));
        let action = try_file_lock_at(&lock_path, || async {
            // Re-read the heartbeat under the lock: another process may have
            // refreshed it after the row snapshot was taken, reviving this
            // version while it is now in use.
            let revived = matches!(
                database.extracted_kas_version_last_used(version),
                Ok(Some(ts)) if now_ms.saturating_sub(ts) <= max_age_ms
            );
            if revived {
                return ReapAction::Revived;
            }
            if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
                ReapAction::Removed
            } else {
                ReapAction::Failed
            }
        })
        .await;
        match action {
            Ok(Some(ReapAction::Removed)) => {
                let _ = database.delete_extracted_kas_version(version);
                // The lock's version is gone for good, so its lock file is dead
                // weight; the OS already released the lock when the guard dropped.
                let _ = std::fs::remove_file(&lock_path);
                debug!(version, "KAS GC: reaped stale bundle");
                report.reaped.push(version.clone());
            },
            Ok(Some(ReapAction::Revived)) => {
                debug!(version, "KAS GC: skipped bundle revived by a peer");
                report.skipped_fresh.push(version.clone());
            },
            Ok(Some(ReapAction::Failed)) => warn!(version, "KAS GC: failed to remove bundle directory"),
            Ok(None) => {
                debug!(version, "KAS GC: skipped bundle locked by a peer");
                report.skipped_locked.push(version.clone());
            },
            Err(err) => warn!(?err, version, "KAS GC: failed to acquire lock for deletion"),
        }
    }

    // Prune rows whose directory is gone. A fresh row is a pending extraction,
    // so only stale ones are dropped.
    for (version, _) in &rows {
        if dirs.iter().any(|d| d == version) || Some(version.as_str()) == current_kas_version || is_fresh(version) {
            continue;
        }
        let _ = database.delete_extracted_kas_version(version);
        debug!(version, "KAS GC: dropped dangling heartbeat row");
        report.dropped_rows.push(version.clone());
    }

    // Sweep scratch directories whose owning process is gone.
    for (name, pid) in incoming {
        let alive = pid.is_some_and(&is_pid_alive);
        if !alive {
            let _ = std::fs::remove_dir_all(kas_root.join(&name));
            debug!(scratch = %name, "KAS GC: swept scratch dir of a dead process");
            report.swept.push(name);
        }
    }

    if !report.reaped.is_empty()
        || !report.dropped_rows.is_empty()
        || !report.swept.is_empty()
        || !report.skipped_fresh.is_empty()
    {
        info!(
            reaped = report.reaped.len(),
            dropped_rows = report.dropped_rows.len(),
            swept = report.swept.len(),
            skipped_locked = report.skipped_locked.len(),
            skipped_fresh = report.skipped_fresh.len(),
            "KAS GC pass complete",
        );
    }

    report
}

/// Default process-liveness probe used by GC in production. Conservative:
/// anything other than a definitive "no such process" counts as alive, so an
/// in-use scratch directory is never swept.
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    !matches!(kill(Pid::from_raw(pid as i32), None), Err(nix::errno::Errno::ESRCH))
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    true
}

/// Ensures that KAS assets (node runtime, KAS acp-server.js bundle) are extracted to the file
/// system.
///
/// Each path is taken from its env var if non-empty, otherwise from the
/// embedded asset extracted on demand. A `None` return for either part
/// means neither an env override nor an embedded asset was available
/// (dev builds without embedded assets).
///
/// Extraction is skipped when both env vars are already set. When
/// `print_extracting_message` is set and the bundle is not yet on disk (a cold
/// start, where the untar can take ~20s before the TUI appears), a short
/// progress note is printed to stderr.
pub async fn ensure_kas_assets(os: &Os, print_extracting_message: bool) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let node_override = os
        .env
        .get(KIRO_KAS_NODE_PATH)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let server_override = os
        .env
        .get(KIRO_KAS_SERVER_PATH)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    // On a cold start the bundle untar runs for ~20s with nothing on screen
    // (the TUI only appears once bun is spawned afterwards), so give interactive
    // users a heads-up. Silent on warm starts and when the server is overridden.
    if print_extracting_message
        && server_override.is_none()
        && let Some(version) = kas_version()
        && kas_bundle_dir().is_ok_and(|root| !root.join(&version).exists())
    {
        eprintln!("Preparing V3...");
    }

    let node = match node_override {
        Some(path) => Some(path),
        None => extract_kas_node_if_needed(os).await?,
    };
    let server = match server_override {
        Some(path) => Some(path),
        None => extract_kas_bundle_if_needed(os).await?,
    };
    Ok((node, server))
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

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;

    async fn test_db() -> crate::database::Database {
        crate::database::Database::new_default().await.unwrap()
    }

    fn epoch_ms_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    #[tokio::test]
    async fn gc_reaps_stale_dir_and_prunes_its_row() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-stale")).unwrap();
        db.touch_extracted_kas_version("v-stale").unwrap();

        let report = run_gc(
            &db,
            root.path(),
            None,
            epoch_ms_now() + 20 * DAY_MS,
            KAS_VERSION_MAX_AGE,
            |_| true,
        )
        .await;

        assert_eq!(report.reaped, vec!["v-stale".to_string()]);
        assert!(!root.path().join("v-stale").exists());
        assert!(db.list_extracted_kas_versions().unwrap().is_empty());
    }

    #[tokio::test]
    async fn gc_keeps_fresh_dir() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-fresh")).unwrap();
        db.touch_extracted_kas_version("v-fresh").unwrap();

        let report = run_gc(&db, root.path(), None, epoch_ms_now(), KAS_VERSION_MAX_AGE, |_| true).await;

        assert!(report.reaped.is_empty());
        assert!(root.path().join("v-fresh").exists());
    }

    #[tokio::test]
    async fn gc_keeps_current_dir_even_when_stale() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-current")).unwrap();
        db.touch_extracted_kas_version("v-current").unwrap();

        let report = run_gc(
            &db,
            root.path(),
            Some("v-current"),
            epoch_ms_now() + 99 * DAY_MS,
            KAS_VERSION_MAX_AGE,
            |_| true,
        )
        .await;

        assert!(report.reaped.is_empty());
        assert!(root.path().join("v-current").exists());
    }

    #[tokio::test]
    async fn gc_reaps_orphan_dir_without_row() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-orphan")).unwrap();

        let report = run_gc(&db, root.path(), None, epoch_ms_now(), KAS_VERSION_MAX_AGE, |_| true).await;

        assert_eq!(report.reaped, vec!["v-orphan".to_string()]);
        assert!(!root.path().join("v-orphan").exists());
    }

    #[tokio::test]
    async fn gc_keeps_fresh_row_without_dir_as_pending() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        db.touch_extracted_kas_version("v-pending").unwrap();

        let report = run_gc(&db, root.path(), None, epoch_ms_now(), KAS_VERSION_MAX_AGE, |_| true).await;

        assert!(report.dropped_rows.is_empty());
        assert_eq!(db.list_extracted_kas_versions().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn gc_prunes_stale_row_without_dir() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        db.touch_extracted_kas_version("v-dangling").unwrap();

        let report = run_gc(
            &db,
            root.path(),
            None,
            epoch_ms_now() + 30 * DAY_MS,
            KAS_VERSION_MAX_AGE,
            |_| true,
        )
        .await;

        assert_eq!(report.dropped_rows, vec!["v-dangling".to_string()]);
        assert!(db.list_extracted_kas_versions().unwrap().is_empty());
    }

    #[tokio::test]
    async fn gc_sweeps_incoming_unless_owning_pid_alive() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join(".incoming-100-v")).unwrap();
        std::fs::create_dir_all(root.path().join(".incoming-200-v")).unwrap();
        std::fs::create_dir_all(root.path().join(".incoming-garbage")).unwrap();

        // pid 100 alive, 200 dead, unparseable name swept as junk.
        let report = run_gc(&db, root.path(), None, epoch_ms_now(), KAS_VERSION_MAX_AGE, |pid| {
            pid == 100
        })
        .await;

        assert!(root.path().join(".incoming-100-v").exists());
        assert!(!root.path().join(".incoming-200-v").exists());
        assert!(!root.path().join(".incoming-garbage").exists());
        assert_eq!(report.swept.len(), 2);
    }

    #[tokio::test]
    async fn gc_skips_stale_dir_whose_lock_is_held() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-busy")).unwrap();
        db.touch_extracted_kas_version("v-busy").unwrap();
        let now = epoch_ms_now();
        let lock_path = root.path().join("v-busy.lock");

        // Hold the per-version lock while GC runs: the stale dir must be skipped.
        with_file_lock_at(&lock_path, Duration::from_secs(5), || async {
            let report = run_gc(&db, root.path(), None, now + 20 * DAY_MS, KAS_VERSION_MAX_AGE, |_| true).await;
            assert_eq!(report.skipped_locked, vec!["v-busy".to_string()]);
            assert!(report.reaped.is_empty());
        })
        .await
        .unwrap();

        assert!(root.path().join("v-busy").exists(), "locked dir must survive GC");
    }

    fn make_test_bundle() -> Vec<u8> {
        use std::io::Write as _;
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let body = b"marker-body";
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, "marker", &body[..]).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = Vec::new();
        let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        enc.write_all(&tar_buf).unwrap();
        enc.finish().unwrap();
        gz
    }

    #[tokio::test]
    async fn extraction_proceeds_when_lock_cannot_be_acquired() {
        let root = TempDir::new().unwrap();
        let version = "1.2.3-deadbeef";
        let bundle = make_test_bundle();
        let lock_path = root.path().join(format!("{version}.lock"));

        // Hold the per-version lock so extraction cannot acquire it within its
        // budget; it must fall back to extracting without the lock, and the
        // atomic rename still yields a fully populated versioned directory.
        with_file_lock_at(&lock_path, Duration::from_secs(5), || async {
            let dir = ensure_kas_extracted(root.path(), version, &bundle, Duration::from_millis(50))
                .await
                .unwrap();
            assert_eq!(dir, root.path().join(version));
            assert_eq!(std::fs::read(dir.join("marker")).unwrap(), b"marker-body");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn gc_removes_lock_file_when_reaping() {
        let db = test_db().await;
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("v-stale")).unwrap();
        std::fs::write(root.path().join("v-stale.lock"), b"{}").unwrap();
        db.touch_extracted_kas_version("v-stale").unwrap();

        let report = run_gc(
            &db,
            root.path(),
            None,
            epoch_ms_now() + 20 * DAY_MS,
            KAS_VERSION_MAX_AGE,
            |_| true,
        )
        .await;

        assert_eq!(report.reaped, vec!["v-stale".to_string()]);
        assert!(!root.path().join("v-stale").exists());
        assert!(
            !root.path().join("v-stale.lock").exists(),
            "lock file should be removed on reap"
        );
    }
}
