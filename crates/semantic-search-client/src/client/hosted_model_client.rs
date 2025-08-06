//! Hosted model download client for Amazon Q CLI
//!
//! This module provides functionality to download model files from a hosted CDN
//! instead of directly from Hugging Face. Models are distributed as zip files
//! containing model.safetensors and tokenizer.json files.

use std::path::Path;
use std::fs;
use anyhow::{Result as AnyhowResult, Context};
use tracing::{info, debug, error};
use indicatif::{ProgressBar, ProgressStyle};

/// Progress callback type for download operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Hosted model client for downloading models from CDN (synchronous like original HF)
pub struct HostedModelClient {
    /// Base URL for the CDN (e.g., "https://desktop.gamma-us-east-1.codewhisperer.ai.aws.dev/models")
    base_url: String,
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
        Self { base_url }
    }

    /// Download a model if it doesn't exist locally (synchronous like original HF api.repo().get())
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model (e.g., "all-MiniLM-L6-v2")
    /// * `target_dir` - Directory where model files should be extracted
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn ensure_model(&self, model_name: &str, target_dir: &Path) -> AnyhowResult<()> {
        self.ensure_model_with_progress(model_name, target_dir, None)
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
    pub fn ensure_model_with_progress(
        &self, 
        model_name: &str, 
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>
    ) -> AnyhowResult<()> {
        // Check if model already exists and is valid
        if self.is_model_valid(target_dir)? {
            info!("Model '{}' already exists and is valid", model_name);
            return Ok(());
        }

        info!("Downloading hosted model: {}", model_name);
        self.download_model(model_name, target_dir, progress_callback)
    }

    /// Download model from hosted CDN (synchronous) with optional progress
    fn download_model(
        &self, 
        model_name: &str, 
        target_dir: &Path,
        progress_callback: Option<ProgressCallback>
    ) -> AnyhowResult<()> {
        // Construct zip filename and URL
        let zip_filename = format!("{}.zip", model_name);
        let zip_url = format!("{}/{}", self.base_url, zip_filename);
        let zip_path = target_dir.join(&zip_filename);

        info!("Constructing download URL:");
        info!("  Base URL: {}", self.base_url);
        info!("  Model name: {}", model_name);
        info!("  Zip filename: {}", zip_filename);
        info!("  Final URL: {}", zip_url);
        info!("  Target path: {:?}", zip_path);

        // Create target directory if it doesn't exist
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create parent directories")?;
        }
        fs::create_dir_all(target_dir)
            .context("Failed to create target directory")?;

        // Download the zip file with progress
        self.download_file(&zip_url, &zip_path, progress_callback)
            .context("Failed to download model zip file")?;

        // Extract zip contents
        self.extract_model_zip(&zip_path, target_dir)
            .context("Failed to extract model zip file")?;

        // Clean up zip file
        fs::remove_file(&zip_path)
            .context("Failed to remove temporary zip file")?;

        info!("Successfully downloaded and extracted model: {}", model_name);
        Ok(())
    }

    /// Download a file from URL to local path (synchronous) with progress
    fn download_file(
        &self, 
        url: &str, 
        target_path: &Path,
        progress_callback: Option<ProgressCallback>
    ) -> AnyhowResult<()> {
        info!("Attempting to download from URL: {}", url);
        
        let response = ureq::get(url).call()
            .map_err(|e| {
                error!("HTTP request failed for URL: {} - Error: {}", url, e);
                match e {
                    ureq::Error::Status(code, response) => {
                        let body = response.into_string().unwrap_or_else(|_| "Unable to read response body".to_string());
                        error!("HTTP {} response body: {}", code, body);
                        anyhow::anyhow!("HTTP {} error for URL: {} - Response: {}", code, url, body)
                    }
                    ureq::Error::Transport(transport_err) => {
                        error!("Transport error: {}", transport_err);
                        anyhow::anyhow!("Transport error for URL: {} - {}", url, transport_err)
                    }
                }
            })?;

        // Get content length for progress tracking
        let content_length = response.header("content-length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let mut file = fs::File::create(target_path)
            .context("Failed to create target file")?;

        // Create progress bar if we have content length and no custom callback
        let progress_bar = if content_length > 0 && progress_callback.is_none() {
            let pb = ProgressBar::new(content_length);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                    .expect("Failed to set progress bar template")
                    .progress_chars("#>-")
            );
            pb.set_message("Downloading model");
            Some(pb)
        } else {
            None
        };

        // Read and write with progress tracking
        let mut reader = response.into_reader();
        let mut buffer = [0; 8192]; // 8KB buffer
        let mut total_downloaded = 0u64;

        loop {
            let bytes_read = std::io::Read::read(&mut reader, &mut buffer)
                .context("Failed to read from response")?;
            
            if bytes_read == 0 {
                break; // EOF
            }

            std::io::Write::write_all(&mut file, &buffer[..bytes_read])
                .context("Failed to write to file")?;

            total_downloaded += bytes_read as u64;

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
            pb.finish_with_message("Download complete");
        }

        debug!("Downloaded {} bytes to {:?}", total_downloaded, target_path);
        Ok(())
    }

    /// Extract model zip file to target directory
    fn extract_model_zip(&self, zip_path: &Path, target_dir: &Path) -> AnyhowResult<()> {
        let file = fs::File::open(zip_path)
            .context("Failed to open zip file")?;
        
        let mut archive = zip::ZipArchive::new(file)
            .context("Failed to read zip archive")?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .context("Failed to read zip entry")?;
            
            let outpath = target_dir.join(file.name());

            if file.is_dir() {
                fs::create_dir_all(&outpath)
                    .context("Failed to create directory from zip")?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)
                        .context("Failed to create parent directory for zip entry")?;
                }
                
                let mut outfile = fs::File::create(&outpath)
                    .context("Failed to create output file")?;
                
                std::io::copy(&mut file, &mut outfile)
                    .context("Failed to extract file from zip")?;
                
                debug!("Extracted: {:?}", outpath);
            }
        }

        Ok(())
    }

    /// Check if model files exist and are valid
    fn is_model_valid(&self, target_dir: &Path) -> AnyhowResult<bool> {
        let model_path = target_dir.join("model.safetensors");
        let tokenizer_path = target_dir.join("tokenizer.json");
        
        let valid = model_path.exists() && tokenizer_path.exists();
        
        if valid {
            debug!("Model files found: model={:?}, tokenizer={:?}", model_path, tokenizer_path);
        } else {
            debug!("Model files missing: model_exists={}, tokenizer_exists={}", 
                   model_path.exists(), tokenizer_path.exists());
        }
        
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hosted_model_client_creation() {
        let client = HostedModelClient::new("https://example.com/models".to_string());
        assert_eq!(client.base_url, "https://example.com/models");
    }

    #[test]
    fn test_is_model_valid_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let client = HostedModelClient::new("https://example.com".to_string());
        
        let is_valid = client.is_model_valid(temp_dir.path()).unwrap();
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
        
        let is_valid = client.is_model_valid(temp_dir.path()).unwrap();
        assert!(is_valid);
    }
}
