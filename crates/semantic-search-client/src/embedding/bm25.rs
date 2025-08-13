use std::sync::Arc;

use bm25::{
    Embedder,
    EmbedderBuilder,
    Embedding,
};
use tracing::{
    debug,
    info,
};

use crate::embedding::benchmark_utils::BenchmarkableEmbedder;
use crate::error::Result;

/// BM25 Text Embedder implementation
///
/// This is a fallback implementation for platforms where neither Candle nor ONNX
/// are fully supported. It uses the BM25 algorithm to create term frequency vectors
/// that can be used for text search.
///
/// Note: BM25 is a keyword-based approach and doesn't support true semantic search.
/// It works by matching keywords rather than understanding semantic meaning, so
/// it will only find matches when there's lexical overlap between query and documents.
pub struct BM25TextEmbedder {
    /// BM25 embedder from the bm25 crate
    embedder: Arc<Embedder>,
    /// Vector dimension (fixed size for compatibility with other embedders)
    dimension: usize,
}

impl BM25TextEmbedder {
    /// Create a new BM25 text embedder
    pub fn new() -> Result<Self> {
        info!("Initializing BM25TextEmbedder with language detection");

        // Initialize with a small sample corpus to build the embedder
        // We can use an empty corpus and rely on the fallback avgdl
        // Using LanguageMode::Detect for automatic language detection
        let embedder = EmbedderBuilder::with_fit_to_corpus(bm25::LanguageMode::Detect, &[]).build();

        debug!(
            "BM25TextEmbedder initialized successfully with avgdl: {}",
            embedder.avgdl()
        );

        Ok(Self {
            embedder: Arc::new(embedder),
            dimension: 384, // Match dimension of other embedders for compatibility
        })
    }

    /// Convert a BM25 sparse embedding to a dense vector of fixed dimension
    fn sparse_to_dense(&self, embedding: Embedding) -> Vec<f32> {
        // Create a zero vector of the target dimension
        let mut dense = vec![0.0; self.dimension];

        // Fill in values from the sparse embedding
        for token in embedding.0 {
            // Use the token index modulo dimension to map to a position in our dense vector
            let idx = (token.index as usize) % self.dimension;
            dense[idx] += token.value;
        }

        // Normalize the vector
        let norm: f32 = dense.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in dense.iter_mut() {
                *val /= norm;
            }
        }

        dense
    }

    /// Embed a text using BM25 algorithm
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Generate BM25 embedding
        let embedding = self.embedder.embed(text);

        // Convert to dense vector
        let dense = self.sparse_to_dense(embedding);

        Ok(dense)
    }

    /// Embed multiple texts using BM25 algorithm
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            results.push(self.embed(text)?);
        }

        Ok(results)
    }
}

// Implement BenchmarkableEmbedder for BM25TextEmbedder
impl BenchmarkableEmbedder for BM25TextEmbedder {
    fn model_name(&self) -> String {
        "BM25".to_string()
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
    fn test_bm25_embed_single() {
        let embedder = BM25TextEmbedder::new().unwrap();
        let text = "This is a test sentence";
        let embedding = embedder.embed(text).unwrap();

        // Check that the embedding has the expected dimension
        assert_eq!(embedding.len(), embedder.dimension);

        // Check that the embedding is normalized
        let norm: f32 = embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5 || norm == 0.0);
    }

    #[test]
    fn test_bm25_embed_batch() {
        let embedder = BM25TextEmbedder::new().unwrap();
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
    fn test_bm25_keyword_matching() {
        let embedder = BM25TextEmbedder::new().unwrap();

        // Create embeddings for two texts
        let text1 = "information retrieval and search engines";
        let text2 = "machine learning algorithms";

        let embedding1 = embedder.embed(text1).unwrap();
        let embedding2 = embedder.embed(text2).unwrap();

        // Create a query embedding
        let query = "information search";
        let query_embedding = embedder.embed(query).unwrap();

        // Calculate cosine similarity
        fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
            let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            dot_product
        }

        let sim1 = cosine_similarity(&query_embedding, &embedding1);
        let sim2 = cosine_similarity(&query_embedding, &embedding2);

        // The query should be more similar to text1 than text2
        assert!(sim1 > sim2);
    }

    #[test]
    fn test_bm25_multilingual() {
        let embedder = BM25TextEmbedder::new().unwrap();

        // Test with different languages
        let english = "The quick brown fox jumps over the lazy dog";
        let spanish = "El zorro marrón rápido salta sobre el perro perezoso";
        let french = "Le rapide renard brun saute par-dessus le chien paresseux";

        // All should produce valid embeddings
        let english_embedding = embedder.embed(english).unwrap();
        let spanish_embedding = embedder.embed(spanish).unwrap();
        let french_embedding = embedder.embed(french).unwrap();

        // Check dimensions
        assert_eq!(english_embedding.len(), embedder.dimension);
        assert_eq!(spanish_embedding.len(), embedder.dimension);
        assert_eq!(french_embedding.len(), embedder.dimension);

        // Check normalization
        let norm_en: f32 = english_embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        let norm_es: f32 = spanish_embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        let norm_fr: f32 = french_embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();

        assert!((norm_en - 1.0).abs() < 1e-5 || norm_en == 0.0);
        assert!((norm_es - 1.0).abs() < 1e-5 || norm_es == 0.0);
        assert!((norm_fr - 1.0).abs() < 1e-5 || norm_fr == 0.0);
    }
}
