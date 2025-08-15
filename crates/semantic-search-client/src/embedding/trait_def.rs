use serde::{
    Deserialize,
    Serialize,
};

use crate::error::Result;

/// Embedding engine type to use
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmbeddingType {
    /// Fast embedding using BM25 (available on all platforms)
    Fast,
    /// Best embedding using all-MiniLM-L6-v2 (not available on Linux ARM)
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    Best,
    /// Use Mock embedding engine (only available in tests)
    #[cfg(test)]
    Mock,
}

// Default implementation based on platform capabilities
// All platforms except Linux ARM: Use Best (all-MiniLM-L6-v2)
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
#[allow(clippy::derivable_impls)]
impl Default for EmbeddingType {
    fn default() -> Self {
        EmbeddingType::Best
    }
}

// Linux ARM: Use Fast (BM25)
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[allow(clippy::derivable_impls)]
impl Default for EmbeddingType {
    fn default() -> Self {
        EmbeddingType::Fast
    }
}

impl EmbeddingType {
    /// Convert to the internal model type for Candle embeddings
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    pub fn to_model_type(&self) -> Option<super::ModelType> {
        match self {
            Self::Fast => None, // BM25 doesn't use Candle models
            Self::Best => Some(super::ModelType::MiniLML6V2),
            #[cfg(test)]
            Self::Mock => None,
        }
    }

    /// Check if this embedding type uses BM25
    pub fn is_bm25(&self) -> bool {
        matches!(self, Self::Fast)
    }

    /// Check if this embedding type uses Candle
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    pub fn is_candle(&self) -> bool {
        matches!(self, Self::Best)
    }

    /// Get a human-readable description of the embedding type
    pub fn description(&self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            Self::Best => "Best",
            #[cfg(test)]
            Self::Mock => "Mock",
        }
    }

    /// Convert from string representation
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "fast" => Some(Self::Fast),
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            "best" => Some(Self::Best),
            #[cfg(test)]
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn to_string(&self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            Self::Best => "Best",
            #[cfg(test)]
            Self::Mock => "Mock",
        }
    }
}
/// Trait for text embedding implementations
///
/// This trait defines the interface for converting text into vector embeddings
/// for semantic search operations.
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

impl TextEmbedderTrait for super::MockTextEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch(texts)
    }
}
