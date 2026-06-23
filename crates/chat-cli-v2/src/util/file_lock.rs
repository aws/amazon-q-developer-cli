//! Generic cross-process file lock built on OS advisory locks (`fd-lock`).
//!
//! The lock is owned by the open file descriptor, so the OS releases it when
//! the holding process exits (normal exit, panic, or `kill -9`) - there is no
//! stale-lock state to reclaim. Advisory only: cooperating processes must all
//! use this same mechanism on the same path.
//!
//! `auth::refresh_coordinator` keeps its own near-identical copy; this module
//! is the shared, generically-tested home those callers can migrate onto.

use std::future::Future;
use std::io::{
    ErrorKind,
    Seek,
    SeekFrom,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    Duration,
    SystemTime,
    UNIX_EPOCH,
};

use tokio::time;

/// Default poll interval while waiting to acquire a contended lock.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum FileLockError {
    #[error("timed out after {timeout:?} acquiring file lock at {path}")]
    Timeout { path: PathBuf, timeout: Duration },
    #[error("io error acquiring file lock at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}

fn open_lock_file(path: &Path) -> Result<std::fs::File, FileLockError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path).map_err(|source| FileLockError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Record the holding pid and acquisition time into the lock file so `cat`/
/// `lsof` reveal who holds it. Purely informational; the lock comes from the
/// fd, not the file body.
fn write_pid(file: &mut std::fs::File) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = write!(file, "{{\"pid\":{},\"acquired_at_ms\":{}}}", std::process::id(), now_ms);
    let _ = file.flush();
}

/// Run `work` while holding an exclusive advisory lock on `path`. The
/// `timeout` bounds only acquiring the lock (polled at `DEFAULT_POLL_INTERVAL`);
/// once the lock is held, `work` runs to completion without a deadline. Returns
/// `Ok(work_output)` once the lock is held and `work` completes, or
/// `FileLockError::Timeout`/`Io` if the lock could not be acquired. `work`'s own
/// result type is opaque to the lock - callers that want to distinguish lock
/// failure from work failure should set `T` to a `Result`.
pub async fn with_file_lock_at<F, Fut, T>(path: &Path, timeout: Duration, work: F) -> Result<T, FileLockError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let file = open_lock_file(path)?;
    let mut rw = fd_lock::RwLock::new(file);
    let deadline = time::Instant::now() + timeout;
    let mut guard = loop {
        match rw.try_write() {
            Ok(guard) => break guard,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {},
            Err(source) => {
                return Err(FileLockError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            },
        }
        if time::Instant::now() >= deadline {
            return Err(FileLockError::Timeout {
                path: path.to_path_buf(),
                timeout,
            });
        }
        time::sleep(DEFAULT_POLL_INTERVAL).await;
    };
    write_pid(&mut guard);
    Ok(work().await)
}

/// Try to acquire the exclusive lock without blocking, running `work` only if
/// it is free. Returns `Ok(Some(work_output))` when acquired, `Ok(None)` when
/// the lock is currently held by another holder, or `Err` on io failure. Used
/// by GC to skip resources that are actively being extracted or used.
pub async fn try_file_lock_at<F, Fut, T>(path: &Path, work: F) -> Result<Option<T>, FileLockError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let file = open_lock_file(path)?;
    let mut rw = fd_lock::RwLock::new(file);
    match rw.try_write() {
        Ok(mut guard) => {
            write_pid(&mut guard);
            Ok(Some(work().await))
        },
        Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
        Err(source) => Err(FileLockError::Io {
            path: path.to_path_buf(),
            source,
        }),
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
    async fn serializes_concurrent_callers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lock");
        let active = Arc::new(AtomicU32::new(0));
        let max_seen = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let path = path.clone();
            let active = Arc::clone(&active);
            let max_seen = Arc::clone(&max_seen);
            handles.push(tokio::spawn(async move {
                with_file_lock_at(&path, Duration::from_secs(5), || async {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    time::sleep(Duration::from_millis(20)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .await
                .unwrap();
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1, "lock must serialize callers");
    }

    #[tokio::test]
    async fn timeout_fires_while_held() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lock");
        with_file_lock_at(&path, Duration::from_secs(5), || async {
            let contended = with_file_lock_at(&path, Duration::from_millis(50), || async {}).await;
            assert!(matches!(contended, Err(FileLockError::Timeout { .. })));
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn timeout_bounds_acquisition_not_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lock");
        // The lock is free, so acquisition is immediate; work then runs longer
        // than the acquisition timeout and must still complete.
        let result = with_file_lock_at(&path, Duration::from_millis(20), || async {
            time::sleep(Duration::from_millis(80)).await;
            7
        })
        .await
        .unwrap();
        assert_eq!(result, 7);
    }

    #[tokio::test]
    async fn try_lock_skips_when_held_and_runs_when_free() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lock");
        with_file_lock_at(&path, Duration::from_secs(5), || async {
            let skipped = try_file_lock_at(&path, || async {}).await.unwrap();
            assert!(skipped.is_none(), "try-lock must skip while held");
        })
        .await
        .unwrap();

        let acquired = try_file_lock_at(&path, || async { 42 }).await.unwrap();
        assert_eq!(acquired, Some(42), "try-lock must run work when free");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn creates_parent_and_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/x.lock");
        with_file_lock_at(&path, Duration::from_secs(5), || async {})
            .await
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
