//! Benchmark utilities for embedding models
//!
//! This module provides standardized utilities for benchmarking embedding models
//! to ensure fair and consistent comparisons between different implementations.

use std::time::{
    Duration,
    Instant,
};

use tracing::info;

/// Standard test data for benchmarking embedding models
pub fn create_standard_test_data() -> Vec<String> {
    vec![
        "This is a short sentence.".to_string(),
        "Another simple example.".to_string(),
        "The quick brown fox jumps over the lazy dog.".to_string(),
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.".to_string(),
        "Machine learning models can process and analyze text data to extract meaningful information and generate embeddings that represent semantic relationships between words and phrases.".to_string(),
    ]
}

/// Benchmark results for embedding operations
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    /// Model name or identifier
    pub model_name: String,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// Time for single embedding
    pub single_time: Duration,
    /// Time for batch embedding
    pub batch_time: Duration,
    /// Number of texts in the batch
    pub batch_size: usize,
}

impl BenchmarkResults {
    /// Create a new benchmark results instance
    pub fn new(
        model_name: String,
        embedding_dim: usize,
        single_time: Duration,
        batch_time: Duration,
        batch_size: usize,
    ) -> Self {
        Self {
            model_name,
            embedding_dim,
            single_time,
            batch_time,
            batch_size,
        }
    }

    /// Get the average time per text in the batch
    pub fn avg_time_per_text(&self) -> Duration {
        if self.batch_size == 0 {
            return Duration::from_secs(0);
        }
        Duration::from_nanos((self.batch_time.as_nanos() / self.batch_size as u128) as u64)
    }

    /// Log the benchmark results
    pub fn log(&self) {
        info!(
            "Model: {}, Embedding dim: {}, Single time: {:?}, Batch time: {:?}, Avg per text: {:?}",
            self.model_name,
            self.embedding_dim,
            self.single_time,
            self.batch_time,
            self.avg_time_per_text()
        );
    }
}

/// Trait for benchmarkable embedding models
pub trait BenchmarkableEmbedder {
    /// Get the model name
    fn model_name(&self) -> String;

    /// Get the embedding dimension
    fn embedding_dim(&self) -> usize;

    /// Embed a single text
    fn embed_single(&self, text: &str) -> Vec<f32>;

    /// Embed a batch of texts
    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>>;
}

/// Run a standardized benchmark on an embedder
///
/// # Arguments
///
/// * `embedder` - The embedder to benchmark
/// * `texts` - The texts to use for benchmarking
///
/// # Returns
///
/// The benchmark results
pub fn run_standard_benchmark<E: BenchmarkableEmbedder>(embedder: &E) -> BenchmarkResults {
    let texts = create_standard_test_data();

    // Warm-up run
    let _ = embedder.embed_batch(&texts);

    // Measure single embedding performance
    let start = Instant::now();
    let single_result = embedder.embed_single(&texts[0]);
    let single_duration = start.elapsed();

    // Measure batch embedding performance
    let start = Instant::now();
    let batch_result = embedder.embed_batch(&texts);
    let batch_duration = start.elapsed();

    // Verify results
    assert_eq!(single_result.len(), embedder.embedding_dim());
    assert_eq!(batch_result.len(), texts.len());
    assert_eq!(batch_result[0].len(), embedder.embedding_dim());

    BenchmarkResults::new(
        embedder.model_name(),
        embedder.embedding_dim(),
        single_duration,
        batch_duration,
        texts.len(),
    )
}
