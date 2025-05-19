//! Text embedding functionality using fastembed
//!
//! This module provides functionality for generating text embeddings
//! using the fastembed library, which is available on macOS and Windows platforms.

use fastembed::{
    InitOptions,
    TextEmbedding,
};
use tracing::{
    debug,
    error,
    info,
};

use crate::embedding::onnx_models::OnnxModelType;
use crate::error::{
    Result,
    SemanticSearchError,
};

/// Text embedder using fastembed
pub struct TextEmbedder {
    /// The embedding model
    model: TextEmbedding,
    /// The model type
    model_type: OnnxModelType,
}

impl TextEmbedder {
    /// Create a new TextEmbedder with the default model (all-MiniLM-L6-v2-Q)
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn new() -> Result<Self> {
        Self::with_model_type(OnnxModelType::default())
    }

    /// Create a new TextEmbedder with a specific model type
    ///
    /// # Arguments
    ///
    /// * `model_type` - The model type to use
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn with_model_type(model_type: OnnxModelType) -> Result<Self> {
        info!("Initializing text embedder with fastembed model: {:?}", model_type);

        // Prepare the models directory
        let models_dir = prepare_models_directory()?;

        // Initialize the embedding model
        let model = initialize_model(model_type, &models_dir)?;

        debug!(
            "Fastembed text embedder initialized successfully with model: {:?}",
            model_type
        );

        Ok(Self { model, model_type })
    }

    /// Get the model type
    pub fn model_type(&self) -> OnnxModelType {
        self.model_type
    }

    /// Generate an embedding for a text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed
    ///
    /// # Returns
    ///
    /// A vector of floats representing the text embedding
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let texts = vec![text];
        match self.model.embed(texts, None) {
            Ok(embeddings) => Ok(embeddings.into_iter().next().unwrap()),
            Err(e) => {
                error!("Failed to embed text: {}", e);
                Err(SemanticSearchError::FastembedError(e.to_string()))
            },
        }
    }

    /// Generate embeddings for multiple texts
    ///
    /// # Arguments
    ///
    /// * `texts` - The texts to embed
    ///
    /// # Returns
    ///
    /// A vector of embeddings
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let documents: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        match self.model.embed(documents, None) {
            Ok(embeddings) => Ok(embeddings),
            Err(e) => {
                error!("Failed to embed batch of texts: {}", e);
                Err(SemanticSearchError::FastembedError(e.to_string()))
            },
        }
    }
}

/// Prepare the models directory
///
/// # Returns
///
/// The models directory path
fn prepare_models_directory() -> Result<std::path::PathBuf> {
    // Get the models directory from the base directory
    let base_dir = crate::config::get_default_base_dir();
    let models_dir = crate::config::get_models_dir(&base_dir);

    // Ensure the models directory exists
    std::fs::create_dir_all(&models_dir)?;

    Ok(models_dir)
}

