//! Module for common testing utilities

use std::env::VarError;
use std::path::{
    Path,
    PathBuf,
};

use super::path::canonicalize_path_sys;
use super::providers::{
    CwdProvider,
    EnvProvider,
    HomeProvider,
    SystemProvider,
};

/// Test helper that wraps a temporary directory and test [SystemProvider].
#[derive(Debug)]
pub struct TestBase {
    test_dir: TestDir,
    provider: TestProvider,
}

impl TestBase {
    /// Creates a new temporary directory with the following defaults configured:
    /// - env vars: HOME=$tempdir_path/home/testuser
    /// - cwd: $tempdir_path
    /// - home: $tempdir_path/home/testuser
    pub async fn new() -> Self {
        let test_dir = TestDir::new();
        let home_path = test_dir.path().join("home/testuser");
        tokio::fs::create_dir_all(&home_path)
            .await
            .expect("failed to create test home directory");
        let provider = TestProvider::new_with_base(home_path).with_cwd(test_dir.path());
        Self { test_dir, provider }
    }

    /// Returns a resolved path using the generated temporary directory as the base.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.test_dir.join(path)
    }

    pub fn provider(&self) -> &TestProvider {
        &self.provider
    }

    pub async fn with_file(mut self, file: impl TestFile) -> Self {
        self.test_dir = self.test_dir.with_file_sys(file, &self.provider).await;
        self
    }
}

impl EnvProvider for TestBase {
    fn var(&self, input: &str) -> Result<String, VarError> {
        self.provider.var(input)
    }
}

impl HomeProvider for TestBase {
    fn home(&self) -> Option<PathBuf> {
        self.provider.home()
    }
}

impl CwdProvider for TestBase {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        self.provider.cwd()
    }
}

impl SystemProvider for TestBase {}

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

    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Returns a resolved path using the generated temporary directory as the base.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.temp_dir.path().join(path)
    }

    /// Writes the given file under the test directory. Creates parent directories if needed.
    ///
    /// The path given by `file` is *not* canonicalized.
    #[deprecated]
    pub async fn with_file(self, file: impl TestFile) -> Self {
        let file_path = file.path();
        if file_path.is_absolute() && !file_path.starts_with(self.temp_dir.path()) {
            panic!("path falls outside of the temp dir");
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

    /// Writes the given file under the test directory. Creates parent directories if needed.
    ///
    /// This function panics if the file path is outside of the test directory.
    pub async fn with_file_sys<P: SystemProvider>(self, file: impl TestFile, provider: &P) -> Self {
        let file_path = canonicalize_path_sys(file.path().to_string_lossy(), provider).unwrap();

        // Check to ensure that the file path resolves under the test directory.
        if !file_path.starts_with(&self.temp_dir.path().to_string_lossy().to_string()) {
            panic!("outside of temp dir");
        }

        let file_path = PathBuf::from(file_path);
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.unwrap();
            }
        }

        tokio::fs::write(file_path, file.content()).await.unwrap();
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

impl TestFile for Box<dyn TestFile> {
    fn path(&self) -> PathBuf {
        (**self).path()
    }

    fn content(&self) -> Vec<u8> {
        (**self).content()
    }
}

/// Test helper that implements [EnvProvider], [HomeProvider], and [CwdProvider].
#[derive(Debug, Clone)]
pub struct TestProvider {
    env: std::collections::HashMap<String, String>,
    home: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl TestProvider {
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
    /// - env vars: HOME=$base
    /// - cwd: $base
    /// - home: $base
    ///
    /// `base` must be an absolute path, otherwise this method panics.
    pub fn new_with_base(base: impl AsRef<Path>) -> Self {
        let base = base.as_ref();
        if !base.is_absolute() {
            panic!("only absolute base paths are supported");
        }
        let mut env = std::collections::HashMap::new();
        env.insert("HOME".to_string(), base.to_string_lossy().to_string());
        Self {
            env,
            home: Some(base.to_owned()),
            cwd: Some(base.to_owned()),
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

impl Default for TestProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvProvider for TestProvider {
    fn var(&self, input: &str) -> Result<String, VarError> {
        self.env.get(input).cloned().ok_or(VarError::NotPresent)
    }
}

impl HomeProvider for TestProvider {
    fn home(&self) -> Option<PathBuf> {
        self.home.as_ref().cloned()
    }
}

impl CwdProvider for TestProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        self.cwd.as_ref().cloned().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            eyre::eyre!("not found"),
        ))
    }
}

impl SystemProvider for TestProvider {}

#[cfg(test)]
mod tests {
    use tokio::fs;

    use super::*;

    #[tokio::test]
    async fn test_tempdir_files() {
        let mut test_dir = TestDir::new();
        let test_provider = TestProvider::new_with_base(test_dir.path());

        let files = [("base", "base"), ("~/tilde", "tilde"), ("$HOME/home", "home")];
        for file in files {
            test_dir = test_dir.with_file_sys(file, &test_provider).await;
        }

        assert_eq!(fs::read_to_string(test_dir.join("base")).await.unwrap(), "base");
        assert_eq!(fs::read_to_string(test_dir.join("tilde")).await.unwrap(), "tilde");
        assert_eq!(fs::read_to_string(test_dir.join("home")).await.unwrap(), "home");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_tempdir_write_file_outside() {
        let test_dir = TestDir::new();
        let test_provider = TestProvider::new_with_base(test_dir.path());

        let _ = test_dir.with_file_sys(("..", "hello"), &test_provider).await;
    }
}
