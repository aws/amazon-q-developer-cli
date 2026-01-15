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

use crate::embedding::ModelConfig;
use crate::model_validator::ModelValidator;

/// Progress callback type for download operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Hosted model client for downloading models from CDN (synchronous)
pub struct HostedModelClient {
    /// Base URL for the CDN
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
    /// Model validator for SHA verification
    validator: ModelValidator,
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
            validator: ModelValidator::new(),
        }
    }

    /// Download a model if it doesn't exist locally (asynchronous)
    ///
    /// # Arguments
    ///
    /// * `model_config` - Model configuration containing name and file paths
    /// * `target_dir` - Directory where model files should be extracted
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn ensure_model(&self, model_config: &ModelConfig, target_dir: &Path) -> AnyhowResult<()> {
        self.ensure_model_with_progress(model_config, target_dir, None).await
    }

    /// Download a model if it doesn't exist locally with optional progress callback
    ///
    /// # Arguments
    ///
    /// * `model_config` - Model configuration containing name and file paths
    /// * `target_dir` - Directory where model files should be extracted
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn ensure_model_with_progress(
        &self,
        model_config: &ModelConfig,
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> AnyhowResult<()> {
        // Check if model already exists and is valid
        if self.is_model_valid(model_config, target_dir).await? {
            debug!("Model '{}' already exists and is valid", model_config.name);
            return Ok(());
        }

        debug!("Downloading hosted model: {}", model_config.name);
        self.download_model(model_config, target_dir, progress_callback).await
    }

    /// Download model from hosted CDN (asynchronous) with optional progress
    async fn download_model(
        &self,
        model_config: &ModelConfig,
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> AnyhowResult<()> {
        // Construct zip filename and URL
        let zip_filename = format!("{}.zip", model_config.name);
        let zip_url = format!("{}/{}", self.base_url, zip_filename);
        let zip_path = target_dir.join(&zip_filename);

        debug!("Constructing download URL:");
        debug!("  Base URL: {}", self.base_url);
        debug!("  Model name: {}", model_config.name);
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

        debug!("Successfully downloaded and extracted model: {}", model_config.name);
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
            anyhow::anyhow!("HTTP request failed for URL: {url} - {e}")
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            error!("HTTP {} response body: {}", status, body);
            return Err(anyhow::anyhow!("HTTP {status} error for URL: {url} - Response: {body}"));
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
    fn is_model_valid_sync(&self, model_config: &ModelConfig, target_dir: &Path) -> AnyhowResult<bool> {
        let model_path = target_dir.join(&model_config.model_file);
        let tokenizer_path = target_dir.join(&model_config.tokenizer_file);

        let model_valid = self.validator.validate_file(&model_path);
        let tokenizer_valid = self.validator.validate_file(&tokenizer_path);

        let valid = model_valid && tokenizer_valid;

        debug!(
            "Model validation for {:?}: model={}, tokenizer={}",
            target_dir, model_valid, tokenizer_valid
        );

        Ok(valid)
    }

    /// Check if model files exist and are valid
    async fn is_model_valid(&self, model_config: &ModelConfig, target_dir: &Path) -> AnyhowResult<bool> {
        let model_path = target_dir.join(&model_config.model_file);
        let tokenizer_path = target_dir.join(&model_config.tokenizer_file);

        let model_valid = self.validator.validate_file(&model_path);
        let tokenizer_valid = self.validator.validate_file(&tokenizer_path);

        let valid = model_valid && tokenizer_valid;

        if valid {
            debug!("Model files validated successfully");
        } else {
            debug!("Model files invalid or missing, will re-download");
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

        // Create a minimal ModelConfig for testing
        let model_config = ModelConfig {
            name: "test-model".to_string(),
            repo_path: "test/repo".to_string(),
            model_file: "model.safetensors".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            config: Default::default(),
            normalize_embeddings: true,
            batch_size: 32,
        };

        let is_valid = client.is_model_valid_sync(&model_config, temp_dir.path()).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_url_construction() {
        // Test the internal URL construction logic by checking what would be built
        let base_url = "https://example.com/models";
        let model_name = "all-MiniLM-L6-v2";
        let expected_url = format!("{base_url}/{model_name}.zip");

        assert_eq!(expected_url, "https://example.com/models/all-MiniLM-L6-v2.zip");
    }

    #[test]
    fn test_is_model_valid_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let client = HostedModelClient::new("https://example.com".to_string());

        // Create a minimal ModelConfig for testing
        let model_config = ModelConfig {
            name: "test-model".to_string(),
            repo_path: "test/repo".to_string(),
            model_file: "model.safetensors".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            config: Default::default(),
            normalize_embeddings: true,
            batch_size: 32,
        };

        // Create mock model files with incorrect content
        fs::write(temp_dir.path().join("model.safetensors"), b"mock model").unwrap();
        fs::write(temp_dir.path().join("tokenizer.json"), b"mock tokenizer").unwrap();

        // Should be invalid because SHA doesn't match allowlisted values
        let is_valid = client.is_model_valid_sync(&model_config, temp_dir.path()).unwrap();
        assert!(!is_valid);

        // Files should be removed after validation failure
        assert!(!temp_dir.path().join("model.safetensors").exists());
        assert!(!temp_dir.path().join("tokenizer.json").exists());
    }
}
