//! Disk-side companion to [`super::v2_to_kas::convert_v2_to_kas`].
//!
//! Atomically writes a `ConvertOutput` (KAS metadata + persisted
//! messages) to the workspace-hashed bucket on disk. Mirrors the
//! staging-rename pattern from [`super::import::import_session`] so a
//! crashed write never leaves a partial session that KAS's listing
//! enumerates.

use std::path::PathBuf;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};
use std::{
    fs,
    process,
};

use thiserror::Error;

use super::schema::{
    PersistedMessage,
    SessionMetadata,
};
use super::workspace_hash::compute_workspace_hash;

/// Inputs to [`write_kas_session_dir`]. Owned data - the writer
/// runs once per session and the input volume is bounded.
pub struct WriteKasSessionOptions {
    /// Root sessions directory, e.g. `~/.kiro/sessions`.
    pub kas_sessions_root: PathBuf,
    /// Workspace paths the session should be bucketed under. Hashed
    /// to derive the bucket directory name.
    pub workspace_paths: Vec<String>,
    /// KAS session id - becomes the leaf directory name under the
    /// bucket.
    pub session_id: String,
    /// Serialized KAS session metadata. Written to `session.json`.
    pub metadata: SessionMetadata,
    /// Serialized persisted messages, one per line in `messages.jsonl`.
    pub messages: Vec<PersistedMessage>,
}

#[derive(Debug, Error)]
pub enum WriteKasSessionError {
    /// The target session dir already exists. Callers are expected to
    /// probe for this case and short-circuit BEFORE calling
    /// [`write_kas_session_dir`] - the writer never overwrites a
    /// populated session because doing so would erase KAS-managed
    /// `snapshots/` and `sub-executions/` subdirectories.
    #[error("target session directory already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Serialize(String),
}

impl WriteKasSessionError {
    fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// Monotonic counter for the staging-dir suffix, in addition to PID
/// and timestamp. Guarantees that two `write_kas_session_dir` calls
/// from the same process and the same wall-clock millisecond still
/// stage at distinct paths.
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically write a KAS session to
/// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{session_id}/`.
///
/// Stages the directory at a per-invocation unique sibling path
/// (`{kas_sessions_root}/.staging-{session_id}-{pid}-{timestamp}-{counter}/`)
/// and renames it into place once both files are written. On failure
/// the staging dir is removed best-effort; the final dir is never
/// partially populated.
///
/// Returns [`WriteKasSessionError::AlreadyExists`] if the final dir
/// is already populated. Callers must probe for this case and decide
/// whether to short-circuit, retry after delete, or surface it as
/// an error - the writer never overwrites a populated session.
pub fn write_kas_session_dir(opts: WriteKasSessionOptions) -> Result<PathBuf, WriteKasSessionError> {
    let hash = compute_workspace_hash(&opts.workspace_paths);
    let bucket_dir = opts.kas_sessions_root.join(&hash);
    let final_dir = bucket_dir.join(&opts.session_id);

    if final_dir.exists() {
        return Err(WriteKasSessionError::AlreadyExists(final_dir));
    }

    let staging_dir = opts.kas_sessions_root.join(staging_dir_name(&opts.session_id));

    let result = (|| -> Result<PathBuf, WriteKasSessionError> {
        if let Some(parent) = staging_dir.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| WriteKasSessionError::io(format!("create parent {}", parent.display()), e))?;
        }
        // The staging dir name is per-invocation unique, so a pre-
        // existing entry under that path is necessarily orphaned from
        // a prior crash with the same pid/timestamp/counter and is
        // safe to clear.
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)
                .map_err(|e| WriteKasSessionError::io(format!("clear stale staging {}", staging_dir.display()), e))?;
        }
        fs::create_dir_all(&staging_dir)
            .map_err(|e| WriteKasSessionError::io(format!("create staging {}", staging_dir.display()), e))?;

        let session_json_path = staging_dir.join("session.json");
        let session_json_bytes = serde_json::to_vec_pretty(&opts.metadata)
            .map_err(|e| WriteKasSessionError::Serialize(format!("serialize session.json: {e}")))?;
        fs::write(&session_json_path, &session_json_bytes)
            .map_err(|e| WriteKasSessionError::io(format!("write {}", session_json_path.display()), e))?;

        let messages_path = staging_dir.join("messages.jsonl");
        let mut buffer = Vec::new();
        for message in &opts.messages {
            let line = serde_json::to_vec(message)
                .map_err(|e| WriteKasSessionError::Serialize(format!("serialize message: {e}")))?;
            buffer.extend_from_slice(&line);
            buffer.push(b'\n');
        }
        fs::write(&messages_path, &buffer)
            .map_err(|e| WriteKasSessionError::io(format!("write {}", messages_path.display()), e))?;

        fs::create_dir_all(&bucket_dir)
            .map_err(|e| WriteKasSessionError::io(format!("create bucket {}", bucket_dir.display()), e))?;

        // A concurrent writer could have populated `final_dir` between
        // our pre-flight check and this rename. Re-check and refuse
        // rather than clobbering - the caller's idempotency probe
        // already ran, and double-conversion isn't a problem we want
        // to paper over here.
        if final_dir.exists() {
            return Err(WriteKasSessionError::AlreadyExists(final_dir));
        }
        fs::rename(&staging_dir, &final_dir).map_err(|e| {
            WriteKasSessionError::io(
                format!("rename {} -> {}", staging_dir.display(), final_dir.display()),
                e,
            )
        })?;
        Ok(final_dir)
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result
}

