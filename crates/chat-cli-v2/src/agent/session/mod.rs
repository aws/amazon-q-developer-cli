//! Session persistence for CLI conversations.
//!
//! Sessions are stored as:
//! - `{session_id}.json` - metadata (cwd, timestamps, session state)
//! - `{session_id}.jsonl` - append-only log entries
//! - `{session_id}.lock` - lock file (exists only when session is active)

use std::fs::{
    self,
    File,
    OpenOptions,
};
use std::io::{
    self,
    BufRead,
    BufReader,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Mutex;

use agent::event_log::LogEntry;
use agent::types::ConversationMetadata;
use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tracing::warn;

use crate::agent::rts::RtsStateSnapshot;
use crate::util::paths::sessions_dir;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Session is active in another process (PID {pid})")]
    ActiveSession { pid: u32, started_at: DateTime<Utc> },
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("{context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("Path error: {0}")]
    Path(#[from] crate::util::paths::DirectoryError),
}

impl SessionError {
    fn io(source: io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    fn json(source: serde_json::Error, context: impl Into<String>) -> Self {
        Self::Json {
            context: context.into(),
            source,
        }
    }
}

/// Versioned session state for forward compatibility.
use agent::permissions::RuntimePermissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum SessionState {
    #[serde(rename = "v1")]
    V1(SessionStateV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateV1 {
    pub conversation_metadata: ConversationMetadata,
    pub rts_model_state: RtsStateSnapshot,
    #[serde(default)]
    pub permissions: RuntimePermissions,
}

impl SessionState {
    pub fn new(
        conversation_metadata: ConversationMetadata,
        rts_model_state: RtsStateSnapshot,
        permissions: RuntimePermissions,
    ) -> Self {
        Self::V1(SessionStateV1 {
            conversation_metadata,
            rts_model_state,
            permissions,
        })
    }

    pub fn conversation_metadata(&self) -> &ConversationMetadata {
        match self {
            Self::V1(v1) => &v1.conversation_metadata,
        }
    }

    pub fn rts_model_state(&self) -> &RtsStateSnapshot {
        match self {
            Self::V1(v1) => &v1.rts_model_state,
        }
    }

    pub fn permissions(&self) -> &RuntimePermissions {
        match self {
            Self::V1(v1) => &v1.permissions,
        }
    }
}

/// Session metadata stored in `{session_id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Unique identifier for this session.
    pub session_id: String,
    /// The ACP working directory used when this session was created.
    pub cwd: PathBuf,
    /// Timestamp when the session was first created.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the last update to session metadata.
    pub updated_at: DateTime<Utc>,
    /// Serialized conversation and model state.
    pub session_state: SessionState,
}

/// Lock file contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionLock {
    pid: u32,
    started_at: DateTime<Utc>,
}

/// RAII guard that releases the session lock on drop.
#[derive(Debug)]
pub struct SessionLockGuard {
    lock_path: PathBuf,
}

impl Drop for SessionLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn lock_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir.join(format!("{}.lock", session_id))
}

fn metadata_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir.join(format!("{}.json", session_id))
}

fn log_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir.join(format!("{}.jsonl", session_id))
}

#[allow(dead_code)]
fn acquire_lock(sessions_dir: &Path, session_id: &str) -> Result<SessionLockGuard, SessionError> {
    acquire_lock_impl(&lock_path(sessions_dir, session_id), is_pid_alive, std::process::id())
}

