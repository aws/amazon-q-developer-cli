//! Installer download functionality with checksum verification.
//!
//! This module provides the `InstallerDownloader` for downloading platform-specific
//! installers from S3 with progress display and SHA256 checksum verification.

use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use futures::StreamExt;
use sha2::{
    Digest,
    Sha256,
};
use tracing::warn;

use super::UpdateError;

/// RAII wrapper for temporary installer files.
///
/// Automatically deletes the temporary file when dropped, ensuring cleanup
/// happens regardless of whether the installation succeeds or fails.
pub struct TempInstallerPath {
    path: PathBuf,
    /// When true, the file will not be deleted on drop (e.g., after successful handoff)
    preserve: bool,
}

impl TempInstallerPath {
    /// Creates a new TempInstallerPath for the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path, preserve: false }
    }

    /// Returns a reference to the underlying path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consumes self and returns the path, preventing automatic cleanup.
    ///
    /// Use this when handing off the file to another component that will
    /// manage its lifecycle.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))] // Used by Windows MSI code path and tests
    pub fn into_path(mut self) -> PathBuf {
        self.preserve = true;
        std::mem::take(&mut self.path)
    }
}

impl Drop for TempInstallerPath {
    fn drop(&mut self) {
        if self.preserve {
            return;
        }
        if self.path.exists()
            && let Err(e) = std::fs::remove_file(&self.path)
        {
            warn!("Failed to cleanup temp installer at {:?}: {}", self.path, e);
        }
    }
}

/// Downloads installers from remote URLs with progress display and checksum verification.
pub struct InstallerDownloader {
    client: reqwest::Client,
}

impl InstallerDownloader {
    /// Creates a new InstallerDownloader with a default HTTP client.
    pub fn new() -> Result<Self, UpdateError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;
        Ok(Self { client })
    }

    /// Downloads an installer to a temporary file with progress display and checksum verification.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to download the installer from
    /// * `expected_sha256` - The expected SHA256 checksum (hex-encoded)
    /// * `progress_callback` - Optional callback for progress updates (bytes_downloaded,
    ///   total_bytes)
    ///
    /// # Returns
    ///
    /// Returns a `TempInstallerPath` pointing to the downloaded file on success.
    ///
    /// # Errors
    ///
    /// * `UpdateError::DownloadFailed` - If the download fails
    /// * `UpdateError::ChecksumMismatch` - If the checksum doesn't match
    /// * `UpdateError::IoError` - If there's a filesystem error
    pub async fn download<F>(
        &self,
        url: &str,
        expected_sha256: &str,
        progress_callback: Option<F>,
    ) -> Result<TempInstallerPath, UpdateError>
    where
        F: Fn(u64, u64),
    {
        // Start the download request
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UpdateError::DownloadFailed(format!(
                "HTTP {} - {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Get content length for progress reporting
        let total_size = response.content_length().unwrap_or(0);

        // Create temp file with appropriate extension based on URL
        let extension = url
            .rsplit('/')
            .next()
            .and_then(|filename| filename.rsplit('.').next())
            .unwrap_or("tmp");

        let temp_path = std::env::temp_dir().join(format!("kiro-installer-{}.{}", uuid::Uuid::new_v4(), extension));

        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| UpdateError::IoError(std::io::Error::other(e.to_string())))?;

        // Stream download while computing hash
        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

            // Write to file
            file.write_all(&chunk)?;

            // Update hash
            hasher.update(&chunk);

            // Update progress
            downloaded += chunk.len() as u64;
            if let Some(ref callback) = progress_callback {
                callback(downloaded, total_size);
            }
        }

        // Ensure all data is flushed to disk
        file.flush()?;
        drop(file);

        // Compute final hash
        let computed_hash = hex::encode(hasher.finalize());

        // Verify checksum (case-insensitive comparison)
        if !computed_hash.eq_ignore_ascii_case(expected_sha256) {
            // Delete the file on checksum mismatch
            if let Err(e) = std::fs::remove_file(&temp_path) {
                warn!("Failed to delete file after checksum mismatch: {}", e);
            }
            return Err(UpdateError::ChecksumMismatch {
                expected: expected_sha256.to_lowercase(),
                actual: computed_hash.to_lowercase(),
            });
        }

        Ok(TempInstallerPath::new(temp_path))
    }
}

impl Default for InstallerDownloader {
    fn default() -> Self {
        Self::new().expect("Failed to create default InstallerDownloader")
    }
}

