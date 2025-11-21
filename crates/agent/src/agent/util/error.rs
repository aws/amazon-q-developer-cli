use std::env::VarError;
use std::sync::PoisonError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UtilError {
    #[error("Missing a home directory")]
    MissingHomeDir,
    #[error("Missing a local data directory")]
    MissingDataLocalDir,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{context}: {source}")]
    JsonWithContext {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{}", .0)]
    Custom(String),

    #[error(transparent)]
    PathExpand(#[from] shellexpand::LookupError<VarError>),

    #[error(transparent)]
    GlobsetError(#[from] globset::Error),
    #[error(transparent)]
    GlobPatternParse(#[from] glob::PatternError),
    #[error(transparent)]
    GlobIterate(#[from] glob::GlobError),

    // database errors
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    R2d2(#[from] r2d2::Error),
    #[error("Failed to open database: {}", .0)]
    DbOpenError(String),

    #[error("{}", .0)]
    PoisonError(String),

    #[error(transparent)]
    StringFromUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    StrFromUtf8(#[from] std::str::Utf8Error),
}

impl UtilError {
    fn io_context(e: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source: e,
        }
    }

    fn json_context(e: serde_json::Error, context: impl Into<String>) -> Self {
        Self::JsonWithContext {
            context: context.into(),
            source: e,
        }
    }
}

impl<T> From<PoisonError<T>> for UtilError {
    fn from(value: PoisonError<T>) -> Self {
        Self::PoisonError(value.to_string())
    }
}

/// Helper trait for creating [UtilError] with included context around common error types.
pub trait ErrorContext<T> {
    fn context(self, context: impl Into<String>) -> Result<T, UtilError>;

    fn with_context<C, F>(self, f: F) -> Result<T, UtilError>
    where
        C: Into<String>,
        F: FnOnce() -> C;
}

impl<T> ErrorContext<T> for Result<T, std::io::Error> {
    fn context(self, context: impl Into<String>) -> Result<T, UtilError> {
        self.map_err(|e| UtilError::io_context(e, context))
    }

    fn with_context<C, F>(self, f: F) -> Result<T, UtilError>
    where
        C: Into<String>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| UtilError::io_context(e, f()))
    }
}

impl<T> ErrorContext<T> for Result<T, serde_json::Error> {
    fn context(self, context: impl Into<String>) -> Result<T, UtilError> {
        self.map_err(|e| UtilError::json_context(e, context))
    }

    fn with_context<C, F>(self, f: F) -> Result<T, UtilError>
    where
        C: Into<String>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| UtilError::json_context(e, f()))
    }
}