/// Attempt to acquire an exclusive lock for a session.
///
/// Uses atomic file creation (`O_CREAT | O_EXCL`) to prevent race conditions:
/// - If the lock file doesn't exist, create it atomically and write our PID
/// - If it exists, check if the owning process is still alive
/// - If the owner is dead (stale lock), remove it and retry once
///
/// The `is_pid_alive` parameter allows injecting a mock for testing.
fn acquire_lock_impl(
    lock_path: &Path,
    is_pid_alive: impl Fn(u32) -> bool,
    current_pid: u32,
) -> Result<SessionLockGuard, SessionError> {
    // Try atomic create - fails with AlreadyExists if lock file exists
    match OpenOptions::new().write(true).create_new(true).open(lock_path) {
        Ok(mut file) => {
            // Lock acquired - write our PID so others can check if we're alive
            let lock = SessionLock {
                pid: current_pid,
                started_at: Utc::now(),
            };
            serde_json::to_writer(&mut file, &lock)
                .map_err(|e| SessionError::json(e, format!("failed to write lock file {:?}", lock_path)))?;
            file.flush()
                .map_err(|e| SessionError::io(e, format!("failed to flush lock file {:?}", lock_path)))?;
            Ok(SessionLockGuard {
                lock_path: lock_path.to_path_buf(),
            })
        },
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            // Lock file exists - check if the owning process is still alive
            let content = fs::read_to_string(lock_path)
                .map_err(|e| SessionError::io(e, format!("failed to read lock file {:?}", lock_path)))?;
            let lock: SessionLock = serde_json::from_str(&content)
                .map_err(|e| SessionError::json(e, format!("failed to parse lock file {:?}", lock_path)))?;

            // Same process can re-acquire its own lock
            if lock.pid == current_pid {
                return Ok(SessionLockGuard {
                    lock_path: lock_path.to_path_buf(),
                });
            }

            if is_pid_alive(lock.pid) {
                // Process is still running - session is genuinely locked
                return Err(SessionError::ActiveSession {
                    pid: lock.pid,
                    started_at: lock.started_at,
                });
            }

            // Stale lock (process died without cleanup) - remove and retry once
            fs::remove_file(lock_path)
                .map_err(|e| SessionError::io(e, format!("failed to remove stale lock file {:?}", lock_path)))?;

            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(lock_path)
                .map_err(|e| SessionError::io(e, format!("failed to create lock file {:?}", lock_path)))?;

            let new_lock = SessionLock {
                pid: current_pid,
                started_at: Utc::now(),
            };
            serde_json::to_writer(&mut file, &new_lock)
                .map_err(|e| SessionError::json(e, format!("failed to write lock file {:?}", lock_path)))?;
            file.flush()
                .map_err(|e| SessionError::io(e, format!("failed to flush lock file {:?}", lock_path)))?;

            Ok(SessionLockGuard {
                lock_path: lock_path.to_path_buf(),
            })
        },
        Err(e) => Err(SessionError::io(
            e,
            format!("failed to create lock file {:?}", lock_path),
        )),
    }
}

/// Check if a process is still running.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle,
        STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess,
        OpenProcess,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        let mut exit_code: u32 = 0;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        result != 0 && exit_code == STILL_ACTIVE
    }
}

/// Atomically write content to a file using write-to-temp-then-rename pattern.
///
/// On POSIX systems, rename() is atomic - the file either has old or new content, never partial.
/// On Windows, rename can fail if the destination is open by another process, but our advisory
/// locking ensures only one process accesses a session's files at a time.
fn atomic_write(path: &Path, content: &str) -> Result<(), SessionError> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content)
        .map_err(|e| SessionError::io(e, format!("failed to write temp file {:?}", tmp_path)))?;
    fs::rename(&tmp_path, path)
        .map_err(|e| SessionError::io(e, format!("failed to rename {:?} to {:?}", tmp_path, path)))?;
    Ok(())
}

/// Handle to a session stored on disk.
///
/// Uses interior mutability for ergonomic `&self` API across async task boundaries.
pub struct SessionDb {
    _lock_guard: SessionLockGuard,
    session: Mutex<SessionData>,
    sessions_dir: PathBuf,
}

static_assertions::assert_impl_all!(SessionDb: Send, Sync);

impl std::fmt::Debug for SessionDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionDb")
            .field("sessions_dir", &self.sessions_dir)
            .finish_non_exhaustive()
    }
}

impl SessionDb {
    /// Create a new session.
    pub fn new(session_id: String, cwd: &Path, state: SessionState) -> Result<Self, SessionError> {
        Self::new_impl(
            &sessions_dir()?,
            session_id,
            cwd,
            state,
            is_pid_alive,
            std::process::id(),
        )
    }

    fn new_impl(
        sessions_dir: &Path,
        session_id: String,
        cwd: &Path,
        state: SessionState,
        is_pid_alive: impl Fn(u32) -> bool,
        current_pid: u32,
    ) -> Result<Self, SessionError> {
        fs::create_dir_all(sessions_dir)
            .map_err(|e| SessionError::io(e, format!("failed to create sessions directory {:?}", sessions_dir)))?;

        let lock_guard = acquire_lock_impl(&lock_path(sessions_dir, &session_id), is_pid_alive, current_pid)?;

        let now = Utc::now();
        let session = SessionData {
            session_id: session_id.clone(),
            cwd: cwd.to_path_buf(),
            created_at: now,
            updated_at: now,
            session_state: state,
        };

        let meta_path = metadata_path(sessions_dir, &session_id);
        let content = serde_json::to_string_pretty(&session)
            .map_err(|e| SessionError::json(e, format!("failed to serialize session {:?}", session_id)))?;
        atomic_write(&meta_path, &content)?;

        File::create(log_path(sessions_dir, &session_id))
            .map_err(|e| SessionError::io(e, format!("failed to create log file for session {:?}", session_id)))?;

        Ok(Self {
            _lock_guard: lock_guard,
            session: Mutex::new(session),
            sessions_dir: sessions_dir.to_path_buf(),
        })
    }

    /// Load an existing session, optionally updating the cwd.
    pub fn load(session_id: &str, cwd: Option<&Path>) -> Result<Self, SessionError> {
        Self::load_impl(&sessions_dir()?, session_id, cwd, is_pid_alive, std::process::id())
    }

