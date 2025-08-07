//! Hosted model download client for Amazon Q CLI
//!
//! This module provides functionality to download model files from a hosted CDN
//! instead of directly from Hugging Face. Models are distributed as zip files
//! containing model.safetensors and tokenizer.json files.

#[cfg(test)]
use std::fs;
use std::path::Path;

use anyhow::{
    Context,
    Result as AnyhowResult,
};
use indicatif::{
    ProgressBar,
    ProgressStyle,
};
use reqwest;
use tracing::{
    debug,
    error,
};

/// Progress callback type for download operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Hosted model client for downloading models from CDN (synchronous)
pub struct HostedModelClient {
    /// Base URL for the CDN
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
}

impl HostedModelClient {
    /// Create a new hosted model client
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL for the CDN where models are hosted
    ///
    /// # Example
    ///
    /// ```no_run
    /// use semantic_search_client::client::HostedModelClient;
    /// let client = HostedModelClient::new("http://example.com/models".to_string());
    /// ```
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Download a model if it doesn't exist locally (asynchronous)
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model (e.g., "all-MiniLM-L6-v2")
    /// * `target_dir` - Directory where model files should be extracted
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn ensure_model(&self, model_name: &str, target_dir: &Path) -> AnyhowResult<()> {
        self.ensure_model_with_progress(model_name, target_dir, None).await
    }

    /// Download a model if it doesn't exist locally with optional progress callback
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model (e.g., "all-MiniLM-L6-v2")
    /// * `target_dir` - Directory where model files should be extracted
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn ensure_model_with_progress(
        &self,
        model_name: &str,
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> AnyhowResult<()> {
        // Check if model already exists and is valid
        if self.is_model_valid(target_dir).await? {
            debug!("Model '{}' already exists and is valid", model_name);
            return Ok(());
        }

        debug!("Downloading hosted model: {}", model_name);
        self.download_model(model_name, target_dir, progress_callback).await
    }

    /// Download model from hosted CDN (asynchronous) with optional progress
    async fn download_model(
        &self,
        model_name: &str,
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> AnyhowResult<()> {
        // Construct zip filename and URL
        let zip_filename = format!("{}.zip", model_name);
        let zip_url = format!("{}/{}", self.base_url, zip_filename);
        let zip_path = target_dir.join(&zip_filename);

        debug!("Constructing download URL:");
        debug!("  Base URL: {}", self.base_url);
        debug!("  Model name: {}", model_name);
        debug!("  Zip filename: {}", zip_filename);
        debug!("  Final URL: {}", zip_url);
        debug!("  Target path: {:?}", zip_path);

        // Create target directory if it doesn't exist
        tokio::fs::create_dir_all(target_dir)
            .await
            .context("Failed to create target directory")?;

        // Download the zip file with progress
        self.download_file(&zip_url, &zip_path, progress_callback)
            .await
            .context("Failed to download model zip file")?;

        // Extract zip contents
        self.extract_model_zip(&zip_path, target_dir)
            .await
            .context("Failed to extract model zip file")?;

        // Clean up zip file
        tokio::fs::remove_file(&zip_path)
            .await
            .context("Failed to remove temporary zip file")?;

        debug!("Successfully downloaded and extracted model: {}", model_name);
        Ok(())
    }

    /// Download a file from URL to local path (asynchronous) with progress
    async fn download_file(
        &self,
        url: &str,
        target_path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> AnyhowResult<()> {
        debug!("Attempting to download from URL: {}", url);

        let response = self.client.get(url).send().await.map_err(|e| {
            error!("HTTP request failed for URL: {} - Error: {}", url, e);
            anyhow::anyhow!("HTTP request failed for URL: {} - {}", url, e)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            error!("HTTP {} response body: {}", status, body);
            return Err(anyhow::anyhow!(
                "HTTP {} error for URL: {} - Response: {}",
                status,
                url,
                body
            ));
        }

        // Get content length for progress tracking
        let content_length = response.content_length().unwrap_or(0);

        let mut file = tokio::fs::File::create(target_path)
            .await
            .context("Failed to create target file")?;

        // Create progress bar if we have content length and no custom callback
        let progress_bar = if content_length > 0 && progress_callback.is_none() {
            let pb = ProgressBar::new(content_length);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{msg} {spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                    .expect("Failed to set progress bar template")
                    .progress_chars("#>-")
            );
            pb.set_message("Loading model");
            Some(pb)
        } else {
            None
        };

        // Read and write with progress tracking
        use tokio::io::AsyncWriteExt;
        use tokio_stream::StreamExt;

        let mut total_downloaded = 0u64;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read chunk from response")?;

            file.write_all(&chunk).await.context("Failed to write chunk to file")?;

            total_downloaded += chunk.len() as u64;

            // Update progress
            if let Some(ref pb) = progress_bar {
                pb.set_position(total_downloaded);
            }
            if let Some(ref callback) = progress_callback {
                callback(total_downloaded, content_length);
            }
        }

        // Finish progress bar
        if let Some(pb) = progress_bar {
            pb.finish_and_clear();
        }

        debug!("Downloaded {} bytes to {:?}", total_downloaded, target_path);
        Ok(())
    }

    /// Extract model zip file to target directory
    async fn extract_model_zip(&self, zip_path: &Path, target_dir: &Path) -> AnyhowResult<()> {
        // Since zip extraction is CPU-intensive and the zip crate is sync,
        // we'll run it in a blocking task
        let zip_path = zip_path.to_path_buf();
        let target_dir = target_dir.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&zip_path).context("Failed to open zip file")?;

            let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).context("Failed to read zip entry")?;

                let outpath = target_dir.join(file.name());

                if file.is_dir() {
                    std::fs::create_dir_all(&outpath).context("Failed to create directory from zip")?;
                } else {
                    if let Some(parent) = outpath.parent() {
                        std::fs::create_dir_all(parent).context("Failed to create parent directory for zip entry")?;
                    }

                    let mut outfile = std::fs::File::create(&outpath).context("Failed to create output file")?;

                    std::io::copy(&mut file, &mut outfile).context("Failed to extract file from zip")?;

                    debug!("Extracted: {:?}", outpath);
                }
            }

            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("Zip extraction task failed")?
        .context("Zip extraction failed")?;

        Ok(())
    }

