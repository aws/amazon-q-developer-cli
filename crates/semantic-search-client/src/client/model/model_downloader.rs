use tracing::debug;

use crate::embedding::EmbeddingType;
use crate::error::{
    Result,
    SemanticSearchError,
};

/// Model downloader utility
pub struct ModelDownloader;

impl ModelDownloader {
    /// Ensure models are downloaded
    pub async fn ensure_models_downloaded(embedding_type: &EmbeddingType) -> Result<()> {
        match embedding_type {
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            EmbeddingType::Best => {
                Self::download_best_model().await?;
            },
            EmbeddingType::Fast => {
                // BM25 doesn't require model downloads
            },
            #[cfg(test)]
            EmbeddingType::Mock => {
                // Mock doesn't require model downloads
            },
        }
        Ok(())
    }

    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    async fn download_best_model() -> Result<()> {
        use crate::client::hosted_model_client::HostedModelClient;
        use crate::embedding::ModelType;

        let model_config = ModelType::default().get_config();
        let (model_path, _tokenizer_path) = model_config.get_local_paths();

        // Create model directory if it doesn't exist
        if let Some(parent) = model_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SemanticSearchError::IoError)?;
        }

        debug!("Reviewing model files for {}...", model_config.name);

        // Get the target directory (parent of model_path, which should be the model directory)
        let target_dir = model_path
            .parent()
            .ok_or_else(|| SemanticSearchError::EmbeddingError("Invalid model path".to_string()))?;

        // Get the hosted models base URL from config
        let semantic_config = crate::config::get_config();
        let base_url = &semantic_config.hosted_models_base_url;

        // Create hosted model client and download with progress bar
        let client = HostedModelClient::new(base_url.clone());
        client
            .ensure_model(&model_config, target_dir)
            .await
            .map_err(|e| SemanticSearchError::EmbeddingError(format!("Failed to download model: {e}")))?;

        debug!("Model download completed for {}", model_config.name);
        Ok(())
    }
}