    fn load_impl(
        sessions_dir: &Path,
        session_id: &str,
        cwd: Option<&Path>,
        is_pid_alive: impl Fn(u32) -> bool,
        current_pid: u32,
    ) -> Result<Self, SessionError> {
        let meta_path = metadata_path(sessions_dir, session_id);
        if !meta_path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        let lock_guard = acquire_lock_impl(&lock_path(sessions_dir, session_id), is_pid_alive, current_pid)?;

        let content = fs::read_to_string(&meta_path)
            .map_err(|e| SessionError::io(e, format!("failed to read session metadata {:?}", meta_path)))?;
        let mut session: SessionData = serde_json::from_str(&content)
            .map_err(|e| SessionError::json(e, format!("failed to parse session metadata {:?}", meta_path)))?;

        // Update cwd if provided
        if let Some(new_cwd) = cwd {
            session.cwd = new_cwd.to_path_buf();
        }

        Ok(Self {
            _lock_guard: lock_guard,
            session: Mutex::new(session),
            sessions_dir: sessions_dir.to_path_buf(),
        })
    }

    pub fn session_id(&self) -> String {
        self.session.lock().unwrap().session_id.clone()
    }

    pub fn session(&self) -> SessionData {
        self.session.lock().unwrap().clone()
    }

    /// Append a log entry to the session's JSONL file.
    pub fn append_log_entry(&self, entry: &LogEntry) -> Result<(), SessionError> {
        let session_id = self.session.lock().unwrap().session_id.clone();
        let path = log_path(&self.sessions_dir, &session_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError::io(e, format!("failed to open log file {:?}", path)))?;

        let mut line = serde_json::to_string(entry).map_err(|e| {
            SessionError::json(e, format!("failed to serialize log entry for session {:?}", session_id))
        })?;
        line.push('\n');

        file.write_all(line.as_bytes())
            .map_err(|e| SessionError::io(e, format!("failed to write to log file {:?}", path)))?;
        Ok(())
    }

    /// Update session state (e.g., on EndTurn).
    pub fn update_state(&self, state: SessionState) -> Result<(), SessionError> {
        let mut session = self.session.lock().unwrap();
        session.session_state = state;
        session.updated_at = Utc::now();

        let meta_path = metadata_path(&self.sessions_dir, &session.session_id);
        let content = serde_json::to_string_pretty(&*session)
            .map_err(|e| SessionError::json(e, format!("failed to serialize session {:?}", session.session_id)))?;
        atomic_write(&meta_path, &content)?;
        Ok(())
    }

    /// Load all log entries from the session's JSONL file.
    pub fn load_log_entries(&self) -> Result<Vec<LogEntry>, SessionError> {
        let session_id = self.session.lock().unwrap().session_id.clone();
        let path = log_path(&self.sessions_dir, &session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path).map_err(|e| SessionError::io(e, format!("failed to open log file {:?}", path)))?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line =
                line.map_err(|e| SessionError::io(e, format!("failed to read line {} from {:?}", i + 1, path)))?;
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str(&line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    warn!("Skipping malformed log entry at line {} in {:?}: {}", i + 1, path, e);
                },
            }
        }

        Ok(entries)
    }
}

/// List all sessions for a given working directory.
pub fn list_sessions_by_cwd(cwd: &Path) -> Result<Vec<SessionData>, SessionError> {
    list_sessions_by_cwd_impl(&sessions_dir()?, cwd)
}