/// Initialize the embedding model
///
/// # Arguments
///
/// * `model_type` - The model type to use
/// * `models_dir` - The models directory path
///
/// # Returns
///
/// The initialized embedding model
fn initialize_model(model_type: OnnxModelType, models_dir: &std::path::Path) -> Result<TextEmbedding> {
    match TextEmbedding::try_new(
        InitOptions::new(model_type.get_fastembed_model())
            .with_cache_dir(models_dir.to_path_buf())
            .with_show_download_progress(true),
    ) {
        Ok(model) => Ok(model),
        Err(e) => {
            error!("Failed to initialize fastembed model: {}", e);
            Err(SemanticSearchError::FastembedError(e.to_string()))
        },
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::time::Instant;

    use super::*;

    /// Helper function to check if real embedder tests should be skipped
    fn should_skip_real_embedder_tests() -> bool {
        // Skip if real embedders are not explicitly requested
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
            return true;
        }

        false
    }

    /// Helper function to create test data for performance tests
    fn create_test_data() -> Vec<String> {
        vec![
            "This is a short sentence.".to_string(),
            "Another simple example.".to_string(),
            "The quick brown fox jumps over the lazy dog.".to_string(),
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.".to_string(),
            "Machine learning models can process and analyze text data to extract meaningful information and generate embeddings that represent semantic relationships between words and phrases.".to_string(),
        ]
    }

    #[test]
    fn test_embed_single() {
        if should_skip_real_embedder_tests() {
            return;
        }

        // Use real embedder for testing
        match TextEmbedder::new() {
            Ok(embedder) => {
                let embedding = embedder.embed("This is a test sentence.").unwrap();

                // MiniLM-L6-v2-Q produces 384-dimensional embeddings
                assert_eq!(embedding.len(), embedder.model_type().get_embedding_dim());
            },
            Err(e) => {
                // If model loading fails, skip the test
                println!("Skipping test: Failed to load real embedder: {}", e);
            },
        }
    }

    #[test]
    fn test_embed_batch() {
        if should_skip_real_embedder_tests() {
            return;
        }

        // Use real embedder for testing
        match TextEmbedder::new() {
            Ok(embedder) => {
                let texts = vec![
                    "The cat sits outside".to_string(),
                    "A man is playing guitar".to_string(),
                ];
                let embeddings = embedder.embed_batch(&texts).unwrap();
                let dim = embedder.model_type().get_embedding_dim();

                assert_eq!(embeddings.len(), 2);
                assert_eq!(embeddings[0].len(), dim);
                assert_eq!(embeddings[1].len(), dim);

                // Check that embeddings are different
                let mut different = false;
                for i in 0..dim {
                    if (embeddings[0][i] - embeddings[1][i]).abs() > 1e-5 {
                        different = true;
                        break;
                    }
                }
                assert!(different);
            },
            Err(e) => {
                // If model loading fails, skip the test
                println!("Skipping test: Failed to load real embedder: {}", e);
            },
        }
    }

    /// Performance test for different model types
    /// This test is only run when MEMORY_BANK_USE_REAL_EMBEDDERS is set
    #[test]
    fn test_model_performance() {
        // Skip this test in CI environments where model files might not be available
        if env::var("CI").is_ok() {
            return;
        }

        // Skip if real embedders are not explicitly requested
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            return;
        }

        // Test data
        let texts = create_test_data();

        // Test each model type
        let model_types = [OnnxModelType::MiniLML6V2Q, OnnxModelType::MiniLML12V2Q];

        for model_type in model_types {
            run_performance_test(model_type, &texts);
        }
    }

    /// Run performance test for a specific model type
    fn run_performance_test(model_type: OnnxModelType, texts: &[String]) {
        match TextEmbedder::with_model_type(model_type) {
            Ok(embedder) => {
                println!("Testing performance of {:?}", model_type);

                // Warm-up run
                let _ = embedder.embed_batch(texts);

                // Measure single embedding performance
                let start = Instant::now();
                let single_result = embedder.embed(&texts[0]);
                let single_duration = start.elapsed();

                // Measure batch embedding performance
                let start = Instant::now();
                let batch_result = embedder.embed_batch(texts);
                let batch_duration = start.elapsed();

                // Check results are valid
                assert!(single_result.is_ok());
                assert!(batch_result.is_ok());

                // Get embedding dimensions
                let embedding_dim = single_result.unwrap().len();

                println!(
                    "Model: {:?}, Embedding dim: {}, Single time: {:?}, Batch time: {:?}, Avg per text: {:?}",
                    model_type,
                    embedding_dim,
                    single_duration,
                    batch_duration,
                    batch_duration.div_f32(texts.len() as f32)
                );
            },
            Err(e) => {
                println!("Failed to load model {:?}: {}", model_type, e);
            },
        }
    }

    /// Test loading all models to ensure they work
    #[test]
    fn test_load_all_models() {
        // Skip this test in CI environments where model files might not be available
        if env::var("CI").is_ok() {
            return;
        }

        // Skip if real embedders are not explicitly requested
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            return;
        }

        let model_types = [OnnxModelType::MiniLML6V2Q, OnnxModelType::MiniLML12V2Q];

        for model_type in model_types {
            test_model_loading(model_type);
        }
    }

    /// Test loading a specific model
    fn test_model_loading(model_type: OnnxModelType) {
        match TextEmbedder::with_model_type(model_type) {
            Ok(embedder) => {
                // Test a simple embedding to verify the model works
                let result = embedder.embed("Test sentence for model verification.");
                assert!(result.is_ok(), "Model {:?} failed to generate embedding", model_type);

                // Verify embedding dimensions
                let embedding = result.unwrap();
                let expected_dim = model_type.get_embedding_dim();

                assert_eq!(
                    embedding.len(),
                    expected_dim,
                    "Model {:?} produced embedding with incorrect dimensions",
                    model_type
                );

                println!("Successfully loaded and tested model {:?}", model_type);
            },
            Err(e) => {
                println!("Failed to load model {:?}: {}", model_type, e);
                // Don't fail the test if a model can't be loaded, just report it
            },
        }
    }
}
impl crate::embedding::BenchmarkableEmbedder for TextEmbedder {
    fn model_name(&self) -> String {
        format!("ONNX-{}", self.model_type().get_model_name())
    }

    fn embedding_dim(&self) -> usize {
        self.model_type().get_embedding_dim()
    }

    fn embed_single(&self, text: &str) -> Vec<f32> {
        self.embed(text).unwrap()
    }

    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.embed_batch(texts).unwrap()
    }
}
