use crate::error::Result;

/// Embedding engine type to use
#[derive(Debug, Clone, Copy)]
pub enum EmbeddingType {
    /// Use Candle embedding engine (not available on Linux ARM)
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    Candle,
    /// Use BM25 embedding engine (available on all platforms)
    BM25,
    /// Use Mock embedding engine (only available in tests)
    #[cfg(test)]
    Mock,
}

// Default implementation based on platform capabilities
// All platforms except Linux ARM: Use Candle
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
#[allow(clippy::derivable_impls)]
impl Default for EmbeddingType {
    fn default() -> Self {
        EmbeddingType::Candle
    }
}

// Linux ARM: Use BM25
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[allow(clippy::derivable_impls)]
impl Default for EmbeddingType {
    fn default() -> Self {
        EmbeddingType::BM25
    }
}

/// Common trait for text embedders
pub trait TextEmbedderTrait: Send + Sync {
    /// Generate an embedding for a text
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
impl TextEmbedderTrait for super::CandleTextEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch(texts)
    }
}

impl TextEmbedderTrait for super::BM25TextEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch(texts)
    }
}

#[cfg(test)]
impl TextEmbedderTrait for super::MockTextEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch(texts)
    }
}