fn list_sessions_by_cwd_impl(sessions_dir: &Path, cwd: &Path) -> Result<Vec<SessionData>, SessionError> {
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let mut sessions = Vec::new();

    let read_dir = fs::read_dir(sessions_dir)
        .map_err(|e| SessionError::io(e, format!("failed to read sessions directory {:?}", sessions_dir)))?;

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let session: SessionData = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let session_cwd = session.cwd.canonicalize().unwrap_or_else(|_| session.cwd.clone());
            if session_cwd == canonical_cwd {
                sessions.push(session);
            }
        }
    }

    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use agent::types::ConversationMetadata;
    use tempfile::TempDir;

    use super::*;
    use crate::agent::rts::RtsStateSnapshot;

    fn test_state() -> SessionState {
        SessionState::new(
            ConversationMetadata::default(),
            RtsStateSnapshot {
                conversation_id: "test-session".to_string(),
                model_info: None,
                context_usage_percentage: None,
            },
            RuntimePermissions::default(),
        )
    }

    fn pid_always_dead(_pid: u32) -> bool {
        false
    }

    fn pid_always_alive(_pid: u32) -> bool {
        true
    }

    #[test]
    fn test_session_state_serialization_roundtrip() {
        let state = test_state();
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains(r#""version":"v1""#));
        let parsed: SessionState = serde_json::from_str(&json).unwrap();
        match parsed {
            SessionState::V1(_) => {},
        }
    }

    #[test]
    fn test_create_and_load_session() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "test-session-1".to_string();

        let handle = SessionDb::new_impl(
            sessions_dir,
            session_id.clone(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();

        assert!(metadata_path(sessions_dir, &session_id).exists());
        assert!(log_path(sessions_dir, &session_id).exists());
        assert!(lock_path(sessions_dir, &session_id).exists());

        drop(handle);

        // Lock should be released
        assert!(!lock_path(sessions_dir, &session_id).exists());

        // Should be able to load
        let loaded = SessionDb::load_impl(sessions_dir, &session_id, None, pid_always_dead, 1000).unwrap();
        assert_eq!(loaded.session().cwd, cwd);
    }

    #[test]
    fn test_load_with_new_cwd() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let original_cwd = Path::new("/original/project");
        let new_cwd = Path::new("/new/project");
        let session_id = "test-session-cwd".to_string();

        let handle = SessionDb::new_impl(
            sessions_dir,
            session_id.clone(),
            original_cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        drop(handle);

        let loaded = SessionDb::load_impl(sessions_dir, &session_id, Some(new_cwd), pid_always_dead, 1000).unwrap();
        assert_eq!(loaded.session().cwd, new_cwd);
    }

    #[test]
    fn test_lock_prevents_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "test-session-lock".to_string();

        // Create session with PID 1000
        let _handle = SessionDb::new_impl(
            sessions_dir,
            session_id.clone(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();

        // Try to load same session as PID 2000 with "alive" PID check - should fail
        let result = SessionDb::load_impl(sessions_dir, &session_id, None, pid_always_alive, 2000);
        assert!(matches!(result, Err(SessionError::ActiveSession { .. })));
    }

    #[test]
    fn test_stale_lock_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        fs::create_dir_all(sessions_dir).unwrap();

        let lock_file = sessions_dir.join("test.lock");
        let stale_lock = SessionLock {
            pid: 99999,
            started_at: Utc::now(),
        };
        fs::write(&lock_file, serde_json::to_string(&stale_lock).unwrap()).unwrap();

        // Should succeed by cleaning up stale lock (is_pid_alive returns false)
        let guard = acquire_lock_impl(&lock_file, pid_always_dead, 12345).unwrap();
        assert!(lock_file.exists());
        drop(guard);
    }

    #[test]
    fn test_append_and_load_log_entries() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "test-session-log".to_string();

        let handle = SessionDb::new_impl(sessions_dir, session_id, cwd, test_state(), pid_always_dead, 1000).unwrap();

        let entry1 = LogEntry::prompt("msg-1".to_string(), vec![]);
        let entry2 = LogEntry::prompt("msg-2".to_string(), vec![]);

        handle.append_log_entry(&entry1).unwrap();
        handle.append_log_entry(&entry2).unwrap();

        let entries = handle.load_log_entries().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_malformed_log_entry_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "test-session-malformed".to_string();

        let handle = SessionDb::new_impl(
            sessions_dir,
            session_id.clone(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        let log_file = log_path(sessions_dir, &session_id);

        // Write valid entry, malformed line, valid entry
        let entry = LogEntry::prompt("msg-1".to_string(), vec![]);
        let mut content = serde_json::to_string(&entry).unwrap();
        content.push('\n');
        content.push_str("not valid json\n");
        content.push_str(&serde_json::to_string(&entry).unwrap());
        content.push('\n');
        fs::write(&log_file, content).unwrap();

        let entries = handle.load_log_entries().unwrap();
        assert_eq!(entries.len(), 2); // Malformed line skipped
    }

    #[test]
    fn test_list_sessions_by_cwd() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();

        let cwd1 = temp_dir.path().join("project1");
        let cwd2 = temp_dir.path().join("project2");
        fs::create_dir_all(&cwd1).unwrap();
        fs::create_dir_all(&cwd2).unwrap();

        // Create sessions for different cwds
        let s1 = SessionDb::new_impl(
            sessions_dir,
            "s1".to_string(),
            &cwd1,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        drop(s1);
        let s2 = SessionDb::new_impl(
            sessions_dir,
            "s2".to_string(),
            &cwd1,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        drop(s2);
        let s3 = SessionDb::new_impl(
            sessions_dir,
            "s3".to_string(),
            &cwd2,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        drop(s3);

        let cwd1_sessions = list_sessions_by_cwd_impl(sessions_dir, &cwd1).unwrap();
        assert_eq!(cwd1_sessions.len(), 2);

        let cwd2_sessions = list_sessions_by_cwd_impl(sessions_dir, &cwd2).unwrap();
        assert_eq!(cwd2_sessions.len(), 1);
    }
}
