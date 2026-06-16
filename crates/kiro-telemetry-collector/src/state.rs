//! State-dir paths, atomic PID-record persistence, advisory file lock,
//! stale-state cleanup, and PID liveness / start-identity helpers.
//!
//! Layout of the state dir (default `$HOME/.kiro/telemetry/collector`):
//!
//! ```text
//! collector/
//!   collector.lock        # advisory exclusive lock
//!   collector.pid.json    # PidRecord (pid, start identity, etc.)
//!   collector.yaml        # last rendered config
//!   collector.fingerprint # last fingerprint hex
//!   collector.stdout.log  # captured stdout (truncated each spawn)
//!   collector.stderr.log  # captured stderr (truncated each spawn)
//!   queue/                # otlphttp sending_queue file_storage
//!   crashes/              # crash-signature drop dir (filelog receiver tail)
//! ```

use std::fs::{
    File,
    OpenOptions,
};
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use fs4::fs_std::FileExt;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    warn,
};

use crate::error::CollectorError;

/// Number of times `acquire_lock` retries on `WouldBlock` before giving up.
const LOCK_TRY_RETRIES: u32 = 3;
const LOCK_TRY_BACKOFF_MS: u64 = 100;

/// Encapsulates the on-disk layout for the per-user collector singleton.
///
/// Use [`StateDir::default`] for the production location
/// (`$HOME/.kiro/telemetry/collector`). [`StateDir::with_root`] is for tests
/// and the eventual config-driven override.
#[derive(Debug, Clone)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    /// Resolve to `$HOME/.kiro/telemetry/collector`. Errors with
    /// [`CollectorError::StateDirCreate`] if `$HOME` is unset.
    ///
    /// Named `default` for ergonomics — note this returns a `Result`, so it
    /// is not a substitute for the `Default` trait (which we cannot
    /// reasonably implement: a missing `$HOME` should not panic and there is
    /// no useful infallible default).
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Result<Self, CollectorError> {
        let home = dirs::home_dir().ok_or_else(|| CollectorError::StateDirCreate {
            path: PathBuf::from("$HOME"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"),
        })?;
        Ok(Self::with_root(home.join(".kiro").join("telemetry").join("collector")))
    }

    /// Construct a state dir rooted at an arbitrary path. Used by tests and
    /// the eventual `KIRO_TELEMETRY_COLLECTOR_STATE_DIR` override.
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join("collector.lock")
    }

    pub fn pid_path(&self) -> PathBuf {
        self.root.join("collector.pid.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("collector.yaml")
    }

    pub fn fingerprint_path(&self) -> PathBuf {
        self.root.join("collector.fingerprint")
    }

    pub fn stdout_log_path(&self) -> PathBuf {
        self.root.join("collector.stdout.log")
    }

    pub fn stderr_log_path(&self) -> PathBuf {
        self.root.join("collector.stderr.log")
    }

    pub fn queue_dir(&self) -> PathBuf {
        self.root.join("queue")
    }

    pub fn crashes_dir(&self) -> PathBuf {
        self.root.join("crashes")
    }

    /// Create the state-dir tree (root + queue + crashes). Idempotent.
    pub fn ensure_dirs(&self) -> Result<(), CollectorError> {
        for p in [&self.root, &self.queue_dir(), &self.crashes_dir()] {
            std::fs::create_dir_all(p).map_err(|source| CollectorError::StateDirCreate {
                path: p.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// Acquire the advisory exclusive lock on `collector.lock`. The returned
    /// guard releases on drop. Retries [`LOCK_TRY_RETRIES`] times with
    /// [`LOCK_TRY_BACKOFF_MS`] backoff before surfacing
    /// [`CollectorError::LockHeld`].
    pub fn acquire_lock(&self) -> Result<LockGuard, CollectorError> {
        self.ensure_dirs()?;
        let path = self.lock_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| CollectorError::LockAcquire {
                path: path.clone(),
                source,
            })?;

        for attempt in 0..=LOCK_TRY_RETRIES {
            match FileExt::try_lock_exclusive(&file) {
                Ok(true) => {
                    return Ok(LockGuard { _file: file, path });
                },
                Ok(false) => {
                    if attempt == LOCK_TRY_RETRIES {
                        return Err(CollectorError::LockHeld { path });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(LOCK_TRY_BACKOFF_MS));
                },
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        if attempt == LOCK_TRY_RETRIES {
                            return Err(CollectorError::LockHeld { path });
                        }
                        std::thread::sleep(std::time::Duration::from_millis(LOCK_TRY_BACKOFF_MS));
                        continue;
                    }
                    return Err(CollectorError::LockAcquire { path, source: e });
                },
            }
        }
        Err(CollectorError::LockHeld { path })
    }

    /// Atomic write: serialize `record` to a `.tmp` sibling and `rename` over
    /// the target so a crashed kiro-cli never leaves a half-written PID file.
    pub fn write_pid_record(&self, record: &PidRecord) -> Result<(), CollectorError> {
        self.ensure_dirs()?;
        let target = self.pid_path();
        let tmp = self.root.join("collector.pid.json.tmp");
        let json = serde_json::to_vec_pretty(record)?;
        let mut f = File::create(&tmp).map_err(|source| CollectorError::ConfigWrite {
            path: tmp.clone(),
            source,
        })?;
        f.write_all(&json).map_err(|source| CollectorError::ConfigWrite {
            path: tmp.clone(),
            source,
        })?;
        f.sync_all().map_err(|source| CollectorError::ConfigWrite {
            path: tmp.clone(),
            source,
        })?;
        drop(f);
        std::fs::rename(&tmp, &target).map_err(|source| CollectorError::ConfigWrite { path: target, source })?;
        Ok(())
    }

    /// `Ok(None)` if the file is missing or unparseable. Parse errors are
    /// logged at `warn!` so a corrupt PID file results in a clean respawn
    /// rather than a hard error.
    pub fn read_pid_record(&self) -> Result<Option<PidRecord>, CollectorError> {
        let p = self.pid_path();
        let buf = match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                warn!(error = %e, path = %p.display(), "failed reading pid record; treating as absent");
                return Ok(None);
            },
        };
        match serde_json::from_str::<PidRecord>(&buf) {
            Ok(rec) => Ok(Some(rec)),
            Err(e) => {
                warn!(error = %e, path = %p.display(), "malformed pid record; treating as absent");
                Ok(None)
            },
        }
    }

    /// Remove all transient state (PID file, fingerprint, last config). Used
    /// after a stale-PID detection or `stop()`. Logs (but does not fail) on
    /// individual unlink errors.
    pub fn cleanup_stale(&self) -> Result<(), CollectorError> {
        for p in [self.pid_path(), self.fingerprint_path(), self.config_path()] {
            match std::fs::remove_file(&p) {
                Ok(()) => debug!(path = %p.display(), "cleanup_stale: removed"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => warn!(error = %e, path = %p.display(), "cleanup_stale: failed"),
            }
        }
        Ok(())
    }
}

