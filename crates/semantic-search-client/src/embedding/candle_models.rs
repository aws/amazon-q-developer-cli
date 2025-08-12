use std::path::PathBuf;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
use candle_transformers::models::bert::Config as BertConfig;

/// Type of model to use for text embedding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    /// MiniLM-L6-v2 model (384 dimensions)
    MiniLML6V2,
    /// MiniLM-L12-v2 model (384 dimensions)
    MiniLML12V2,
}

impl Default for ModelType {
    fn default() -> Self {
        Self::MiniLML6V2
    }
}

/// Configuration for a model
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Name of the model
    pub name: String,
    /// Path to the model repository
    pub repo_path: String,
    /// Name of the model file
    pub model_file: String,
    /// Name of the tokenizer file
    pub tokenizer_file: String,
    /// BERT configuration
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    pub config: BertConfig,
    /// Whether to normalize embeddings
    pub normalize_embeddings: bool,
    /// Batch size for processing
    pub batch_size: usize,
}

impl ModelType {
    /// Get the configuration for this model type
    pub fn get_config(&self) -> ModelConfig {
        match self {
            Self::MiniLML6V2 => ModelConfig {
                name: "all-MiniLM-L6-v2".to_string(),
                repo_path: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
                model_file: "model.safetensors".to_string(),
                tokenizer_file: "tokenizer.json".to_string(),
                #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
                config: BertConfig {
                    vocab_size: 30522,
                    hidden_size: 384,
                    num_hidden_layers: 6,
                    num_attention_heads: 12,
                    intermediate_size: 1536,
                    hidden_act: candle_transformers::models::bert::HiddenAct::Gelu,
                    hidden_dropout_prob: 0.0,
                    max_position_embeddings: 512,
                    type_vocab_size: 2,
                    initializer_range: 0.02,
                    layer_norm_eps: 1e-12,
                    pad_token_id: 0,
                    position_embedding_type: candle_transformers::models::bert::PositionEmbeddingType::Absolute,
                    use_cache: true,
                    classifier_dropout: None,
                    model_type: Some("bert".to_string()),
                },
                normalize_embeddings: true,
                batch_size: 32,
            },
            Self::MiniLML12V2 => ModelConfig {
                name: "all-MiniLM-L12-v2".to_string(),
                repo_path: "sentence-transformers/all-MiniLM-L12-v2".to_string(),
                model_file: "model.safetensors".to_string(),
                tokenizer_file: "tokenizer.json".to_string(),
                #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
                config: BertConfig {
                    vocab_size: 30522,
                    hidden_size: 384,
                    num_hidden_layers: 12,
                    num_attention_heads: 12,
                    intermediate_size: 1536,
                    hidden_act: candle_transformers::models::bert::HiddenAct::Gelu,
                    hidden_dropout_prob: 0.0,
                    max_position_embeddings: 512,
                    type_vocab_size: 2,
                    initializer_range: 0.02,
                    layer_norm_eps: 1e-12,
                    pad_token_id: 0,
                    position_embedding_type: candle_transformers::models::bert::PositionEmbeddingType::Absolute,
                    use_cache: true,
                    classifier_dropout: None,
                    model_type: Some("bert".to_string()),
                },
                normalize_embeddings: true,
                batch_size: 32,
            },
        }
    }

    /// Get the local paths for model files
    pub fn get_local_paths(&self) -> (PathBuf, PathBuf) {
        // Get the base directory and models directory
        let base_dir = crate::config::get_default_base_dir();
        let model_dir = crate::config::get_model_dir(&base_dir, &self.get_config().name);

        // Return paths for model and tokenizer files
        (
            model_dir.join(&self.get_config().model_file),
            model_dir.join(&self.get_config().tokenizer_file),
        )
    }
}

impl ModelConfig {
    /// Get the local paths for model files
    pub fn get_local_paths(&self) -> (PathBuf, PathBuf) {
        // Get the base directory and model directory
        let base_dir = crate::config::get_default_base_dir();
        let model_dir = crate::config::get_model_dir(&base_dir, &self.name);

        // Return paths for model and tokenizer files
        (model_dir.join(&self.model_file), model_dir.join(&self.tokenizer_file))
    }
}
