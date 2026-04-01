//! Session persistence for CLI conversations.
//!
//! Sessions are stored as:
//! - `{session_id}.json` - metadata (cwd, timestamps, session state)
//! - `{session_id}.jsonl` - append-only log entries
//! - `{session_id}.lock` - lock file (exists only when session is active)

pub mod v1_compat;

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
use agent::permissions::RuntimePermissions;
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

/// Versioned session metadata. Contains state about the conversation that is subject to change
/// overtime, e.g. conversation metadata, saved runtime permissions, etc.
///
/// Wraps the concrete state struct in a tagged enum so that older persisted
/// sessions can still be loaded when the schema evolves. The `Unknown`
/// variant acts as a catch-all: if a session file contains an unrecognized
/// version tag (or corrupt JSON for the state portion), serde will
/// deserialize it as `Unknown` instead of failing outright.
///
/// On [`SessionDb::load`], `Unknown` is replaced with a default value of `SessionStateV1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "version")]
pub enum SessionState {
    #[serde(rename = "v1")]
    V1(SessionStateV1),
    /// Catch-all for unrecognized or corrupt session state versions.
    #[serde(other)]
    Unknown,
}

/// Versioned session metadata. See [`SessionState`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateV1 {
    /// Metadata about the conversation history.
    pub conversation_metadata: ConversationMetadata,
    pub rts_model_state: RtsStateSnapshot,
    #[serde(default)]
    pub permissions: RuntimePermissions,
    #[serde(default)]
    pub agent_name: Option<String>,
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
            agent_name: None,
        })
    }

    pub fn conversation_metadata(&self) -> Option<&ConversationMetadata> {
        match self {
            Self::V1(v1) => Some(&v1.conversation_metadata),
            Self::Unknown => None,
        }
    }

    pub fn rts_model_state(&self) -> Option<&RtsStateSnapshot> {
        match self {
            Self::V1(v1) => Some(&v1.rts_model_state),
            Self::Unknown => None,
        }
    }

    pub fn permissions(&self) -> Option<&RuntimePermissions> {
        match self {
            Self::V1(v1) => Some(&v1.permissions),
            Self::Unknown => None,
        }
    }

    pub fn agent_name(&self) -> Option<&str> {
        match self {
            Self::V1(v1) => v1.agent_name.as_deref(),
            Self::Unknown => None,
        }
    }

    pub fn set_agent_name(&mut self, name: String) {
        match self {
            Self::V1(v1) => v1.agent_name = Some(name),
            Self::Unknown => {},
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
    /// Human-readable title derived from the first user prompt.
    #[serde(default)]
    pub title: Option<String>,
    /// Whether this session was exported from a V1 conversation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exported_from_v1: bool,
    /// Absolute path to the file this session was imported from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
    /// Serialized conversation and model state.
    #[serde(deserialize_with = "deserialize_session_state")]
    pub session_state: SessionState,
}

/// Deserialize `SessionState`, falling back to `Unknown` if the payload is
/// corrupt or has incompatible fields. `#[serde(other)]` only catches
/// unrecognized version tags; this also handles a recognized tag (e.g. `"v1"`)
/// whose inner fields fail to deserialize.
fn deserialize_session_state<'de, D>(deserializer: D) -> Result<SessionState, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(SessionState::deserialize(value).unwrap_or(SessionState::Unknown))
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

pub fn metadata_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir.join(format!("{}.json", session_id))
}

pub fn log_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir.join(format!("{}.jsonl", session_id))
}

pub fn acquire_lock(sessions_dir: &Path, session_id: &str) -> Result<SessionLockGuard, SessionError> {
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

/// Check if a process is still running AND belongs to a kiro binary.
///
/// Guards against PID reuse: if the OS recycled the PID to an unrelated
/// process we treat the lock as stale.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    use sysinfo::{
        Pid,
        ProcessesToUpdate,
        System,
    };

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
    system.process(pid).is_some_and(|proc_| {
        let name = proc_.name().to_string_lossy();
        name.starts_with(crate::util::CLI_BINARY_NAME)
            // "chat_cli" is the binary name for cargo debug builds
            || name.starts_with("chat_cli")
    })
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
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        result != 0 && exit_code == STILL_ACTIVE as u32
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

