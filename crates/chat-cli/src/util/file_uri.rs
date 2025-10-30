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

    // Resolve the path
    let resolved_path = if path_str.starts_with('/') {
        // Absolute path
        PathBuf::from(path_str)
    } else {
        // Relative path - resolve relative to base_path
        base_path.join(path_str)
    };

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
                    path.starts_with("/"),
                    "Path should be absolute after tilde expansion, got: {:?}",
                    path
                );
                assert!(
                    !path.to_string_lossy().contains("~"),
                    "Path should not contain literal tilde, got: {:?}",
                    path
                );
            },
            _ => panic!("Expected FileNotFound error"),
        }

        Ok(())
    }
}