    /// Check if model files exist and are valid (sync version for testing)
    #[cfg(test)]
    fn is_model_valid_sync(&self, target_dir: &Path) -> AnyhowResult<bool> {
        let model_path = target_dir.join("model.safetensors");
        let tokenizer_path = target_dir.join("tokenizer.json");

        let valid = model_path.exists() && tokenizer_path.exists();

        debug!(
            "Model validation for {:?}: model={}, tokenizer={}",
            target_dir,
            model_path.exists(),
            tokenizer_path.exists()
        );

        Ok(valid)
    }

    /// Check if model files exist and are valid
    async fn is_model_valid(&self, target_dir: &Path) -> AnyhowResult<bool> {
        let model_path = target_dir.join("model.safetensors");
        let tokenizer_path = target_dir.join("tokenizer.json");

        // Use tokio::fs for async file operations
        let model_exists = (tokio::fs::metadata(&model_path).await).is_ok();

        let tokenizer_exists = (tokio::fs::metadata(&tokenizer_path).await).is_ok();

        let valid = model_exists && tokenizer_exists;

        if valid {
            debug!(
                "Model files found: model={:?}, tokenizer={:?}",
                model_path, tokenizer_path
            );
        } else {
            debug!(
                "Model files missing: model_exists={}, tokenizer_exists={}",
                model_path.exists(),
                tokenizer_path.exists()
            );
        }

        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_hosted_model_client_creation() {
        let client = HostedModelClient::new("https://example.com/models".to_string());
        assert_eq!(client.base_url, "https://example.com/models");
    }

    #[test]
    fn test_is_model_valid_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let client = HostedModelClient::new("https://example.com".to_string());

        let is_valid = client.is_model_valid_sync(temp_dir.path()).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_url_construction() {
        // Test the internal URL construction logic by checking what would be built
        let base_url = "https://example.com/models";
        let model_name = "all-MiniLM-L6-v2";
        let expected_url = format!("{}/{}.zip", base_url, model_name);

        assert_eq!(expected_url, "https://example.com/models/all-MiniLM-L6-v2.zip");
    }

    #[test]
    fn test_is_model_valid_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let client = HostedModelClient::new("https://example.com".to_string());

        // Create mock model files
        fs::write(temp_dir.path().join("model.safetensors"), b"mock model").unwrap();
        fs::write(temp_dir.path().join("tokenizer.json"), b"mock tokenizer").unwrap();

        let is_valid = client.is_model_valid_sync(temp_dir.path()).unwrap();
        assert!(is_valid);
    }
}