/// PID-record persisted to `collector.pid.json` after a successful spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PidRecord {
    /// OS PID of the spawned `otelcol-contrib`.
    pub pid: u32,
    /// Platform-specific start-identity used to detect PID recycling. See
    /// [`process_start_identity`]. `0` on platforms where unsupported (Windows
    /// MVP), in which case liveness falls back to `is_pid_alive` + the health
    /// probe.
    pub start_identity: u64,
    /// Wall-clock time we spawned at, in unix-epoch milliseconds. For
    /// debugging only — not load-bearing.
    pub spawned_at_unix_ms: u64,
    /// Upstream OTLP endpoint the collector was started with. Surfaced by
    /// `status()`.
    pub upstream_endpoint: String,
    /// Path to the `otelcol-contrib` binary that was spawned. Used by the
    /// fingerprint comparison so a new binary forces a respawn.
    pub binary_path: PathBuf,
}

impl PidRecord {
    /// Build a record for the current moment given a spawned child's PID.
    pub fn new(pid: u32, upstream_endpoint: String, binary_path: PathBuf) -> Self {
        let start_identity = process_start_identity(pid).unwrap_or(0);
        let spawned_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            pid,
            start_identity,
            spawned_at_unix_ms,
            upstream_endpoint,
            binary_path,
        }
    }
}

