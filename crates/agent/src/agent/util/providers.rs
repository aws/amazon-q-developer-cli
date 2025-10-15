use std::env::VarError;
use std::path::PathBuf;

use super::directories;

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
pub struct SystemProvider;

impl EnvProvider for SystemProvider {
    fn var(&self, input: &str) -> Result<String, VarError> {
        std::env::var(input)
    }
}

impl HomeProvider for SystemProvider {
    fn home(&self) -> Option<PathBuf> {
        directories::home_dir().ok()
    }
}

impl CwdProvider for SystemProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        std::env::current_dir()
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct TestSystem {
    env: std::collections::HashMap<String, String>,
    home: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

#[cfg(test)]
impl TestSystem {
    pub fn new() -> Self {
        let mut env = std::collections::HashMap::new();
        env.insert("HOME".to_string(), "/home/testuser".to_string());
        Self {
            env,
            home: Some(PathBuf::from("/home/testuser")),
            cwd: Some(PathBuf::from("/home/testuser")),
        }
    }

    pub fn with_var(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.env.insert(key.as_ref().to_string(), value.as_ref().to_string());
        self
    }

    pub fn with_cwd(mut self, cwd: impl AsRef<std::path::Path>) -> Self {
        self.cwd = Some(PathBuf::from(cwd.as_ref()));
        self
    }
}

#[cfg(test)]
impl EnvProvider for TestSystem {
    fn var(&self, input: &str) -> Result<String, VarError> {
        self.env.get(input).cloned().ok_or(VarError::NotPresent)
    }
}

#[cfg(test)]
impl HomeProvider for TestSystem {
    fn home(&self) -> Option<PathBuf> {
        self.home.as_ref().cloned()
    }
}

#[cfg(test)]
impl CwdProvider for TestSystem {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        self.cwd.as_ref().cloned().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            eyre::eyre!("not found"),
        ))
    }
}
