use thiserror::Error;

/// Errors that can occur during the update process
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Failed to fetch manifest: {status} - {message}")]
    ManifestFetchFailed { status: u16, message: String },

    #[error("Failed to fetch manifest: {0}")]
    ManifestNetworkError(#[from] reqwest::Error),

    #[error("Invalid manifest JSON: {0}")]
    ManifestParseError(#[from] serde_json::Error),

    #[error("Invalid version string: {0}")]
    InvalidVersion(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Checksum verification failed: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Installation failed with exit code {code}: {message}")]
    InstallationFailed { code: i32, message: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsupported_platform_display() {
        let err = UpdateError::UnsupportedPlatform("freebsd".to_string());
        assert_eq!(err.to_string(), "Unsupported platform: freebsd");
    }

    #[test]
    fn test_manifest_fetch_failed_display() {
        let err = UpdateError::ManifestFetchFailed {
            status: 404,
            message: "Not Found".to_string(),
        };
        assert_eq!(err.to_string(), "Failed to fetch manifest: 404 - Not Found");
    }

    #[test]
    fn test_manifest_fetch_failed_contains_status_code() {
        let err = UpdateError::ManifestFetchFailed {
            status: 500,
            message: "Internal Server Error".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("500"), "Error message should contain status code");
    }

    #[test]
    fn test_invalid_version_display() {
        let err = UpdateError::InvalidVersion("not-a-version".to_string());
        assert_eq!(err.to_string(), "Invalid version string: not-a-version");
    }

    #[test]
    fn test_download_failed_display() {
        let err = UpdateError::DownloadFailed("Connection reset".to_string());
        assert_eq!(err.to_string(), "Download failed: Connection reset");
    }

    #[test]
    fn test_checksum_mismatch_display() {
        let err = UpdateError::ChecksumMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Checksum verification failed: expected abc123, got def456"
        );
    }

    #[test]
    fn test_installation_failed_display() {
        let err = UpdateError::InstallationFailed {
            code: 1,
            message: "Permission denied".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Installation failed with exit code 1: Permission denied"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let update_err: UpdateError = io_err.into();
        assert!(matches!(update_err, UpdateError::IoError(_)));
        assert!(update_err.to_string().contains("file not found"));
    }

    #[test]
    fn test_serde_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let update_err: UpdateError = json_err.into();
        assert!(matches!(update_err, UpdateError::ManifestParseError(_)));
        assert!(update_err.to_string().contains("Invalid manifest JSON"));
    }
}