/// Drops the advisory file lock when this guard goes out of scope.
#[derive(Debug)]
pub struct LockGuard {
    _file: File,
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        debug!(path = %self.path.display(), "releasing collector state lock");
        // The OS releases the flock(2) when the file descriptor is closed,
        // which happens via `_file`'s Drop. Nothing else to do.
    }
}

// ---------------------------------------------------------------------------
// Platform-gated process-info helpers.
// ---------------------------------------------------------------------------

/// Best-effort liveness check for an arbitrary PID.
///
/// On unix this issues `kill(pid, 0)` which returns success if the caller has
/// permission to signal the process and the PID is alive. ESRCH means the
/// process is gone. On Windows we conservatively return `true` and let the
/// health probe + start-identity be the real liveness check (the MVP doesn't
/// implement `OpenProcess`-based detection).
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        // EPERM means it exists but we can't signal it — still alive.
        Err(nix::errno::Errno::EPERM) => true,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    // Windows MVP — see module doc.
    true
}

/// Platform-specific stable identity for a running process, derived from its
/// kernel-tracked start time. Used to catch PID recycling: if a recorded PID
/// is reused by an unrelated process between kiro-cli invocations, the start
/// identity will differ and we'll treat the recorded entry as stale.
///
/// Returns `Ok(0)` on platforms where this is not implemented (Windows MVP).
#[cfg(target_os = "macos")]
pub fn process_start_identity(pid: u32) -> Result<u64, std::io::Error> {
    use std::mem;

    // proc_pidinfo(PROC_PIDTBSDINFO) -> struct proc_bsdinfo. Fields
    // pbi_start_tvsec/pbi_start_tvusec are the kernel-tracked process start
    // time. Pack them into a single u64 = sec * 1_000_000 + usec which is
    // unique per-process for the lifetime of the system.
    let mut info: libc::proc_bsdinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let res = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            ((&mut info) as *mut libc::proc_bsdinfo).cast::<libc::c_void>(),
            size,
        )
    };
    if res <= 0 {
        // proc_pidinfo returns 0 on most failures; check errno.
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            // Process is gone — caller treats 0 as "unverifiable".
            return Ok(0);
        }
        return Err(err);
    }
    let sec = info.pbi_start_tvsec;
    let usec = info.pbi_start_tvusec;
    Ok(sec.saturating_mul(1_000_000).saturating_add(usec))
}

