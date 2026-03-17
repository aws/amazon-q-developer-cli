use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use eyre::Result;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileUriError {
    #[error("Invalid file URI format: {uri}")]
    InvalidUri { uri: String },
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    #[error("Failed to read file {path}: {source}")]
    ReadError { path: PathBuf, source: std::io::Error },
}

/// Resolves a file:// URI to its content, supporting both relative and absolute paths.
///
/// # Arguments
/// * `uri` - The file:// URI to resolve
/// * `base_path` - Base path for resolving relative URIs (typically the agent config file
///   directory)
///
/// # Returns
/// The content of the file as a String
pub fn resolve_file_uri(uri: &str, base_path: &Path) -> Result<String, FileUriError> {
    // Validate URI format
    if !uri.starts_with("file://") {
        return Err(FileUriError::InvalidUri { uri: uri.to_string() });
    }

    // Extract the path part after "file://"
    let path_str = uri.trim_start_matches("file://");

    // Handle empty path
    if path_str.is_empty() {
        return Err(FileUriError::InvalidUri { uri: uri.to_string() });
    }

    // Expand tilde to home directory
    let path_str = shellexpand::tilde(path_str).to_string();

    // Normalize forward slashes to platform separator for cross-platform compatibility.
    // File URIs always use forward slashes, but on Windows we need backslashes for
    // proper path resolution (especially when the path contains .. components).
    let path_str = path_str.replace('/', std::path::MAIN_SEPARATOR_STR);

    // Resolve the path
    let resolved_path = if Path::new(&path_str).is_absolute() {
        // Absolute path
        PathBuf::from(path_str)
    } else {
        // Relative path - resolve relative to base_path
        base_path.join(path_str)
    };

    // Normalize the path to resolve .. and . components without requiring the file to exist.
    // This is important on Windows where mixed separators and unresolved .. can cause
    // path resolution failures (e.g. C:\Users\foo\.kiro\agents\..\..\file.md).
    let resolved_path = normalize_path(&resolved_path);

    // Check if file exists
    if !resolved_path.exists() {
        return Err(FileUriError::FileNotFound { path: resolved_path });
    }

    // Check if it's a file (not a directory)
    if !resolved_path.is_file() {
        return Err(FileUriError::FileNotFound { path: resolved_path });
    }

    // Read the file content
    fs::read_to_string(&resolved_path).map_err(|source| FileUriError::ReadError {
        path: resolved_path,
        source,
    })
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
/// Unlike `canonicalize()`, this works even if the path doesn't exist yet and doesn't
/// produce UNC paths on Windows.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // Pop the last component if it's a normal dir, otherwise keep the ..
                if components
                    .last()
                    .is_some_and(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    components.pop();
                } else {
                    components.push(component);
                }
            },
            std::path::Component::CurDir => {
                // Skip . components
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
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_invalid_uri_format() {
        let base = Path::new("/tmp");

        // Not a file:// URI
        let result = resolve_file_uri("http://example.com", base);
        assert!(matches!(result, Err(FileUriError::InvalidUri { .. })));

        // Empty path
        let result = resolve_file_uri("file://", base);
        assert!(matches!(result, Err(FileUriError::InvalidUri { .. })));
    }

    #[test]
    fn test_file_not_found() {
        let base = Path::new("/tmp");

        let result = resolve_file_uri("file:///nonexistent/file.txt", base);
        assert!(matches!(result, Err(FileUriError::FileNotFound { .. })));
    }

    #[test]
    fn test_absolute_path_resolution() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello, World!";
        fs::write(&file_path, content)?;

        let uri = format!("file://{}", file_path.display());
        let base = Path::new("/some/other/path");

        let result = resolve_file_uri(&uri, base)?;
        assert_eq!(result, content);

        Ok(())
    }

    #[test]
    fn test_relative_path_resolution() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("subdir").join("test.txt");
        fs::create_dir_all(file_path.parent().unwrap())?;
        let content = "Relative content";
        fs::write(&file_path, content)?;

        let uri = "file://subdir/test.txt";
        let base = temp_dir.path();

        let result = resolve_file_uri(uri, base)?;
        assert_eq!(result, content);

        Ok(())
    }

    #[test]
    fn test_directory_instead_of_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path().join("testdir");
        fs::create_dir(&dir_path)?;

        let uri = format!("file://{}", dir_path.display());
        let base = Path::new("/tmp");

        let result = resolve_file_uri(&uri, base);
        assert!(matches!(result, Err(FileUriError::FileNotFound { .. })));

        Ok(())
    }

    #[test]
    fn test_tilde_expansion() -> Result<(), Box<dyn std::error::Error>> {
        // Test that tilde gets expanded by verifying the path is absolute after expansion
        // We can't easily mock HOME and don't want to write test files there, but we can verify
        // the expansion behavior using error messages
        let uri = "file://~/test.txt";
        let base = Path::new("/some/other/path");

        // This will fail to find the file (expected), but the error should show
        // an expanded absolute path, not a path with literal ~
        let result = resolve_file_uri(uri, base);

        match result {
            Err(FileUriError::FileNotFound { path }) => {
                // Verify the path was expanded (should start with / not ~)
                assert!(
                    path.is_absolute(),
                    "Path should be absolute after tilde expansion, got: {path:?}"
                );
                assert!(
                    !path.to_string_lossy().contains("~"),
                    "Path should not contain literal tilde, got: {path:?}"
                );
            },
            _ => panic!("Expected FileNotFound error"),
        }

        Ok(())
    }

    #[test]
    fn test_relative_path_with_parent_dir() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        // Create file at temp_dir/test.txt
        let file_path = temp_dir.path().join("test.txt");
        let content = "Parent dir content";
        fs::write(&file_path, content)?;

        // Create a subdirectory to use as base_path
        let sub_dir = temp_dir.path().join("sub").join("dir");
        fs::create_dir_all(&sub_dir)?;

        // URI with .. should resolve from sub/dir back to temp_dir
        let uri = "file://../../test.txt";
        let result = resolve_file_uri(uri, &sub_dir)?;
        assert_eq!(result, content);

        Ok(())
    }

    #[test]
    fn test_normalize_path_resolves_parent_components() {
        let path = PathBuf::from("/a/b/c/../../d");
        assert_eq!(normalize_path(&path), PathBuf::from("/a/d"));
    }

    #[test]
    fn test_normalize_path_resolves_current_dir() {
        let path = PathBuf::from("/a/./b/./c");
        assert_eq!(normalize_path(&path), PathBuf::from("/a/b/c"));
    }
}
