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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn test_missing_home_dir_display() {
        let e = UtilError::MissingHomeDir;
        assert_eq!(e.to_string(), "Missing a home directory");
    }

    #[test]
    fn test_missing_data_local_dir_display() {
        let e = UtilError::MissingDataLocalDir;
        assert_eq!(e.to_string(), "Missing a local data directory");
    }

    #[test]
    fn test_custom_display() {
        let e = UtilError::Custom("oops".to_string());
        assert_eq!(e.to_string(), "oops");
    }

    #[test]
    fn test_db_open_error_display() {
        let e = UtilError::DbOpenError("db.sqlite".to_string());
        assert!(e.to_string().contains("Failed to open database"));
        assert!(e.to_string().contains("db.sqlite"));
    }

    #[test]
    fn test_poison_error_display() {
        let e = UtilError::PoisonError("p".to_string());
        assert_eq!(e.to_string(), "p");
    }

    #[test]
    fn test_io_context() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let e = UtilError::io_context(io_err, "reading config");
        let s = e.to_string();
        assert!(s.contains("reading config"));
        assert!(s.contains("file gone"));
    }

    #[test]
    fn test_json_context() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let e = UtilError::json_context(json_err, "parsing settings");
        let s = e.to_string();
        assert!(s.contains("parsing settings"));
    }

    #[test]
    fn test_io_error_context_trait_ok() {
        let r: Result<i32, std::io::Error> = Ok(42);
        assert_eq!(r.context("ctx").unwrap(), 42);
    }

    #[test]
    fn test_io_error_context_trait_err() {
        let r: Result<i32, std::io::Error> = Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "x"));
        let err = r.context("op failed").unwrap_err();
        assert!(err.to_string().contains("op failed"));
    }

    #[test]
    fn test_io_error_with_context_trait() {
        let r: Result<i32, std::io::Error> = Err(std::io::Error::new(std::io::ErrorKind::Other, "bad"));
        let err = r.with_context(|| "lazy ctx").unwrap_err();
        assert!(err.to_string().contains("lazy ctx"));
    }

    #[test]
    fn test_io_error_with_context_trait_ok() {
        let r: Result<i32, std::io::Error> = Ok(5);
        assert_eq!(r.with_context(|| "x").unwrap(), 5);
    }

    #[test]
    fn test_json_error_context_trait_ok() {
        let r: Result<serde_json::Value, serde_json::Error> = Ok(serde_json::Value::Null);
        assert!(r.context("c").is_ok());
    }

    #[test]
    fn test_json_error_context_trait_err() {
        let r: Result<serde_json::Value, serde_json::Error> = serde_json::from_str("invalid");
        let err = r.context("decode").unwrap_err();
        assert!(err.to_string().contains("decode"));
    }

    #[test]
    fn test_json_error_with_context_trait() {
        let r: Result<serde_json::Value, serde_json::Error> = serde_json::from_str("invalid");
        let err = r.with_context(|| "lazy decode").unwrap_err();
        assert!(err.to_string().contains("lazy decode"));
    }

    #[test]
    fn test_from_json_error() {
        let err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let e: UtilError = err.into();
        assert!(matches!(e, UtilError::Json(_)));
    }

    #[test]
    fn test_from_poison_error() {
        let m = Mutex::new(0);
        let _ = std::panic::catch_unwind(|| {
            let _g = m.lock().unwrap();
            panic!();
        });
        let result = m.lock();
        if let Err(p) = result {
            let e: UtilError = p.into();
            assert!(matches!(e, UtilError::PoisonError(_)));
        }
    }

    #[test]
    fn test_from_glob_pattern_error() {
        let err = glob::Pattern::new("[").unwrap_err();
        let e: UtilError = err.into();
        assert!(matches!(e, UtilError::GlobPatternParse(_)));
    }

    #[test]
    fn test_from_string_from_utf8_error() {
        let err = String::from_utf8(vec![0xff, 0xfe]).unwrap_err();
        let e: UtilError = err.into();
        assert!(matches!(e, UtilError::StringFromUtf8(_)));
    }

    #[test]
    #[allow(invalid_from_utf8)]
    fn test_from_str_utf8_error() {
        let bytes = [0xff, 0xfe];
        let err = std::str::from_utf8(&bytes).unwrap_err();
        let e: UtilError = err.into();
        assert!(matches!(e, UtilError::StrFromUtf8(_)));
    }
}