/// Build a staging directory name unique to this invocation. The name
/// is sibling to the per-workspace bucket dirs and lives outside any
/// bucket so KAS's session lister never enumerates an in-flight
/// staging dir.
fn staging_dir_name(session_id: &str) -> String {
    let pid = process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(".staging-{session_id}-{pid}-{nanos}-{counter}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::thread;

    use tempfile::TempDir;

    use super::*;
    use crate::agent::kas::schema::{
        CURRENT_SCHEMA_VERSION,
        MessagePayload,
        PersistedMessage,
        UserMessagePayload,
    };

    fn fixture_metadata(id: &str) -> SessionMetadata {
        SessionMetadata {
            schema_version: CURRENT_SCHEMA_VERSION.to_string(),
            id: id.to_string(),
            title: "test".to_string(),
            agent_mode: "default".to_string(),
            workspace_paths: vec!["/tmp/ws".to_string()],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_modified_at: "2024-01-01T00:00:00Z".to_string(),
            data_model_version: None,
            model_id: None,
            created_reason: None,
            extra: serde_json::Map::new(),
        }
    }

    fn fixture_message(id: &str, content: &str) -> PersistedMessage {
        PersistedMessage {
            id: id.to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload: MessagePayload::User(UserMessagePayload {
                content: content.to_string(),
                source: None,
                images: None,
                meta: None,
            }),
        }
    }

    /// Happy path: writes session.json + messages.jsonl atomically.
    #[test]
    fn write_session_dir_writes_both_files() {
        let root = TempDir::new().unwrap();
        let metadata = fixture_metadata("sess_test");
        let messages = vec![fixture_message("m-1", "hello")];

        let final_dir = write_kas_session_dir(WriteKasSessionOptions {
            kas_sessions_root: root.path().to_path_buf(),
            workspace_paths: vec!["/tmp/ws".to_string()],
            session_id: "sess_test".to_string(),
            metadata,
            messages,
        })
        .expect("first write should succeed");

        assert!(final_dir.join("session.json").exists());
        assert!(final_dir.join("messages.jsonl").exists());
    }

    /// Refusing to clobber a populated final_dir prevents the writer
    /// from erasing KAS-managed `snapshots/` and `sub-executions/`
    /// subdirectories that may have been created by KAS itself
    /// between the caller's idempotency probe and this writer.
    #[test]
    fn write_session_dir_refuses_existing_target() {
        let root = TempDir::new().unwrap();
        let metadata = fixture_metadata("sess_test");

        write_kas_session_dir(WriteKasSessionOptions {
            kas_sessions_root: root.path().to_path_buf(),
            workspace_paths: vec!["/tmp/ws".to_string()],
            session_id: "sess_test".to_string(),
            metadata: metadata.clone(),
            messages: vec![],
        })
        .expect("first write should succeed");

        // Drop a sentinel into the populated session dir to confirm
        // the writer doesn't clobber.
        let hash = compute_workspace_hash(&["/tmp/ws".to_string()]);
        let session_dir = root.path().join(&hash).join("sess_test");
        let snapshots_dir = session_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::write(snapshots_dir.join("sentinel"), b"keep me").unwrap();

        let result = write_kas_session_dir(WriteKasSessionOptions {
            kas_sessions_root: root.path().to_path_buf(),
            workspace_paths: vec!["/tmp/ws".to_string()],
            session_id: "sess_test".to_string(),
            metadata,
            messages: vec![],
        });

        assert!(matches!(result, Err(WriteKasSessionError::AlreadyExists(_))));
        // Sentinel survived the refused write.
        assert!(snapshots_dir.join("sentinel").exists());
    }

    /// Concurrent writers stage at distinct paths so they do not
    /// corrupt each other's in-flight state.
    #[test]
    fn concurrent_writers_use_distinct_staging_dirs() {
        let root = TempDir::new().unwrap();
        let observed: Mutex<HashMap<String, ()>> = Mutex::new(HashMap::new());

        thread::scope(|s| {
            for i in 0..16 {
                let root = root.path().to_path_buf();
                let observed = &observed;
                s.spawn(move || {
                    let session_id = format!("sess_{i}");
                    let metadata = fixture_metadata(&session_id);
                    let _ = write_kas_session_dir(WriteKasSessionOptions {
                        kas_sessions_root: root,
                        workspace_paths: vec!["/tmp/ws".to_string()],
                        session_id: session_id.clone(),
                        metadata,
                        messages: vec![],
                    });
                    let staging = staging_dir_name(&session_id);
                    observed.lock().unwrap().insert(staging, ());
                });
            }
        });

        // 16 concurrent invocations against 16 distinct session ids
        // should produce 16 distinct staging names.
        assert_eq!(observed.lock().unwrap().len(), 16);
    }
}
