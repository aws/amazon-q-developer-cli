//! Module for common testing utilities

use std::env::VarError;
use std::path::{
    Path,
    PathBuf,
};

use super::providers::{
    CwdProvider,
    EnvProvider,
    HomeProvider,
};

#[derive(Debug)]
pub struct TestDir {
    temp_dir: tempfile::TempDir,
}

impl TestDir {
    pub fn new() -> Self {
        Self {
            temp_dir: tempfile::tempdir().unwrap(),
        }
    }

    /// Returns a resolved path using the generated temporary directory as the base.
    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.temp_dir.path().join(path)
    }

    /// Writes the given file under the test directory. Creates parent directories if needed.
    pub async fn with_file(self, file: impl TestFile) -> Self {
        let file_path = file.path();
        if file_path.is_absolute() {
            panic!("absolute paths are currently not supported");
        }

        let path = self.temp_dir.path().join(file_path);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.unwrap();
            }
        }
        tokio::fs::write(path, file.content()).await.unwrap();
        self
    }
}

impl Default for TestDir {
    fn default() -> Self {
        Self::new()
    }
}

pub trait TestFile {
    fn path(&self) -> PathBuf;
    fn content(&self) -> Vec<u8>;
}

impl<T, U> TestFile for (T, U)
where
    T: AsRef<str>,
    U: AsRef<[u8]>,
{
    fn path(&self) -> PathBuf {
        PathBuf::from(self.0.as_ref())
    }

    fn content(&self) -> Vec<u8> {
        self.1.as_ref().to_vec()
    }
}

/// Test helper that implements [EnvProvider], [HomeProvider], and [CwdProvider].
#[derive(Debug, Clone)]
pub struct TestSystem {
    env: std::collections::HashMap<String, String>,
    home: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl TestSystem {
    /// Creates a new implementation of [SystemProvider] with the following defaults:
    /// - env vars: HOME=/home/testuser
    /// - cwd: /home/testuser
    /// - home: /home/testuser
    pub fn new() -> Self {
        let mut env = std::collections::HashMap::new();
        env.insert("HOME".to_string(), "/home/testuser".to_string());
        Self {
            env,
            home: Some(PathBuf::from("/home/testuser")),
            cwd: Some(PathBuf::from("/home/testuser")),
        }
    }

    /// Creates a new implementation of [SystemProvider] with the following defaults:
    /// - env vars: HOME=$base/home/testuser
    /// - cwd: $base/home/testuser
    /// - home: $base/home/testuser
    pub fn new_with_base(base: impl AsRef<Path>) -> Self {
        let base = base.as_ref();
        let home = base.join("home/testuser");
        let mut env = std::collections::HashMap::new();
        env.insert("HOME".to_string(), home.to_string_lossy().to_string());
        Self {
            env,
            home: Some(home.clone()),
            cwd: Some(home),
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

impl Default for TestSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvProvider for TestSystem {
    fn var(&self, input: &str) -> Result<String, VarError> {
        self.env.get(input).cloned().ok_or(VarError::NotPresent)
    }
}

impl HomeProvider for TestSystem {
    fn home(&self) -> Option<PathBuf> {
        self.home.as_ref().cloned()
    }
}

impl CwdProvider for TestSystem {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        self.cwd.as_ref().cloned().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            eyre::eyre!("not found"),
        ))
    }
}
