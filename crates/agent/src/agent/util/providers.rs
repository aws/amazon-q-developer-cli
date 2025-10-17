use std::env::VarError;
use std::path::PathBuf;

use super::directories;

/// A trait for accessing system and process context (env vars, home dir, current working dir,
/// etc.).
pub trait SystemProvider: EnvProvider + HomeProvider + CwdProvider + std::fmt::Debug + Send + Sync + 'static {}

impl<T> SystemProvider for T where T: EnvProvider + HomeProvider + CwdProvider + std::fmt::Debug + Send + Sync + 'static {}

/// A trait for accessing environment variables.
///
/// This provides unit tests the capability to fake system context.
pub trait EnvProvider {
    fn var(&self, input: &str) -> Result<String, VarError>;
}

/// A trait for getting the home directory.
///
/// This provides unit tests the capability to fake system context.
pub trait HomeProvider {
    fn home(&self) -> Option<PathBuf>;
}

/// A trait for getting the current working directory.
///
/// This provides unit tests the capability to fake system context.
pub trait CwdProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error>;
}

/// Provides real implementations for [EnvProvider], [HomeProvider], and [CwdProvider].
#[derive(Debug, Clone, Copy)]
pub struct RealProvider;

impl EnvProvider for RealProvider {
    fn var(&self, input: &str) -> Result<String, VarError> {
        std::env::var(input)
    }
}

impl HomeProvider for RealProvider {
    fn home(&self) -> Option<PathBuf> {
        directories::home_dir().ok()
    }
}

impl CwdProvider for RealProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        std::env::current_dir()
    }
}
