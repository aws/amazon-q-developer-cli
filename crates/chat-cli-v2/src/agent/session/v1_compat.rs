//! Compatibility layer for importing V1 conversations into V2 sessions.
//!
//! Defines a trait that `chat-cli` implements (since it owns the V1 types)
//! and `chat-cli-v2` consumes via trait object.
//!
//! This will be removed when v1 code is deprecated in favor of only supporting v2.

/// V1 user message start delimiter. V1 wraps the actual user prompt in these
/// markers when context entries (hooks, timestamps) are prepended to the message.
/// Mirrors `USER_ENTRY_START_HEADER` in `chat_cli::cli::chat::message`.
pub const USER_MESSAGE_START_HEADER: &str = "--- USER MESSAGE BEGIN ---\n";

/// V1 user message end delimiter. See [`USER_MESSAGE_START_HEADER`].
pub const USER_MESSAGE_END_HEADER: &str = "--- USER MESSAGE END ---";

use std::path::{
    Path,
    PathBuf,
};

use chrono::{
    DateTime,
    Utc,
};

/// Error from a V1 export operation.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct V1ExportError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

/// Lightweight V1 session info for listing in the V2 session picker.
#[derive(Debug, Clone)]
pub struct V1SessionInfo {
    /// The V1 conversation UUID, reused as the V2 session ID.
    pub conversation_id: String,
    /// The filesystem path (cwd) the conversation was created in.
    pub cwd: PathBuf,
    /// Human-readable title derived from the first user prompt.
    pub title: Option<String>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Number of messages in the conversation history.
    pub message_count: usize,
}

/// Trait for accessing V1 conversation data from V2 code.
///
/// Implemented in `chat-cli` which has access to V1 types.
pub trait V1SessionExporter: Send + Sync + std::fmt::Debug {
    /// List V1 sessions for the given working directory.
    fn list_sessions(&self, cwd: &Path) -> Result<Vec<V1SessionInfo>, V1ExportError>;

    /// Export a V1 conversation to V2 session format on disk.
    ///
    /// Writes `{session_id}.json` and `{session_id}.jsonl` to `sessions_dir`.
    /// No-op if session files already exist (idempotent).
    fn export_session(&self, conversation_id: &str, sessions_dir: &Path) -> Result<(), V1ExportError>;

    /// Try to parse raw JSON as a V1 `ConversationState` and export it.
    ///
    /// Returns `Ok(())` if the content was a valid V1 conversation and was
    /// successfully exported. Returns `Err` if the content is not a V1
    /// conversation or on I/O failures.
    fn try_export_from_json(
        &self,
        json_content: &str,
        session_id: &str,
        cwd: &Path,
        sessions_dir: &Path,
        imported_from: Option<&Path>,
    ) -> Result<(), V1ExportError>;
}

/// No-op exporter for contexts where V1 database is unavailable, only created for the
/// compiler. No-op implementation used by the `chat_cli_v2` binary, which has no access to the
/// V1 database. The real implementation lives in `chat_cli::V1SessionExporterImpl`.
///
/// TODO: Remove once the standalone `chat_cli_v2` binary is cleaned up and removed.
#[derive(Debug)]
pub struct NoOpV1SessionExporter;

impl V1SessionExporter for NoOpV1SessionExporter {
    fn list_sessions(&self, _cwd: &Path) -> Result<Vec<V1SessionInfo>, V1ExportError> {
        Ok(Vec::new())
    }

    fn export_session(&self, _conversation_id: &str, _sessions_dir: &Path) -> Result<(), V1ExportError> {
        Ok(())
    }

    fn try_export_from_json(
        &self,
        _json_content: &str,
        _session_id: &str,
        _cwd: &Path,
        _sessions_dir: &Path,
        _imported_from: Option<&Path>,
    ) -> Result<(), V1ExportError> {
        Err(V1ExportError {
            message: "V1 export not available".into(),
            source: None,
        })
    }
}