impl Drop for SessionDb {
    fn drop(&mut self) {
        let Some(session) = self.session.lock().ok() else {
            return;
        };
        let log = log_path(&self.sessions_dir, &session.session_id);
        let is_empty = fs::metadata(&log).map_or(true, |m| m.len() == 0);
        if is_empty {
            tracing::debug!(session_id = %session.session_id, "cleaning up empty session");
            let _ = fs::remove_file(metadata_path(&self.sessions_dir, &session.session_id));
            let _ = fs::remove_file(&log);
        }
    }
}

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
            title: None,
            exported_from_v1: false,
            imported_from: None,
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

    /// Load a session from a specific sessions directory.
    pub fn load_with_sessions_dir(
        sessions_dir: &Path,
        session_id: &str,
        cwd: Option<&Path>,
    ) -> Result<Self, SessionError> {
        Self::load_impl(sessions_dir, session_id, cwd, is_pid_alive, std::process::id())
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

        // Replace unrecognized/corrupt session state with a fresh V1 state.
        // The event log is the source of truth for conversation history.
        if matches!(session.session_state, SessionState::Unknown) {
            session.session_state = SessionState::V1(SessionStateV1 {
                conversation_metadata: ConversationMetadata::default(),
                rts_model_state: RtsStateSnapshot {
                    conversation_id: session.session_id.clone(),
                    model_info: None,
                    context_usage_percentage: None,
                },
                permissions: RuntimePermissions::default(),
                agent_name: None,
            });
        }

        // Update cwd if provided
        let mut dirty = false;
        if let Some(new_cwd) = cwd
            && session.cwd != new_cwd
        {
            session.cwd = new_cwd.to_path_buf();
            dirty = true;
        }

        // Backfill title from first log entry if missing
        if session.title.is_none()
            && let Some(title) = title_from_first_log_entry(sessions_dir, session_id)
        {
            session.title = Some(title);
            dirty = true;
        }

        if dirty {
            let content = serde_json::to_string_pretty(&session)
                .map_err(|e| SessionError::json(e, format!("failed to serialize session {:?}", session_id)))?;
            atomic_write(&meta_path, &content)?;
        }

        Ok(Self {
            _lock_guard: lock_guard,
            session: Mutex::new(session),
            sessions_dir: sessions_dir.to_path_buf(),
        })
    }

    pub fn session_id(&self) -> String {
        self.session.lock().expect("session mutex poisoned").session_id.clone()
    }

    pub fn session(&self) -> SessionData {
        self.session.lock().expect("session mutex poisoned").clone()
    }

    /// Append a log entry to the session's JSONL file.
    pub fn append_log_entry(&self, entry: &LogEntry) -> Result<(), SessionError> {
        let session_id = self.session.lock().expect("session mutex poisoned").session_id.clone();
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
        let mut session = self.session.lock().expect("session mutex poisoned");
        session.session_state = state;
        session.updated_at = Utc::now();

        let meta_path = metadata_path(&self.sessions_dir, &session.session_id);
        let content = serde_json::to_string_pretty(&*session)
            .map_err(|e| SessionError::json(e, format!("failed to serialize session {:?}", session.session_id)))?;
        atomic_write(&meta_path, &content)?;
        Ok(())
    }

    /// Set a human-readable title for the session (typically from the first user prompt).
    pub fn set_title(&self, title: String) -> Result<(), SessionError> {
        let mut session = self.session.lock().expect("session mutex poisoned");
        session.title = Some(title);
        session.updated_at = Utc::now();

        let meta_path = metadata_path(&self.sessions_dir, &session.session_id);
        let content = serde_json::to_string_pretty(&*session)
            .map_err(|e| SessionError::json(e, format!("failed to serialize session {:?}", session.session_id)))?;
        atomic_write(&meta_path, &content)?;
        Ok(())
    }

    /// Load all log entries from the session's JSONL file.
    pub fn load_log_entries(&self) -> Result<Vec<LogEntry>, SessionError> {
        let session_id = self.session.lock().expect("session mutex poisoned").session_id.clone();
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

/// Create a human-readable session title from prompt content blocks.
///
/// Takes the first text block, collapses to a single line, and truncates to 30 characters.
pub fn create_session_title(content: &[agent::agent_loop::types::ContentBlock]) -> Option<String> {
    const MAX_TITLE_LEN: usize = 30;
    const SUFFIX: &str = "...";

    let text = content.iter().find_map(|b| b.text())?;
    let single_line = text.lines().next().unwrap_or(text).trim();

    if single_line.is_empty() {
        return None;
    }
    if single_line.len() <= MAX_TITLE_LEN {
        return Some(single_line.to_string());
    }
    let truncated = agent::util::truncate_safe(single_line, MAX_TITLE_LEN - SUFFIX.len());
    Some(format!("{}{}", truncated, SUFFIX))
}

/// Check whether a session has any log entries (i.e. at least one prompt was sent).
pub fn has_log_entries(sessions_dir: &Path, session_id: &str) -> bool {
    fs::metadata(log_path(sessions_dir, session_id)).is_ok_and(|m| m.len() > 0)
}

/// Check whether a session exists on disk (both metadata and log files present).
pub fn session_exists(sessions_dir: &Path, session_id: &str) -> bool {
    metadata_path(sessions_dir, session_id).exists() && log_path(sessions_dir, session_id).exists()
}

/// Derive a title from the first log entry of a session.
///
/// Handles both `Prompt` (normal sessions) and `Compaction` (imported V1 sessions
/// that were compacted) by extracting the first user message content.
pub fn title_from_first_log_entry(sessions_dir: &Path, session_id: &str) -> Option<String> {
    let file = File::open(log_path(sessions_dir, session_id)).ok()?;
    let first_line = BufReader::new(file).lines().next()?.ok()?;
    let entry: LogEntry = serde_json::from_str(&first_line).ok()?;
    match entry {
        LogEntry::V1(agent::event_log::LogEntryV1::Prompt { content, .. }) => create_session_title(&content),
        LogEntry::V1(agent::event_log::LogEntryV1::Compaction { messages_snapshot, .. }) => {
            let first_user = messages_snapshot
                .iter()
                .find(|m| m.role == agent::agent_loop::types::Role::User)?;
            create_session_title(&first_user.content)
        },
        LogEntry::V1(_) => None,
    }
}

/// Read the persisted agent name from session metadata without loading the full session.
pub fn peek_agent_name(sessions_dir: &Path, session_id: &str) -> Option<String> {
    let path = metadata_path(sessions_dir, session_id);
    let content = fs::read_to_string(path).ok()?;
    let data: SessionData = serde_json::from_str(&content).ok()?;
    data.session_state.agent_name().map(|s| s.to_string())
}

/// Lightweight view of session metadata for listing.
///
/// Skips [`SessionData::session_state`]. Somewhat of a micro-optimization, but saves time scanning
/// large session directories with large [`SessionState`] included.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[typeshare::typeshare]
pub struct SessionDataView {
    pub session_id: String,
    #[typeshare(serialized_as = "String")]
    pub cwd: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub title: Option<String>,
}

trait ListableSession: serde::de::DeserializeOwned {
    fn cwd(&self) -> &Path;
    fn updated_at(&self) -> &DateTime<Utc>;
}

impl ListableSession for SessionData {
    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

impl ListableSession for SessionDataView {
    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Lists available sessions, filtered by cwd if provided.
///
/// When `cwd` is `Some`, only sessions matching that working directory are returned.
/// When `cwd` is `None`, all sessions are returned.
pub fn list_sessions(cwd: Option<&Path>) -> Result<Vec<SessionDataView>, SessionError> {
    list_sessions_impl(&sessions_dir()?, cwd)
}

fn list_sessions_impl<T: ListableSession>(sessions_dir: &Path, cwd: Option<&Path>) -> Result<Vec<T>, SessionError> {
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let canonical_cwd = cwd.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
    let mut sessions = Vec::new();

    let read_dir = fs::read_dir(sessions_dir)
        .map_err(|e| SessionError::io(e, format!("failed to read sessions directory {:?}", sessions_dir)))?;

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let session: T = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to deserialize session");
                continue;
            },
        };
        if let Some(ref filter_cwd) = canonical_cwd {
            let session_cwd = session
                .cwd()
                .canonicalize()
                .unwrap_or_else(|_| session.cwd().to_path_buf());
            if session_cwd != *filter_cwd {
                continue;
            }
        }
        sessions.push(session);
    }

    sessions.sort_by(|a, b| b.updated_at().cmp(a.updated_at()));
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

    /// Write a dummy log entry so the session isn't cleaned up on drop.
    fn write_dummy_log(db: &SessionDb) {
        db.append_log_entry(&LogEntry::prompt("msg".to_string(), vec![], None))
            .unwrap();
    }

    #[test]
    fn test_session_state_serialization_roundtrip() {
        let state = test_state();
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains(r#""version":"v1""#));
        let parsed: SessionState = serde_json::from_str(&json).unwrap();
        match parsed {
            SessionState::V1(_) => {},
            SessionState::Unknown => panic!("should not deserialize as Unknown"),
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

        write_dummy_log(&handle);
        drop(handle);

        // Lock should be released
        assert!(!lock_path(sessions_dir, &session_id).exists());

        // Should be able to load
        let loaded = SessionDb::load_impl(sessions_dir, &session_id, None, pid_always_dead, 1000).unwrap();
        assert_eq!(loaded.session().cwd, cwd);
        assert!(loaded.session().title.is_none());

        // Set title and verify it persists across reload
        loaded.set_title("Test title".to_string()).unwrap();
        assert_eq!(loaded.session().title.as_deref(), Some("Test title"));
        drop(loaded);

        let reloaded = SessionDb::load_impl(sessions_dir, &session_id, None, pid_always_dead, 2000).unwrap();
        assert_eq!(reloaded.session().title.as_deref(), Some("Test title"));
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
        write_dummy_log(&handle);
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

        let entry1 = LogEntry::prompt("msg-1".to_string(), vec![], None);
        let entry2 = LogEntry::prompt("msg-2".to_string(), vec![], None);

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
        let entry = LogEntry::prompt("msg-1".to_string(), vec![], None);
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
        write_dummy_log(&s1);
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
        write_dummy_log(&s2);
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
        write_dummy_log(&s3);
        drop(s3);

        let cwd1_sessions: Vec<SessionDataView> = list_sessions_impl(sessions_dir, Some(&cwd1)).unwrap();
        assert_eq!(cwd1_sessions.len(), 2);

        let cwd2_sessions: Vec<SessionDataView> = list_sessions_impl(sessions_dir, Some(&cwd2)).unwrap();
        assert_eq!(cwd2_sessions.len(), 1);

        // None returns all sessions
        let all_sessions: Vec<SessionDataView> = list_sessions_impl(sessions_dir, None).unwrap();
        assert_eq!(all_sessions.len(), 3);
    }

    #[test]
    fn test_create_session_title_short() {
        use agent::agent_loop::types::ContentBlock;
        let content = vec![ContentBlock::Text("Fix the login bug".to_string())];
        assert_eq!(create_session_title(&content).as_deref(), Some("Fix the login bug"));
    }

    #[test]
    fn test_create_session_title_truncates_at_30() {
        use agent::agent_loop::types::ContentBlock;
        let content = vec![ContentBlock::Text(
            "This is a very long prompt that should be truncated".to_string(),
        )];
        let title = create_session_title(&content).unwrap();
        assert_eq!(title, "This is a very long prompt ...");
        assert_eq!(title.len(), 30);
    }

    #[test]
    fn test_create_session_title_uses_first_line() {
        use agent::agent_loop::types::ContentBlock;
        let content = vec![ContentBlock::Text("First line\nSecond line\nThird".to_string())];
        assert_eq!(create_session_title(&content).as_deref(), Some("First line"));
    }

    #[test]
    fn test_create_session_title_skips_non_text() {
        use agent::agent_loop::types::ContentBlock;
        let content = vec![ContentBlock::Text("Hello".to_string())];
        assert_eq!(create_session_title(&content).as_deref(), Some("Hello"));
        assert_eq!(create_session_title(&[]).as_deref(), None);
    }

    #[test]
    fn test_list_sessions_includes_title() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = temp_dir.path().join("project");
        fs::create_dir_all(&cwd).unwrap();

        let db = SessionDb::new_impl(
            sessions_dir,
            "lt1".to_string(),
            &cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        db.set_title("Session with title".to_string()).unwrap();
        write_dummy_log(&db);
        drop(db);

        let db2 = SessionDb::new_impl(
            sessions_dir,
            "lt2".to_string(),
            &cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        write_dummy_log(&db2);
        drop(db2);

        let sessions: Vec<SessionDataView> = list_sessions_impl(sessions_dir, Some(&cwd)).unwrap();
        assert_eq!(sessions.len(), 2);

        let titled = sessions.iter().find(|s| s.session_id == "lt1").unwrap();
        assert_eq!(titled.title.as_deref(), Some("Session with title"));

        let untitled = sessions.iter().find(|s| s.session_id == "lt2").unwrap();
        assert!(untitled.title.is_none());
    }

    #[test]
    #[ignore]
    fn test_session_data_view_skips_session_state_benchmark() {
        // Verify SessionDataView deserializes correctly and is faster by skipping session_state
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");

        let db = SessionDb::new_impl(
            sessions_dir,
            "perf1".to_string(),
            &cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        write_dummy_log(&db);
        drop(db);

        // Inflate the JSON with a large session_state to simulate real sessions (~40KB avg)
        let json = fs::read_to_string(metadata_path(sessions_dir, "perf1")).unwrap();
        let mut val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let padding = "x".repeat(50_000);
        val["session_state"]["_padding"] = serde_json::Value::String(padding);
        let large_json = serde_json::to_string(&val).unwrap();
        assert!(
            large_json.len() > 50_000,
            "JSON should be >50KB to simulate real sessions"
        );

        let iterations = 500;

        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _: SessionData = serde_json::from_str(&large_json).unwrap();
        }
        let full_duration = start.elapsed();

        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _: SessionDataView = serde_json::from_str(&large_json).unwrap();
        }
        let view_duration = start.elapsed();

        // SessionDataView should be faster since it skips session_state
        assert!(
            view_duration < full_duration,
            "SessionDataView ({view_duration:?}) should be faster than SessionData ({full_duration:?})"
        );

        // Verify correctness
        let full: SessionData = serde_json::from_str(&large_json).unwrap();
        let view: SessionDataView = serde_json::from_str(&large_json).unwrap();
        assert_eq!(full.session_id, view.session_id);
        assert_eq!(full.cwd, view.cwd);
        assert_eq!(full.created_at, view.created_at);
        assert_eq!(full.updated_at, view.updated_at);
        assert_eq!(full.title, view.title);
    }

    /// Regression test: sessions saved before a new field was added must still load.
    ///
    /// The fixture files under `fixtures/` were generated from real types *before*
    /// the `metering_usage` field existed. They are frozen and must never be edited.
    /// If you add a new field to any type in the session serialization chain, this
    /// test will fail unless you also add `#[serde(default)]` to that field.
    #[test]
    fn test_backward_compat_pre_metering_session_loads() {
        use agent::event_log::LogEntry;

        // Frozen fixture: SessionData JSON written before metering_usage existed
        let session_json = include_str!("fixtures/session_v1_pre_metering.json");
        let session: SessionData = serde_json::from_str(session_json)
            .expect("old session format must deserialize; did you add a required field without #[serde(default)]?");

        assert_eq!(session.session_id, "compat-metering");
        let turns = &session
            .session_state
            .conversation_metadata()
            .unwrap()
            .user_turn_metadatas;
        assert_eq!(turns.len(), 1);
        assert!(
            turns[0].metering_usage.is_empty(),
            "metering_usage should default to empty vec"
        );
        assert_eq!(turns[0].input_token_count, 100);
        assert_eq!(turns[0].output_token_count, 50);

        // Frozen fixture: JSONL log entries written before any format changes
        let log_jsonl = include_str!("fixtures/session_v1_pre_metering.jsonl");
        let entries: Vec<LogEntry> = log_jsonl
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                serde_json::from_str(l)
                    .expect("old log entry must deserialize; did you add a required field without #[serde(default)]?")
            })
            .collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_peek_agent_name() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");

        // No session file → None
        assert!(peek_agent_name(sessions_dir, "nonexistent").is_none());

        // Session without agent_name → None
        let db = SessionDb::new_impl(
            sessions_dir,
            "pa1".to_string(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        write_dummy_log(&db);
        drop(db);
        assert!(peek_agent_name(sessions_dir, "pa1").is_none());

        // Session with agent_name → Some
        let mut state = test_state();
        state.set_agent_name("my-agent".to_string());
        let db2 = SessionDb::new_impl(sessions_dir, "pa2".to_string(), cwd, state, pid_always_dead, 1000).unwrap();
        write_dummy_log(&db2);
        drop(db2);
        assert_eq!(peek_agent_name(sessions_dir, "pa2").as_deref(), Some("my-agent"));
    }

    #[test]
    fn test_title_from_first_log_entry() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");

        let db = SessionDb::new_impl(
            sessions_dir,
            "t1".to_string(),
            &cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        db.append_log_entry(&LogEntry::prompt(
            "m1".to_string(),
            vec![agent::agent_loop::types::ContentBlock::Text(
                "Explain how websockets work in Rust".to_string(),
            )],
            None,
        ))
        .unwrap();
        drop(db);

        let title = title_from_first_log_entry(sessions_dir, "t1");
        assert_eq!(title.as_deref(), Some("Explain how websockets work..."));

        // No log file → None
        assert!(title_from_first_log_entry(sessions_dir, "nonexistent").is_none());

        // Empty log → None
        let db2 = SessionDb::new_impl(
            sessions_dir,
            "t2".to_string(),
            &cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        drop(db2);
        assert!(title_from_first_log_entry(sessions_dir, "t2").is_none());
    }

    #[test]
    fn test_session_exists_requires_both_files() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        assert!(!session_exists(dir, "s1"), "should be false when neither file exists");

        fs::write(metadata_path(dir, "s1"), "{}").unwrap();
        assert!(!session_exists(dir, "s1"), "should be false when only metadata exists");

        fs::remove_file(metadata_path(dir, "s1")).unwrap();
        fs::write(log_path(dir, "s1"), "").unwrap();
        assert!(!session_exists(dir, "s1"), "should be false when only log exists");

        fs::write(metadata_path(dir, "s1"), "{}").unwrap();
        assert!(session_exists(dir, "s1"), "should be true when both files exist");
    }

    #[test]
    fn test_load_unknown_session_state_falls_back_to_v1() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "unknown-state";

        // Create a valid session, then overwrite session_state with an unknown version tag.
        // This simulates loading a session written by a newer client version.
        let db = SessionDb::new_impl(
            sessions_dir,
            session_id.to_string(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        write_dummy_log(&db);
        drop(db);

        let meta_path = metadata_path(sessions_dir, session_id);
        let mut val: serde_json::Value = serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        val["session_state"] = serde_json::json!({"version": "v2", "some_future_field": 42});
        fs::write(&meta_path, serde_json::to_string_pretty(&val).unwrap()).unwrap();

        let loaded = SessionDb::load_impl(sessions_dir, session_id, None, pid_always_dead, 1000).unwrap();
        let session = loaded.session();
        match &session.session_state {
            SessionState::V1(v1) => {
                assert_eq!(v1.rts_model_state.conversation_id, session_id);
            },
            SessionState::Unknown => panic!("Unknown state should have been replaced with V1"),
        }
    }

    #[test]
    fn test_load_corrupt_v1_session_state_falls_back_to_v1() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path();
        let cwd = Path::new("/test/project");
        let session_id = "corrupt-v1";

        // Create a valid session, then corrupt the v1 state fields.
        // This simulates a v1 state with incompatible field changes.
        let db = SessionDb::new_impl(
            sessions_dir,
            session_id.to_string(),
            cwd,
            test_state(),
            pid_always_dead,
            1000,
        )
        .unwrap();
        write_dummy_log(&db);
        drop(db);

        let meta_path = metadata_path(sessions_dir, session_id);
        let mut val: serde_json::Value = serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        val["session_state"] = serde_json::json!({"version": "v1", "unexpected_field": true});
        fs::write(&meta_path, serde_json::to_string_pretty(&val).unwrap()).unwrap();

        let loaded = SessionDb::load_impl(sessions_dir, session_id, None, pid_always_dead, 1000).unwrap();
        let session = loaded.session();
        match &session.session_state {
            SessionState::V1(v1) => {
                assert_eq!(v1.rts_model_state.conversation_id, session_id);
            },
            SessionState::Unknown => panic!("Unknown state should have been replaced with V1"),
        }
    }
}
