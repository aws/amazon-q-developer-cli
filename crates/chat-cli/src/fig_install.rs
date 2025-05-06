use std::time::SystemTimeError;

use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Util(#[from] crate::fig_util::UtilError),
    #[error(transparent)]
    Settings(#[from] crate::fig_settings::SettingsError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Semver(#[from] semver::Error),
    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
    #[error(transparent)]
    Strum(#[from] strum::ParseError),
    #[cfg(target_os = "macos")]
    #[error("failed to update due to auth error: `{0}`")]
    SecurityFramework(#[from] security_framework::base::Error),
}

impl From<crate::fig_util::directories::DirectoryError> for Error {
    fn from(err: crate::fig_util::directories::DirectoryError) -> Self {
        crate::fig_util::UtilError::Directory(err).into()
    }
}
