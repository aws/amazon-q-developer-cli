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

/// Convenience wrapper around [`resolve_path_fuzzy`] using the real system provider.
pub fn resolve_path_fuzzy_real(path: impl AsRef<str>) -> Result<String, UtilError> {
    let sys = RealProvider;
    resolve_path_fuzzy(path, &sys)
}

pub fn canonicalize_path_sys<P: SystemProvider>(path: impl AsRef<str>, provider: &P) -> Result<String, UtilError> {
    let expanded =
        shellexpand::full_with_context(path.as_ref(), shellexpand_home(provider), shellexpand_context(provider))?;
    #[cfg(windows)]
    let path_buf = if Path::new(expanded.as_ref() as &str).is_absolute() {
        PathBuf::from(expanded.as_ref() as &str)
    } else {
        let current_dir = provider
            .cwd()
            .with_context(|| "could not get current directory".to_string())?;
        current_dir.join(expanded.as_ref() as &str)
    };
    #[cfg(not(windows))]
    let path_buf = if expanded.starts_with("/") {
        // Already absolute path
        PathBuf::from(expanded.as_ref() as &str)
    } else {
        // Convert relative paths to absolute paths
        let current_dir = provider
            .cwd()
            .with_context(|| "could not get current directory".to_string())?;
        current_dir.join(expanded.as_ref() as &str)
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

/// Normalize all Unicode whitespace characters to ASCII space for comparison.
/// Safer than stripping — preserves space positions so "my file.txt" won't
/// accidentally match "myfile.txt". Only matches when whitespace differs in
/// type (e.g., U+202F vs U+0020), not in position.
fn normalize_whitespace(s: &str) -> String {
    s.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect()
}

/// Try to find a file in the parent directory that matches after normalizing
/// Unicode whitespace. Handles cases where filenames contain special whitespace
/// characters but the LLM outputs regular ASCII spaces.
fn try_fuzzy_whitespace_match(path: &Path) -> Option<PathBuf> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;
    let filename = path.file_name()?.to_str()?;
    let normalized = normalize_whitespace(filename);

    let mut matched: Option<PathBuf> = None;
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let entry_name = entry.file_name();
        let Some(entry_str) = entry_name.to_str() else {
            continue;
        };
        if normalize_whitespace(entry_str) == normalized {
            if matched.is_some() {
                // Multiple fuzzy matches — ambiguous, bail out
                return None;
            }
            let actual = entry.path();
            matched = Some(actual.canonicalize().ok().unwrap_or(actual));
        }
    }
    matched
}

/// Resolve a path with fuzzy Unicode whitespace matching.
///
/// First attempts exact canonicalization via [`canonicalize_path_sys`]. If the
/// path doesn't exist, falls back to scanning the parent directory for a file
/// whose name matches after normalizing Unicode whitespace to ASCII spaces.
///
/// Use this instead of `canonicalize_path_sys` when the path originates from
/// an LLM tool call, where Unicode whitespace variants (U+202F, U+00A0) in
/// real filenames may have been replaced with regular ASCII spaces.
pub fn resolve_path_fuzzy<P: SystemProvider>(path: impl AsRef<str>, provider: &P) -> Result<String, UtilError> {
    let canonical = canonicalize_path_sys(path, provider)?;
    let canonical_path = Path::new(&canonical);
    if canonical_path.exists() {
        return Ok(canonical);
    }
    // Exact path doesn't exist — try fuzzy whitespace matching
    if let Some(matched) = try_fuzzy_whitespace_match(canonical_path) {
        return Ok(matched.to_string_lossy().to_string());
    }
    Ok(canonical)
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

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello world"), "hello world");
        assert_eq!(normalize_whitespace("no\u{202F}break"), "no break");
        assert_eq!(normalize_whitespace("no\u{00A0}break"), "no break");
        assert_eq!(normalize_whitespace("a\u{202F}b\u{00A0}c"), "a b c");
    }

    #[test]
    fn test_fuzzy_whitespace_match_finds_unicode_spaces() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file with narrow no-break space (U+202F) — like macOS screenshots
        let actual_name = "Screenshot\u{202F}2026-03-22.png";
        std::fs::write(dir.path().join(actual_name), b"test").unwrap();

        // Query with regular ASCII space
        let query_path = dir.path().join("Screenshot 2026-03-22.png");
        let result = try_fuzzy_whitespace_match(&query_path);
        assert!(result.is_some(), "should find file with Unicode whitespace");
        assert!(result.unwrap().exists(), "matched path should exist");
    }

    #[test]
    fn test_fuzzy_whitespace_match_returns_none_for_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other_file.txt"), b"test").unwrap();

        let query_path = dir.path().join("nonexistent file.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }

    #[test]
    fn test_fuzzy_whitespace_match_exact_match_not_needed() {
        let dir = tempfile::tempdir().unwrap();
        // File exists with exact name — canonicalize would succeed, so fuzzy
        // match shouldn't be needed, but it should still work if called
        let name = "file with spaces.txt";
        std::fs::write(dir.path().join(name), b"test").unwrap();

        let query_path = dir.path().join(name);
        let result = try_fuzzy_whitespace_match(&query_path);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_whitespace_match_ambiguous_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // Two files that differ only in whitespace type — ambiguous
        std::fs::write(dir.path().join("file name.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("file\u{00A0}name.txt"), b"b").unwrap();

        // Both normalize to "file name.txt" — should return None (ambiguous)
        let query_path = dir.path().join("file\u{202F}name.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }

    #[test]
    fn test_fuzzy_whitespace_match_no_false_positive_on_missing_spaces() {
        let dir = tempfile::tempdir().unwrap();
        // "myfile.txt" exists but user asked for "my file.txt"
        // Normalize preserves space positions, so these should NOT match
        std::fs::write(dir.path().join("myfile.txt"), b"test").unwrap();

        let query_path = dir.path().join("my file.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }
}
