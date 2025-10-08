use std::borrow::Cow;
use std::env::VarError;
use std::path::{
    Path,
    PathBuf,
};

use super::directories;
use super::error::{
    ErrorContext as _,
    UtilError,
};

/// Performs tilde and environment variable expansion on the provided input.
pub fn expand_path(input: &str) -> Result<Cow<'_, str>, UtilError> {
    let env_provider = |input: &str| Ok(std::env::var(input).ok());
    let home_provider = || directories::home_dir().map(|p| p.to_string_lossy().to_string()).ok();
    Ok(shellexpand::full_with_context(input, home_provider, env_provider)?)
}

/// Converts the given path to a normalized absolute path.
///
/// Internally, this function:
/// - Performs tilde expansion
/// - Performs env var expansion
/// - Resolves `.` and `..` path components
pub fn canonicalize_path(path: impl AsRef<str>) -> Result<String, UtilError> {
    let env_provider = |input: &str| Ok(std::env::var(input).ok());
    let home_provider = || directories::home_dir().map(|p| p.to_string_lossy().to_string()).ok();
    let cwd_provider = || std::env::current_dir().with_context(|| "could not get current directory".to_string());
    canonicalize_path_impl(path, env_provider, home_provider, cwd_provider)
}

pub fn canonicalize_path_impl<E, H, C>(
    path: impl AsRef<str>,
    env_provider: E,
    home_provider: H,
    cwd_provider: C,
) -> Result<String, UtilError>
where
    E: Fn(&str) -> Result<Option<String>, VarError>,
    H: Fn() -> Option<String>,
    C: Fn() -> Result<PathBuf, UtilError>,
{
    let expanded = shellexpand::full_with_context(path.as_ref(), home_provider, env_provider)?;
    let path_buf = if !expanded.starts_with("/") {
        // Convert relative paths to absolute paths
        let current_dir = cwd_provider()?;
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_canonicalize_path() {
        // test setup
        let env_vars = [
            ("TEST_VAR".to_string(), "test_var".to_string()),
            ("HOME".to_string(), "/home/testuser".to_string()),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();
        let env_provider = |var: &str| Ok(env_vars.get(var).cloned());
        let home_provider = || Some("/home/testuser".to_string());
        let cwd_provider = || Ok(PathBuf::from("/home/testuser/testdir"));

        let tests = [
            ("path", "/home/testuser/testdir/path"),
            ("../**/.rs", "/home/testuser/**/.rs"),
            ("~", "/home/testuser"),
            ("~/file/**.md", "/home/testuser/file/**.md"),
            ("~/.././../home//testuser/path/..", "/home/testuser"),
        ];

        for (path, expected) in tests {
            let actual = canonicalize_path_impl(path, env_provider, home_provider, cwd_provider).unwrap();
            assert_eq!(
                actual, expected,
                "Expected '{}' to expand to '{}', instead got '{}'",
                path, expected, actual
            );
        }
    }
}