#[cfg(target_os = "linux")]
pub fn process_start_identity(pid: u32) -> Result<u64, std::io::Error> {
    // /proc/<pid>/stat: field 22 is starttime in clock ticks since boot.
    // The line format is `pid (comm) state ppid ...`. The `(comm)` may
    // contain spaces and parentheses, so we slice from the last ')'.
    let path = format!("/proc/{pid}/stat");
    let buf = std::fs::read_to_string(&path)?;
    let last_paren = buf
        .rfind(')')
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no ')' in stat"))?;
    let after = buf
        .get(last_paren + 1..)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "stat line too short"))?
        .trim();
    let fields: Vec<&str> = after.split_whitespace().collect();
    // After `pid (comm)`, the next field is `state` (index 0 in `fields`).
    // `starttime` is the 22nd field overall, i.e. `fields[19]` (state=0,
    // ppid=1, ..., starttime=19).
    let starttime = fields
        .get(19)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no starttime in stat"))?;
    starttime
        .parse::<u64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn process_start_identity(_pid: u32) -> Result<u64, std::io::Error> {
    // Windows MVP: unsupported. Returns 0 — `start_identity == 0` is
    // documented to mean "unverifiable".
    Ok(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_state() -> (StateDir, tempfile::TempDir) {
        let td = tempfile::tempdir().expect("tempdir");
        let s = StateDir::with_root(td.path().to_path_buf());
        (s, td)
    }

    #[test]
    fn paths_under_root() {
        let (s, td) = tmp_state();
        let r = td.path();
        assert_eq!(s.lock_path(), r.join("collector.lock"));
        assert_eq!(s.pid_path(), r.join("collector.pid.json"));
        assert_eq!(s.config_path(), r.join("collector.yaml"));
        assert_eq!(s.fingerprint_path(), r.join("collector.fingerprint"));
        assert_eq!(s.queue_dir(), r.join("queue"));
        assert_eq!(s.crashes_dir(), r.join("crashes"));
    }

    #[test]
    fn ensure_dirs_creates_tree() {
        let (s, _td) = tmp_state();
        s.ensure_dirs().expect("ensure_dirs");
        assert!(s.root().is_dir());
        assert!(s.queue_dir().is_dir());
        assert!(s.crashes_dir().is_dir());
    }

    #[test]
    fn pid_record_roundtrip() {
        let (s, _td) = tmp_state();
        let rec = PidRecord {
            pid: 4242,
            start_identity: 1234567890,
            spawned_at_unix_ms: 1_700_000_000_000,
            upstream_endpoint: "https://upstream.example/v1/metrics".to_string(),
            binary_path: PathBuf::from("/usr/local/bin/otelcol-contrib"),
        };
        s.write_pid_record(&rec).expect("write");
        let got = s.read_pid_record().expect("read").expect("present");
        assert_eq!(rec, got);
    }

    #[test]
    fn read_pid_record_missing_returns_none() {
        let (s, _td) = tmp_state();
        s.ensure_dirs().expect("ensure_dirs");
        assert!(s.read_pid_record().expect("ok").is_none());
    }

    #[test]
    fn read_pid_record_malformed_returns_none() {
        let (s, _td) = tmp_state();
        s.ensure_dirs().expect("ensure_dirs");
        std::fs::write(s.pid_path(), b"not json").expect("write garbage");
        assert!(s.read_pid_record().expect("ok").is_none());
    }

    #[test]
    fn acquire_lock_held_by_second_attempt() {
        let (s, _td) = tmp_state();
        let _g = s.acquire_lock().expect("first");
        let err = s.acquire_lock().expect_err("second should fail");
        assert!(matches!(err, CollectorError::LockHeld { .. }), "got: {err:?}");
    }

    #[test]
    fn acquire_lock_releases_on_drop() {
        let (s, _td) = tmp_state();
        {
            let _g = s.acquire_lock().expect("first");
        }
        let _g2 = s.acquire_lock().expect("second after drop");
    }

    #[cfg(unix)]
    #[test]
    fn pid_alive_for_self() {
        let me = std::process::id();
        assert!(is_pid_alive(me));
    }

    #[cfg(unix)]
    #[test]
    fn pid_alive_false_for_huge() {
        // 2^31-2 — vanishingly likely to exist as a PID.
        assert!(!is_pid_alive(2_147_483_646));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn start_identity_self_nonzero_and_stable() {
        let me = std::process::id();
        let a = process_start_identity(me).expect("identity 1");
        let b = process_start_identity(me).expect("identity 2");
        assert_ne!(a, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn cleanup_stale_idempotent() {
        let (s, _td) = tmp_state();
        s.ensure_dirs().expect("ensure_dirs");
        // No files yet — should not error.
        s.cleanup_stale().expect("first cleanup");
        std::fs::write(s.pid_path(), b"{}").expect("seed pid");
        std::fs::write(s.fingerprint_path(), b"deadbeef").expect("seed fp");
        std::fs::write(s.config_path(), b"yaml").expect("seed yaml");
        s.cleanup_stale().expect("second cleanup");
        assert!(!s.pid_path().exists());
        assert!(!s.fingerprint_path().exists());
        assert!(!s.config_path().exists());
    }
}
