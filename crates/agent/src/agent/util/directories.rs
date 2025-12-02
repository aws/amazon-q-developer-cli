use std::path::PathBuf;

use super::error::UtilError;

type Result<T, E = UtilError> = std::result::Result<T, E>;

pub fn home_dir() -> Result<PathBuf, UtilError> {
    dirs::home_dir().ok_or(UtilError::MissingHomeDir)
}
