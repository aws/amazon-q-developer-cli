use std::env::VarError;
use std::path::PathBuf;

use super::directories;

/// A trait for accessing system and process context (env vars, home dir, current working dir,
/// etc.).
pub trait SystemProvider: EnvProvider + HomeProvider + CwdProvider {}

impl<T> SystemProvider for T where T: EnvProvider + HomeProvider + CwdProvider {}

/// A trait for accessing environment variables.
///
/// This provides unit tests the capability to fake system context.
pub trait EnvProvider {
    fn var(&self, input: &str) -> Result<String, VarError>;

    /// Helper for [shellexpand::full_with_context]
    fn shellexpand_context(&self) -> impl Fn(&str) -> Result<Option<String>, VarError> {
        |input: &str| Ok(EnvProvider::var(self, input).ok())
    }
}

/// A trait for getting the home directory.
///
/// This provides unit tests the capability to fake system context.
pub trait HomeProvider {
    fn home(&self) -> Option<PathBuf>;

    /// Helper for [shellexpand::full_with_context]
    fn shellexpand_home(&self) -> impl Fn() -> Option<String> {
        || HomeProvider::home(self).map(|h| h.to_string_lossy().to_string())
    }
}

/// A trait for getting the current working directory.
///
/// This provides unit tests the capability to fake system context.
pub trait CwdProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error>;
}

/// Provides real implementations for [EnvProvider], [HomeProvider], and [CwdProvider].
#[derive(Clone, Copy)]
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
