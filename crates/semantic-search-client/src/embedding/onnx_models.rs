use std::path::PathBuf;

use fastembed::EmbeddingModel;

/// Type of ONNX model to use for text embedding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnnxModelType {
    /// MiniLM-L6-v2-Q model (384 dimensions, quantized)
    MiniLML6V2Q,
    /// MiniLM-L12-v2-Q model (384 dimensions, quantized)
    MiniLML12V2Q,
}

impl Default for OnnxModelType {
    fn default() -> Self {
        Self::MiniLML6V2Q
    }
}

impl OnnxModelType {
    /// Get the fastembed model for this model type
    pub fn get_fastembed_model(&self) -> EmbeddingModel {
        match self {
            Self::MiniLML6V2Q => EmbeddingModel::AllMiniLML6V2Q,
            Self::MiniLML12V2Q => EmbeddingModel::AllMiniLML12V2Q,
        }
    }

    /// Get the embedding dimension for this model type
    pub fn get_embedding_dim(&self) -> usize {
        match self {
            Self::MiniLML6V2Q => 384,
            Self::MiniLML12V2Q => 384,
        }
    }

    /// Get the model name
    pub fn get_model_name(&self) -> &'static str {
        match self {
            Self::MiniLML6V2Q => "all-MiniLM-L6-v2-Q",
            Self::MiniLML12V2Q => "all-MiniLM-L12-v2-Q",
        }
    }

    /// Get the local paths for model files
    pub fn get_local_paths(&self) -> PathBuf {
        // Get the base directory and model directory
        let base_dir = crate::config::get_default_base_dir();
        crate::config::get_model_dir(&base_dir, self.get_model_name())
    }
}
