use std::env::VarError;
use std::path::PathBuf;
use std::sync::Arc;

use super::directories;

/// A trait for accessing system and process context (env vars, home dir, current working dir,
/// etc.).
pub trait SystemProvider: EnvProvider + HomeProvider + CwdProvider + std::fmt::Debug + Send + Sync + 'static {}

impl EnvProvider for Box<dyn SystemProvider> {
    fn var(&self, input: &str) -> Result<String, VarError> {
        (**self).var(input)
    }
}

impl HomeProvider for Box<dyn SystemProvider> {
    fn home(&self) -> Option<PathBuf> {
        (**self).home()
    }
}

impl CwdProvider for Box<dyn SystemProvider> {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        (**self).cwd()
    }
}

impl SystemProvider for Box<dyn SystemProvider> {}

impl EnvProvider for Arc<dyn SystemProvider> {
    fn var(&self, input: &str) -> Result<String, VarError> {
        (**self).var(input)
    }
}

impl HomeProvider for Arc<dyn SystemProvider> {
    fn home(&self) -> Option<PathBuf> {
        (**self).home()
    }
}

impl CwdProvider for Arc<dyn SystemProvider> {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        (**self).cwd()
    }
}

impl SystemProvider for Arc<dyn SystemProvider> {}

/// A trait for accessing environment variables.
///
/// This provides unit tests the capability to fake system context.
pub trait EnvProvider {
    fn var(&self, input: &str) -> Result<String, VarError>;
}

impl EnvProvider for Box<dyn EnvProvider> {
    fn var(&self, input: &str) -> Result<String, VarError> {
        (**self).var(input)
    }
}

/// A trait for getting the home directory.
///
/// This provides unit tests the capability to fake system context.
pub trait HomeProvider {
    fn home(&self) -> Option<PathBuf>;
}

impl HomeProvider for Box<dyn HomeProvider> {
    fn home(&self) -> Option<PathBuf> {
        (**self).home()
    }
}

/// A trait for getting the current working directory.
///
/// This provides unit tests the capability to fake system context.
pub trait CwdProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error>;
}

impl CwdProvider for Box<dyn CwdProvider> {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        (**self).cwd()
    }
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

impl SystemProvider for RealProvider {}