/// Computes the SHA256 hash of a byte slice.
///
/// This is a utility function exposed for testing and verification purposes.
#[cfg(test)]
pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Verifies that a computed hash matches an expected hash (case-insensitive).
///
/// Returns `Ok(())` if the hashes match, or `Err(UpdateError::ChecksumMismatch)` if they don't.
#[cfg(test)]
pub fn verify_checksum(computed: &str, expected: &str) -> Result<(), UpdateError> {
    if computed.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(UpdateError::ChecksumMismatch {
            expected: expected.to_lowercase(),
            actual: computed.to_lowercase(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    // =========================================================================
    // Unit Tests
    // =========================================================================

    #[test]
    fn test_compute_sha256_empty() {
        let hash = compute_sha256(b"");
        // SHA256 of empty string
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_sha256_hello_world() {
        let hash = compute_sha256(b"hello world");
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_verify_checksum_matching() {
        let hash = "abc123def456";
        assert!(verify_checksum(hash, hash).is_ok());
    }

    #[test]
    fn test_verify_checksum_case_insensitive() {
        assert!(verify_checksum("ABC123DEF456", "abc123def456").is_ok());
        assert!(verify_checksum("abc123def456", "ABC123DEF456").is_ok());
        assert!(verify_checksum("AbC123DeF456", "aBc123dEf456").is_ok());
    }

    #[test]
    fn test_verify_checksum_mismatch() {
        let result = verify_checksum("abc123", "def456");
        assert!(result.is_err());
        if let Err(UpdateError::ChecksumMismatch { expected, actual }) = result {
            assert_eq!(expected, "def456");
            assert_eq!(actual, "abc123");
        } else {
            panic!("Expected ChecksumMismatch error");
        }
    }

    #[test]
    fn test_temp_installer_path_cleanup_on_drop() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test-installer.msi");

        // Create a file
        fs::write(&file_path, b"test content").unwrap();
        assert!(file_path.exists());

        // Create TempInstallerPath and drop it
        {
            let _temp = TempInstallerPath::new(file_path.clone());
            assert!(file_path.exists()); // Still exists while in scope
        }

        // File should be deleted after drop
        assert!(!file_path.exists());
    }

    #[test]
    fn test_temp_installer_path_preserve() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test-installer.pkg");

        // Create a file
        fs::write(&file_path, b"test content").unwrap();
        assert!(file_path.exists());

        // Create TempInstallerPath and preserve it
        let returned_path = {
            let temp = TempInstallerPath::new(file_path.clone());
            temp.into_path()
        };

        // File should still exist after drop because we called into_path()
        assert!(returned_path.exists());
        assert_eq!(returned_path, file_path);

        // Clean up manually
        fs::remove_file(&returned_path).unwrap();
    }

    #[test]
    fn test_temp_installer_path_nonexistent_file() {
        // Should not panic when dropping a TempInstallerPath for a file that doesn't exist
        let path = PathBuf::from("/nonexistent/path/to/installer.msi");
        let _temp = TempInstallerPath::new(path);
        // Drop happens here - should not panic
    }

    #[test]
    fn test_temp_installer_path_path_accessor() {
        let path = PathBuf::from("/some/path/installer.pkg");
        let temp = TempInstallerPath::new(path.clone());
        assert_eq!(temp.path(), &path);
    }

    #[tokio::test]
    async fn test_download_http_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/installer.msi")
            .with_status(404)
            .create_async()
            .await;

        let downloader = InstallerDownloader::new().unwrap();
        let result = downloader
            .download::<fn(u64, u64)>(&format!("{}/installer.msi", server.url()), "abc123", None)
            .await;

        assert!(result.is_err());
        if let Err(UpdateError::DownloadFailed(msg)) = result {
            assert!(msg.contains("404"));
        } else {
            panic!("Expected DownloadFailed error");
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_download_success_with_valid_checksum() {
        let mut server = mockito::Server::new_async().await;
        let content = b"test installer content";
        let expected_hash = compute_sha256(content);

        let mock = server
            .mock("GET", "/installer.pkg")
            .with_status(200)
            .with_header("content-length", &content.len().to_string())
            .with_body(content)
            .create_async()
            .await;

        let downloader = InstallerDownloader::new().unwrap();
        let result = downloader
            .download::<fn(u64, u64)>(&format!("{}/installer.pkg", server.url()), &expected_hash, None)
            .await;

        assert!(result.is_ok());
        let temp_path = result.unwrap();

        // Verify file exists and has correct content
        let downloaded_content = fs::read(temp_path.path()).unwrap();
        assert_eq!(downloaded_content, content);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_download_checksum_mismatch_deletes_file() {
        let mut server = mockito::Server::new_async().await;
        let content = b"test installer content";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let mock = server
            .mock("GET", "/installer.tar.gz")
            .with_status(200)
            .with_body(content)
            .create_async()
            .await;

        let downloader = InstallerDownloader::new().unwrap();
        let result = downloader
            .download::<fn(u64, u64)>(&format!("{}/installer.tar.gz", server.url()), wrong_hash, None)
            .await;

        assert!(result.is_err());
        if let Err(UpdateError::ChecksumMismatch { expected, actual }) = result {
            assert_eq!(expected, wrong_hash);
            assert_eq!(actual, compute_sha256(content));
        } else {
            panic!("Expected ChecksumMismatch error");
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_download_progress_callback() {
        let mut server = mockito::Server::new_async().await;
        let content = b"test content for progress";
        let expected_hash = compute_sha256(content);

        let mock = server
            .mock("GET", "/installer.msi")
            .with_status(200)
            .with_header("content-length", &content.len().to_string())
            .with_body(content)
            .create_async()
            .await;

        let downloader = InstallerDownloader::new().unwrap();

        let progress_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let progress_called_clone = progress_called.clone();

        let result = downloader
            .download(
                &format!("{}/installer.msi", server.url()),
                &expected_hash,
                Some(move |downloaded, total| {
                    progress_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    assert!(downloaded <= total || total == 0);
                }),
            )
            .await;

        assert!(result.is_ok());
        assert!(progress_called.load(std::sync::atomic::Ordering::SeqCst));

        mock.assert_async().await;
    }
}
