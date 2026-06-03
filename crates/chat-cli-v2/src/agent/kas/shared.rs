//! Types and helpers shared between [`super::import`] and [`super::export`].

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by import / export.
#[derive(Debug, Error)]
pub enum SessionArchiveError {
    #[error("{0}")]
    Message(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl SessionArchiveError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }

    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// Resolve the KAS sessions root from `~/.kiro/sessions/`, honoring
/// the `KIRO_HOME` environment variable.
pub fn default_kas_sessions_root() -> Result<PathBuf, SessionArchiveError> {
    let kiro_home = crate::util::paths::kiro_home_dir_from_process_env()
        .map_err(|e| SessionArchiveError::msg(format!("failed to resolve kiro home directory: {e}")))?;
    Ok(kiro_home.join("sessions"))
}
