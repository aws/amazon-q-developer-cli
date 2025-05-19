use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use tracing::{
    debug,
    info,
};

use crate::embedding::benchmark_utils::BenchmarkableEmbedder;
use crate::error::Result;

/// TF (Term Frequency) Text Embedder implementation
///
/// This is a simplified fallback implementation for platforms where neither Candle nor ONNX
/// are fully supported. It uses a hash-based approach to create term frequency vectors
/// that can be used for text search.
///
/// Note: This is a keyword-based approach and doesn't support true semantic search.
/// It works by matching keywords rather than understanding semantic meaning, so
/// it will only find matches when there's lexical overlap between query and documents.
pub struct TFTextEmbedder {
    /// Vector dimension
    dimension: usize,
}

impl TFTextEmbedder {
    /// Create a new TF text embedder
    pub fn new() -> Result<Self> {
        info!("Initializing TF Text Embedder");

        let embedder = Self {
            dimension: 384, // Match dimension of other embedders for compatibility
        };

        debug!("TF Text Embedder initialized successfully");
        Ok(embedder)
    }

    /// Tokenize text into terms
    fn tokenize(text: &str) -> Vec<String> {
        // Simple tokenization by splitting on whitespace and punctuation
        text.to_lowercase()
            .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }
    
    /// Hash a string to an index within the dimension range
    fn hash_to_index(token: &str, dimension: usize) -> usize {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        (hasher.finish() as usize) % dimension
    }

    /// Create a term frequency vector from tokens
    fn create_term_frequency_vector(&self, tokens: &[String]) -> Vec<f32> {
        let mut vector = vec![0.0; self.dimension];
        
        // Count term frequencies using hash-based indexing
        for token in tokens {
            let idx = Self::hash_to_index(token, self.dimension);
            vector[idx] += 1.0;
        }
        
        // Normalize the vector
        let norm: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in vector.iter_mut() {
                *val /= norm;
            }
        }
        
        vector
    }

    /// Embed a text using simplified hash-based approach
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = Self::tokenize(text);
        let vector = self.create_term_frequency_vector(&tokens);
        Ok(vector)
    }

    /// Embed multiple texts
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            results.push(self.embed(text)?);
        }

        Ok(results)
    }
}

// Implement BenchmarkableEmbedder for TFTextEmbedder
impl BenchmarkableEmbedder for TFTextEmbedder {
    fn model_name(&self) -> String {
        "TF".to_string()
    }

    fn embedding_dim(&self) -> usize {
        self.dimension
    }

    fn embed_single(&self, text: &str) -> Vec<f32> {
        self.embed(text).unwrap_or_else(|_| vec![0.0; self.dimension])
    }

    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.embed_batch(texts)
            .unwrap_or_else(|_| vec![vec![0.0; self.dimension]; texts.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tf_embed_single() {
        let embedder = TFTextEmbedder::new().unwrap();
        let text = "This is a test sentence";
        let embedding = embedder.embed(text).unwrap();

        // Check that the embedding has the expected dimension
        assert_eq!(embedding.len(), embedder.dimension);

        // Check that the embedding is normalized
        let norm: f32 = embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5 || norm == 0.0);
    }

    #[test]
    fn test_tf_embed_batch() {
        let embedder = TFTextEmbedder::new().unwrap();
        let texts = vec![
            "First test sentence".to_string(),
            "Second test sentence".to_string(),
            "Third test sentence".to_string(),
        ];
        let embeddings = embedder.embed_batch(&texts).unwrap();

        // Check that we got the right number of embeddings
        assert_eq!(embeddings.len(), texts.len());

        // Check that each embedding has the expected dimension
        for embedding in &embeddings {
            assert_eq!(embedding.len(), embedder.dimension);
        }
    }

    #[test]
    fn test_tf_tokenization() {
        // Test basic tokenization
        let tokens = TFTextEmbedder::tokenize("Hello, world! This is a test.");
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "a", "test"]);

        // Test case insensitivity
        let tokens = TFTextEmbedder::tokenize("HELLO world");
        assert_eq!(tokens, vec!["hello", "world"]);

        // Test handling of multiple spaces and punctuation
        let tokens = TFTextEmbedder::tokenize("  multiple   spaces, and! punctuation...");
        assert_eq!(tokens, vec!["multiple", "spaces", "and", "punctuation"]);
    }
}
