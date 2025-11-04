use std::borrow::Cow;
use std::env::VarError;
use std::path::{
    Path,
    PathBuf,
};

use super::error::{
    ErrorContext as _,
    UtilError,
};
use super::providers::{
    EnvProvider,
    HomeProvider,
    RealProvider,
    SystemProvider,
};

/// Performs tilde and environment variable expansion on the provided input.
pub fn expand_path<'a>(input: &'a str, provider: &'_ impl SystemProvider) -> Result<Cow<'a, str>, UtilError> {
    Ok(shellexpand::full_with_context(
        input,
        shellexpand_home(provider),
        shellexpand_context(provider),
    )?)
}

/// Converts the given path to a normalized absolute path.
///
/// Internally, this function:
/// - Performs tilde expansion
/// - Performs env var expansion
/// - Resolves `.` and `..` path components
pub fn canonicalize_path(path: impl AsRef<str>) -> Result<String, UtilError> {
    let sys = RealProvider;
    canonicalize_path_sys(path, &sys)
}

pub fn canonicalize_path_sys<P: SystemProvider>(path: impl AsRef<str>, provider: &P) -> Result<String, UtilError> {
    let expanded =
        shellexpand::full_with_context(path.as_ref(), shellexpand_home(provider), shellexpand_context(provider))?;
    let path_buf = if !expanded.starts_with("/") {
        // Convert relative paths to absolute paths
        let current_dir = provider
            .cwd()
            .with_context(|| "could not get current directory".to_string())?;
        current_dir.join(expanded.as_ref() as &str)
    } else {
        // Already absolute path
        PathBuf::from(expanded.as_ref() as &str)
    };

    // Try canonicalize first, fallback to manual normalization if it fails
    match path_buf.canonicalize() {
        Ok(normalized) => Ok(normalized.as_path().to_string_lossy().to_string()),
        Err(_) => {
            // If canonicalize fails (e.g., path doesn't exist), do manual normalization
            let normalized = normalize_path(&path_buf);
            Ok(normalized.to_string_lossy().to_string())
        },
    }
}

/// Manually normalize a path by resolving . and .. components
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory components
            },
            std::path::Component::ParentDir => {
                // Pop the last component for parent directory
                components.pop();
            },
            _ => {
                components.push(component);
            },
        }
    }
    components.iter().collect()
}

/// Helper for [shellexpand::full_with_context]
fn shellexpand_home<H: HomeProvider>(provider: &H) -> impl Fn() -> Option<String> {
    || HomeProvider::home(provider).map(|h| h.to_string_lossy().to_string())
}

/// Helper for [shellexpand::full_with_context]
fn shellexpand_context<E: EnvProvider>(provider: &E) -> impl Fn(&str) -> Result<Option<String>, VarError> {
    |input: &str| Ok(EnvProvider::var(provider, input).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestProvider;

    #[test]
    fn test_canonicalize_path() {
        let sys = TestProvider::new()
            .with_var("TEST_VAR", "test_var")
            .with_cwd("/home/testuser/testdir");

        let tests = [
            ("path", "/home/testuser/testdir/path"),
            ("../**/.rs", "/home/testuser/**/.rs"),
            ("~", "/home/testuser"),
            ("~/file/**.md", "/home/testuser/file/**.md"),
            ("~/.././../home//testuser/path/..", "/home/testuser"),
        ];

        for (path, expected) in tests {
            let actual = canonicalize_path_sys(path, &sys).unwrap();
            assert_eq!(
                actual, expected,
                "Expected '{}' to expand to '{}', instead got '{}'",
                path, expected, actual
            );
        }
    }
}
