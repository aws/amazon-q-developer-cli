#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
use crate::embedding::CandleTextEmbedder;
use crate::embedding::MockTextEmbedder; // Used for Fast type since BM25 doesn't need embeddings
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
use crate::embedding::ModelType;
use crate::embedding::{
    EmbeddingType,
    TextEmbedderTrait,
};
use crate::error::Result;

/// Creates a text embedder based on the specified embedding type
///
/// # Arguments
///
/// * `embedding_type` - Type of embedding engine to use
///
/// # Returns
///
/// A text embedder instance
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn create_embedder(embedding_type: EmbeddingType) -> Result<Box<dyn TextEmbedderTrait>> {
    let embedder: Box<dyn TextEmbedderTrait> = match embedding_type {
        EmbeddingType::Fast => Box::new(MockTextEmbedder::new(384)), // BM25 doesn't use embeddings
        #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
        EmbeddingType::Best => Box::new(CandleTextEmbedder::with_model_type(ModelType::MiniLML6V2)?),
        #[cfg(test)]
        EmbeddingType::Mock => Box::new(MockTextEmbedder::new(384)),
    };

    Ok(embedder)
}

/// Creates a text embedder based on the specified embedding type
/// (Linux version)
///
/// # Arguments
///
/// * `embedding_type` - Type of embedding engine to use
///
/// # Returns
///
/// A text embedder instance
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn create_embedder(embedding_type: EmbeddingType) -> Result<Box<dyn TextEmbedderTrait>> {
    let embedder: Box<dyn TextEmbedderTrait> = match embedding_type {
        EmbeddingType::Fast => Box::new(MockTextEmbedder::new(384)), // BM25 doesn't use embeddings
        #[cfg(not(target_arch = "aarch64"))]
        EmbeddingType::Best => Box::new(CandleTextEmbedder::with_model_type(ModelType::MiniLML6V2)?),
        #[cfg(test)]
        EmbeddingType::Mock => Box::new(MockTextEmbedder::new(384)),
    };

    Ok(embedder)
}
