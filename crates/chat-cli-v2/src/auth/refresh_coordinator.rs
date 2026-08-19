//! Cross-process coordination for OIDC refresh among kiro-cli processes.
//!
//! When N kiro-cli instances wake together they can all refresh the same
//! RT in parallel; AWS rotates the RT on first use, so losers see
//! `invalid_grant` between the winner's OIDC success and its store write.
//! The lock is best-effort for ~90 second windows; we always re-read the
//! store for a peer's successful refresh before deleting, to minimize
//! race-induced unexpected logouts.
//!
//! [`with_refresh_lock`] runs read-store → check → OIDC → write-store
//! under an exclusive `fd-lock` on `${data_local_dir}/kiro-cli/.refresh.lock`.
//! V1, V2, and the autocomplete sibling crate all resolve to the same
//! lock path via `dirs::data_local_dir()`, so they serialize on
//! `flock(2)` / `LockFileEx` regardless of which crate holds the lock.

use std::future::Future;
use std::io::ErrorKind;
use std::path::{
    Path,
    PathBuf,
};
use std::time::Duration;

use tokio::time;
use tracing::{
    trace,
    warn,
};

use crate::auth::AuthError;

const LOCK_FILE_NAME: &str = ".refresh.lock";

/// Total budget for `acquire + critical-section + release`. ~90s is
/// conservative: cancelling a process mid-save after AWS has rotated
/// the RT strands the new token and forces the next holder into
/// `invalid_grant`. Typical refresh is <5s; crossing 90s means
/// something is genuinely wedged.
pub(crate) const LOCK_HOLD_TIMEOUT: Duration = Duration::from_secs(90);

/// Polling interval for the lock-acquire loop. Short enough to feel
/// responsive once the lock is released, long enough to avoid burning
/// CPU.
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Resolves to `${data_local_dir}/kiro-cli/.refresh.lock`. Self-contained
/// (uses `dirs::data_local_dir`) so V1, V2, and autocomplete agree on the
/// path without any of them pulling in another crate's path layer.
pub fn lock_path() -> Result<PathBuf, AuthError> {
    dirs::data_local_dir()
        .map(|d| d.join("kiro-cli").join(LOCK_FILE_NAME))
        .ok_or_else(|| AuthError::OAuthCustomError("could not determine data_local_dir for refresh lock".into()))
}

/// Run `work` while holding the kiro-cli refresh lock, bounded by
/// [`LOCK_HOLD_TIMEOUT`]. Callers should re-read the secret store at
/// the top of `work` to short-circuit when a peer already refreshed.
pub async fn with_refresh_lock<F, Fut, T>(work: F) -> Result<T, AuthError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, AuthError>>,
{
    with_refresh_lock_at(&lock_path()?, LOCK_HOLD_TIMEOUT, work).await
}

async fn with_refresh_lock_at<F, Fut, T>(path: &Path, hold_timeout: Duration, work: F) -> Result<T, AuthError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, AuthError>>,
{
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Lock file is informational only; match the autocomplete sibling's 0o600.
        opts.mode(0o600);
    }
    let file = opts.open(path)?;

    let body = async move {
        // Pre-Polonius rejects returning a borrowed `RwLockWriteGuard<'_, _>`
        // from a function-level loop, so inline the acquire here.
        let mut rw = fd_lock::RwLock::new(file);
        let _guard = loop {
            match rw.try_write() {
                Ok(g) => break g,
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    time::sleep(LOCK_POLL_INTERVAL).await;
                },
                Err(e) => return Err(AuthError::Io(e)),
            }
        };
        trace!("acquired refresh lock");
        work().await
    };

    match time::timeout(hold_timeout, body).await {
        Ok(r) => r,
        Err(_) => {
            warn!(
                timeout_secs = hold_timeout.as_secs(),
                "refresh lock timed out (peer wedged?)"
            );
            // Reuse the existing AuthError timeout variant.
            Err(AuthError::OAuthTimeout)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{
        AtomicU32,
        Ordering,
    };

    use super::*;

    #[tokio::test]
    async fn lock_serializes_concurrent_callers() {
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("test.lock"));
        let counter = Arc::new(AtomicU32::new(0));
        let max_observed = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let path = Arc::clone(&path);
            let counter = Arc::clone(&counter);
            let max_observed = Arc::clone(&max_observed);
            handles.push(tokio::spawn(async move {
                with_refresh_lock_at(&path, LOCK_HOLD_TIMEOUT, || async {
                    let inside = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    max_observed.fetch_max(inside, Ordering::SeqCst);
                    time::sleep(Duration::from_millis(20)).await;
                    counter.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert_eq!(
            max_observed.load(Ordering::SeqCst),
            1,
            "more than one task was inside the critical section at once"
        );
    }

    #[tokio::test]
    async fn lock_timeout_fires_when_body_exceeds_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");
        let result = with_refresh_lock_at(&path, Duration::from_millis(50), || async {
            time::sleep(Duration::from_millis(500)).await;
            Ok::<_, AuthError>(())
        })
        .await;
        match result {
            Err(AuthError::OAuthTimeout) => {},
            other => panic!("expected OAuthTimeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn lock_creates_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("test.lock");
        with_refresh_lock_at(&path, LOCK_HOLD_TIMEOUT, || async { Ok::<_, AuthError>(()) })
            .await
            .unwrap();
        assert!(path.exists(), "lock file was not created");
        assert!(path.parent().unwrap().exists(), "parent directory was not created");
    }

    #[tokio::test]
    async fn lock_can_be_reused_across_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");
        for _ in 0..3 {
            with_refresh_lock_at(&path, LOCK_HOLD_TIMEOUT, || async { Ok::<_, AuthError>(()) })
                .await
                .unwrap();
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lock_file_is_mode_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");
        with_refresh_lock_at(&path, LOCK_HOLD_TIMEOUT, || async { Ok::<_, AuthError>(()) })
            .await
            .unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "lock file mode is {mode:o}, expected 600");
    }
}
