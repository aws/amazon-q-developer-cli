//! Version manifest data models and fetching functionality.
//!
//! This module provides types for parsing the S3-hosted version manifest
//! and a fetcher for downloading it.

use serde::{
    Deserialize,
    Serialize,
};

use super::UpdateError;

/// A single artifact entry in the version manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ArtifactEntry {
    /// Installer kind (e.g., "deb", "msi", "pkg", "tarXz")
    pub kind: String,

    /// Rust target triple (e.g., "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc")
    #[serde(rename = "targetTriple")]
    pub target_triple: String,

    /// Operating system (e.g., "linux", "windows", "macos")
    pub os: String,

    /// File type (e.g., "tarXz", "msi", "pkg")
    #[serde(rename = "fileType")]
    pub file_type: String,

    /// CPU architecture (e.g., "x86_64", "aarch64")
    pub architecture: String,

    /// Build variant (e.g., "headless", "full")
    pub variant: String,

    /// Relative download path (e.g., "nightly/1.27.1/kiro-cli-x86_64-linux.tar.xz")
    pub download: String,

    /// SHA256 checksum of the artifact (hex-encoded)
    pub sha256: String,

    /// File size in bytes
    pub size: u64,

    /// Release channel (e.g., "nightly", "stable")
    pub channel: String,
}

/// The version manifest: a list of available artifacts.
///
/// The manifest is an array of `ArtifactEntry` objects. The update logic
/// finds the appropriate entry by matching `os` and `architecture` to the
/// current platform.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct VersionManifest {
    /// Latest available version (semantic version string)
    pub version: String,

    /// List of available artifacts for this version
    pub packages: Vec<ArtifactEntry>,
}

impl VersionManifest {
    /// Find the artifact matching the given OS and architecture.
    ///
    /// Returns the first matching artifact, or `None` if no match is found.
    pub fn find_artifact(&self, os: &str, architecture: &str) -> Option<&ArtifactEntry> {
        self.packages
            .iter()
            .find(|a| a.os == os && a.architecture == architecture)
    }
}

/// Fetches the version manifest from a remote URL.
pub struct ManifestFetcher {
    client: reqwest::Client,
}

impl ManifestFetcher {
    /// Creates a new ManifestFetcher with a default HTTP client.
    pub fn new() -> Result<Self, UpdateError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(UpdateError::ManifestNetworkError)?;
        Ok(Self { client })
    }

    /// Fetches and parses the version manifest from the given URL.
    pub async fn fetch(&self, url: &str) -> Result<VersionManifest, UpdateError> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::ManifestFetchFailed {
                status: response.status().as_u16(),
                message: response.status().canonical_reason().unwrap_or("Unknown").to_string(),
            });
        }

        let text = response.text().await?;
        let manifest: VersionManifest = serde_json::from_str(&text)?;
        Ok(manifest)
    }
}

impl Default for ManifestFetcher {
    fn default() -> Self {
        Self::new().expect("Failed to create default ManifestFetcher")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Unit Tests
    // =========================================================================

    fn sample_manifest_json() -> &'static str {
        r#"{
            "version": "1.27.1",
            "packages": [
                {
                    "kind": "deb",
                    "targetTriple": "x86_64-unknown-linux-gnu",
                    "os": "linux",
                    "fileType": "tarXz",
                    "architecture": "x86_64",
                    "variant": "headless",
                    "download": "nightly/1.27.1/kirocli-x86_64-linux.tar.xz",
                    "sha256": "abc123",
                    "size": 189423512,
                    "channel": "nightly"
                },
                {
                    "kind": "msi",
                    "targetTriple": "x86_64-pc-windows-msvc",
                    "os": "windows",
                    "fileType": "msi",
                    "architecture": "x86_64",
                    "variant": "full",
                    "download": "nightly/1.27.1/kiro-cli-x86_64-pc-windows-msvc.msi",
                    "sha256": "def456",
                    "size": 95000000,
                    "channel": "nightly"
                }
            ]
        }"#
    }

    #[test]
    fn test_parse_valid_manifest() {
        let manifest: VersionManifest = serde_json::from_str(sample_manifest_json()).unwrap();
        assert_eq!(manifest.version, "1.27.1");
        assert_eq!(manifest.packages.len(), 2);
        assert_eq!(manifest.packages[0].os, "linux");
        assert_eq!(manifest.packages[0].architecture, "x86_64");
        assert_eq!(manifest.packages[1].os, "windows");
        assert_eq!(manifest.packages[1].kind, "msi");
    }

    #[test]
    fn test_find_artifact_linux() {
        let manifest: VersionManifest = serde_json::from_str(sample_manifest_json()).unwrap();
        let artifact = manifest.find_artifact("linux", "x86_64");
        assert!(artifact.is_some());
        assert_eq!(artifact.unwrap().kind, "deb");
    }

    #[test]
    fn test_find_artifact_windows() {
        let manifest: VersionManifest = serde_json::from_str(sample_manifest_json()).unwrap();
        let artifact = manifest.find_artifact("windows", "x86_64");
        assert!(artifact.is_some());
        assert_eq!(artifact.unwrap().kind, "msi");
    }

    #[test]
    fn test_find_artifact_not_found() {
        let manifest: VersionManifest = serde_json::from_str(sample_manifest_json()).unwrap();
        let artifact = manifest.find_artifact("macos", "aarch64");
        assert!(artifact.is_none());
    }

    #[test]
    fn test_parse_malformed_json() {
        let result: Result<VersionManifest, _> = serde_json::from_str("{ invalid }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_artifacts() {
        let json = r#"{ "version": "1.0.0", "packages": [] }"#;
        let manifest: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.packages.len(), 0);
    }

    #[tokio::test]
    async fn test_fetch_http_404() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/manifest.json")
            .with_status(404)
            .create_async()
            .await;
        let fetcher = ManifestFetcher::new().unwrap();
        let result = fetcher.fetch(&format!("{}/manifest.json", server.url())).await;
        assert!(matches!(
            result,
            Err(UpdateError::ManifestFetchFailed { status: 404, .. })
        ));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_http_500() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/manifest.json")
            .with_status(500)
            .create_async()
            .await;
        let fetcher = ManifestFetcher::new().unwrap();
        let result = fetcher.fetch(&format!("{}/manifest.json", server.url())).await;
        assert!(matches!(
            result,
            Err(UpdateError::ManifestFetchFailed { status: 500, .. })
        ));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/manifest.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(sample_manifest_json())
            .create_async()
            .await;
        let fetcher = ManifestFetcher::new().unwrap();
        let manifest = fetcher.fetch(&format!("{}/manifest.json", server.url())).await.unwrap();
        assert_eq!(manifest.version, "1.27.1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_malformed_json_response() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/manifest.json")
            .with_status(200)
            .with_body("{ not valid }")
            .create_async()
            .await;
        let fetcher = ManifestFetcher::new().unwrap();
        let result = fetcher.fetch(&format!("{}/manifest.json", server.url())).await;
        assert!(matches!(result, Err(UpdateError::ManifestParseError(_))));
        mock.assert_async().await;
    }

    #[test]
    fn http_error_contains_status_code() {
        for (status, message) in [(400, "Bad Request"), (404, "Not Found"), (500, "Internal Server Error")] {
            let err = UpdateError::ManifestFetchFailed {
                status,
                message: message.to_string(),
            };
            assert!(err.to_string().contains(&status.to_string()));
        }
    }
}
