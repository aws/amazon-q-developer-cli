#[cfg(not(target_arch = "aarch64"))]
use crate::embedding::CandleTextEmbedder;
#[cfg(test)]
use crate::embedding::MockTextEmbedder;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::embedding::TextEmbedder;
use crate::embedding::{
    BM25TextEmbedder,
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
        #[cfg(not(target_arch = "aarch64"))]
        EmbeddingType::Candle => Box::new(CandleTextEmbedder::new()?),
        EmbeddingType::Onnx => Box::new(TextEmbedder::new()?),
        EmbeddingType::BM25 => Box::new(BM25TextEmbedder::new()?),
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
        #[cfg(not(target_arch = "aarch64"))]
        EmbeddingType::Candle => Box::new(CandleTextEmbedder::new()?),
        EmbeddingType::BM25 => Box::new(BM25TextEmbedder::new()?),
        #[cfg(test)]
        EmbeddingType::Mock => Box::new(MockTextEmbedder::new(384)),
    };

    Ok(embedder)
}
