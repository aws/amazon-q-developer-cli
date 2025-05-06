use std::sync::PoisonError;

use thiserror::Error;

use crate::util::directories::DirectoryError;

// A cloneable error
#[derive(Debug, Clone, thiserror::Error)]
#[error("Failed to open database: {}", .0)]
pub struct DbOpenError(pub(crate) String);

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    FigUtilError(#[from] crate::util::UtilError),
    #[error("settings file is not a json object")]
    SettingsNotObject,
    #[error(transparent)]
    DirectoryError(#[from] DirectoryError),
    #[error("memory backend is not used")]
    MemoryBackendNotUsed,
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    R2d2(#[from] r2d2::Error),
    #[error(transparent)]
    DbOpenError(#[from] DbOpenError),
    #[error("{}", .0)]
    PoisonError(String),
}

impl<T> From<PoisonError<T>> for SettingsError {
    fn from(value: PoisonError<T>) -> Self {
        Self::PoisonError(value.to_string())
    }
}

pub type Result<T, E = SettingsError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    fn all_errors() -> Vec<SettingsError> {
        vec![
            std::io::Error::new(std::io::ErrorKind::InvalidData, "oops").into(),
            serde_json::from_str::<()>("oops").unwrap_err().into(),
            crate::util::UtilError::UnsupportedPlatform.into(),
            SettingsError::SettingsNotObject,
            crate::util::directories::DirectoryError::NoHomeDirectory.into(),
            SettingsError::MemoryBackendNotUsed,
            rusqlite::Error::SqliteSingleThreadedMode.into(),
            // r2d2::Error
            DbOpenError("oops".into()).into(),
            PoisonError::<()>::new(()).into(),
        ]
    }

    #[test]
    fn test_error_display_debug() {
        for error in all_errors() {
            eprintln!("{}", error);
            eprintln!("{:?}", error);
        }
    }
}
