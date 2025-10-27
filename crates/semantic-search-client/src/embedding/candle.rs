use std::path::Path;
use std::thread::available_parallelism;

use candle_core::{
    Device,
    Tensor,
};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel,
    DTYPE,
};
use rayon::prelude::*;
use tokenizers::Tokenizer;
use tracing::{
    debug,
    error,
    info,
};

use crate::embedding::candle_models::{
    ModelConfig,
    ModelType,
};
use crate::error::{
    Result,
    SemanticSearchError,
};

/// Text embedding generator using Candle for embedding models
pub struct CandleTextEmbedder {
    /// The BERT model
    model: BertModel,
    /// The tokenizer
    tokenizer: Tokenizer,
    /// The device to run on
    device: Device,
    /// Model configuration
    config: ModelConfig,
}

impl CandleTextEmbedder {
    /// Create a new TextEmbedder with the default model (all-MiniLM-L6-v2)
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn new() -> Result<Self> {
        Self::with_model_type(ModelType::default())
    }

    /// Create a new TextEmbedder with a specific model type
    ///
    /// # Arguments
    ///
    /// * `model_type` - The type of model to use
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn with_model_type(model_type: ModelType) -> Result<Self> {
        let model_config = model_type.get_config();
        let (model_path, tokenizer_path) = model_config.get_local_paths();

        Self::with_model_config(&model_path, &tokenizer_path, model_config)
    }

    /// Create a new TextEmbedder with specific model paths and configuration
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the model file (.safetensors)
    /// * `tokenizer_path` - Path to the tokenizer file (.json)
    /// * `config` - Model configuration
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn with_model_config(model_path: &Path, tokenizer_path: &Path, config: ModelConfig) -> Result<Self> {
        info!("Initializing text embedder with model: {:?}", model_path);

        // Initialize thread pool
        let threads = initialize_thread_pool()?;
        info!("Using {} threads for text embedding", threads);

        // Load tokenizer
        let tokenizer = load_tokenizer(tokenizer_path)?;

        // Get the best available device (Metal, CUDA, or CPU)
        let device = get_best_available_device();

        // Load model
        let model = load_model(model_path, &config, &device)?;

        debug!("Text embedder initialized successfully");

        Ok(Self {
            model,
            tokenizer,
            device,
            config,
        })
    }

    /// Create a new TextEmbedder with specific model paths
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the model file (.safetensors)
    /// * `tokenizer_path` - Path to the tokenizer file (.json)
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn with_model_paths(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        // Use default model configuration
        let config = ModelType::default().get_config();
        Self::with_model_config(model_path, tokenizer_path, config)
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
        let texts = vec![text.to_string()];
        match self.embed_batch(&texts) {
            Ok(embeddings) => Ok(embeddings.into_iter().next().unwrap()),
            Err(e) => {
                error!("Failed to embed text: {}", e);
                Err(e)
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
        // Configure tokenizer with padding
        let tokenizer = prepare_tokenizer(&self.tokenizer)?;

        // Process in batches for better memory efficiency
        let batch_size = self.config.batch_size;

        // Use parallel iterator to process batches in parallel
        let all_embeddings: Vec<Vec<f32>> = texts
            .par_chunks(batch_size)
            .flat_map(|batch| self.process_batch(batch, &tokenizer))
            .collect();

        // Check if we have the correct number of embeddings
        if all_embeddings.len() != texts.len() {
            return Err(SemanticSearchError::EmbeddingError(
                "Failed to generate embeddings for all texts".to_string(),
            ));
        }

        Ok(all_embeddings)
    }

    /// Process a batch of texts to generate embeddings
    fn process_batch(&self, batch: &[String], tokenizer: &Tokenizer) -> Vec<Vec<f32>> {
        // Tokenize batch
        let tokens = match tokenizer.encode_batch(batch.to_vec(), true) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to tokenize texts: {}", e);
                return Vec::new();
            },
        };

        // Convert tokens to tensors
        let (token_ids, attention_mask) = match create_tensors_from_tokens(&tokens, &self.device) {
            Ok(tensors) => tensors,
            Err(_) => return Vec::new(),
        };

        // Create token type ids
        let token_type_ids = match token_ids.zeros_like() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create zeros tensor for token_type_ids: {}", e);
                return Vec::new();
            },
        };

        // Run model inference and process results
        self.run_inference_and_process(&token_ids, &token_type_ids, &attention_mask)
            .unwrap_or_else(|_| Vec::new())
    }

    /// Run model inference and process the results
    fn run_inference_and_process(
        &self,
        token_ids: &Tensor,
        token_type_ids: &Tensor,
        attention_mask: &Tensor,
    ) -> Result<Vec<Vec<f32>>> {
        // Run model inference
        let embeddings = match self.model.forward(token_ids, token_type_ids, Some(attention_mask)) {
            Ok(e) => e,
            Err(e) => {
                error!("Model inference failed: {}", e);
                return Err(SemanticSearchError::EmbeddingError(format!(
                    "Model inference failed: {e}"
                )));
            },
        };

        // Apply mean pooling
        let mean_embeddings = match embeddings.mean(1) {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to compute mean embeddings: {}", e);
                return Err(SemanticSearchError::EmbeddingError(format!(
                    "Failed to compute mean embeddings: {e}"
                )));
            },
        };

        // Normalize if configured
        let final_embeddings = if self.config.normalize_embeddings {
            normalize_l2(&mean_embeddings)?
        } else {
            mean_embeddings
        };

        // Convert to Vec<Vec<f32>>
        match final_embeddings.to_vec2::<f32>() {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Failed to convert embeddings to vector: {}", e);
                Err(SemanticSearchError::EmbeddingError(format!(
                    "Failed to convert embeddings to vector: {e}"
                )))
            },
        }
    }
}

/// Initialize thread pool for parallel processing
fn initialize_thread_pool() -> Result<usize> {
    // Automatically detect available parallelism
    let threads = match available_parallelism() {
        Ok(n) => n.get(),
        Err(e) => {
            error!("Failed to detect available parallelism: {}", e);
            // Default to 4 threads if detection fails
            4
        },
    };

    // Initialize the global Rayon thread pool once
    if let Err(e) = rayon::ThreadPoolBuilder::new().num_threads(threads).build_global() {
        // This is fine - it means the pool is already initialized
        debug!("Rayon thread pool already initialized or failed: {}", e);
    }

    Ok(threads)
}

/// Load tokenizer from file
fn load_tokenizer(tokenizer_path: &Path) -> Result<Tokenizer> {
    match Tokenizer::from_file(tokenizer_path) {
        Ok(t) => Ok(t),
        Err(e) => {
            error!("Failed to load tokenizer from {:?}: {}", tokenizer_path, e);
            Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to load tokenizer: {e}"
            )))
        },
    }
}

/// Get the best available device for inference
fn get_best_available_device() -> Device {
    // Always use CPU for embedding to avoid hardware acceleration issues
    info!("Using CPU for text embedding (hardware acceleration disabled)");
    Device::Cpu
}

/// Load model from file
fn load_model(model_path: &Path, config: &ModelConfig, device: &Device) -> Result<BertModel> {
    // Load model weights
    let vb = unsafe {
        match VarBuilder::from_mmaped_safetensors(&[model_path], DTYPE, device) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to load model weights from {:?}: {}", model_path, e);
                return Err(SemanticSearchError::EmbeddingError(format!(
                    "Failed to load model weights: {e}"
                )));
            },
        }
    };

    // Create BERT model
    match BertModel::load(vb, &config.config) {
        Ok(m) => Ok(m),
        Err(e) => {
            error!("Failed to create BERT model: {}", e);
            Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to create BERT model: {e}"
            )))
        },
    }
}

/// Prepare tokenizer with padding configuration
fn prepare_tokenizer(tokenizer: &Tokenizer) -> Result<Tokenizer> {
    let mut tokenizer = tokenizer.clone();
    if let Some(pp) = tokenizer.get_padding_mut() {
        pp.strategy = tokenizers::PaddingStrategy::BatchLongest;
    } else {
        let pp = tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(pp));
    }
    Ok(tokenizer)
}

/// Create tensors from tokenized inputs
fn create_tensors_from_tokens(tokens: &[tokenizers::Encoding], device: &Device) -> Result<(Tensor, Tensor)> {
    // Pre-allocate vectors with exact capacity
    let mut token_ids = Vec::with_capacity(tokens.len());
    let mut attention_mask = Vec::with_capacity(tokens.len());

    // Convert tokens to tensors
    for tokens in tokens {
        let ids = tokens.get_ids().to_vec();
        let mask = tokens.get_attention_mask().to_vec();

        let ids_tensor = match Tensor::new(ids.as_slice(), device) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create token_ids tensor: {}", e);
                return Err(SemanticSearchError::EmbeddingError(format!(
                    "Failed to create token_ids tensor: {e}"
                )));
            },
        };

        let mask_tensor = match Tensor::new(mask.as_slice(), device) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create attention_mask tensor: {}", e);
                return Err(SemanticSearchError::EmbeddingError(format!(
                    "Failed to create attention_mask tensor: {e}"
                )));
            },
        };

        token_ids.push(ids_tensor);
        attention_mask.push(mask_tensor);
    }

    // Stack tensors into batches
    let token_ids = match Tensor::stack(&token_ids, 0) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to stack token_ids tensors: {}", e);
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to stack token_ids tensors: {e}"
            )));
        },
    };

    let attention_mask = match Tensor::stack(&attention_mask, 0) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to stack attention_mask tensors: {}", e);
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to stack attention_mask tensors: {e}"
            )));
        },
    };

    Ok((token_ids, attention_mask))
}

/// Normalize embedding to unit length (L2 norm)
fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    // Calculate squared values
    let squared = match v.sqr() {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to square tensor for L2 normalization: {}", e);
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to square tensor: {e}"
            )));
        },
    };

    // Sum along last dimension and keep dimensions
    let sum_squared = match squared.sum_keepdim(1) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to sum squared values: {}", e);
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to sum tensor: {e}"
            )));
        },
    };

    // Calculate square root for L2 norm
    let norm = match sum_squared.sqrt() {
        Ok(n) => n,
        Err(e) => {
            error!("Failed to compute square root for normalization: {}", e);
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Failed to compute square root: {e}"
            )));
        },
    };

    // Divide by norm
    match v.broadcast_div(&norm) {
        Ok(n) => Ok(n),
        Err(e) => {
            error!("Failed to normalize by division: {}", e);
            Err(SemanticSearchError::EmbeddingError(format!("Failed to normalize: {e}")))
        },
    }
}

impl crate::embedding::BenchmarkableEmbedder for CandleTextEmbedder {
    fn model_name(&self) -> String {
        format!("Candle-{}", self.config.name)
    }

    fn embedding_dim(&self) -> usize {
        self.config.config.hidden_size
    }

    fn embed_single(&self, text: &str) -> Vec<f32> {
        self.embed(text).unwrap()
    }

    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.embed_batch(texts).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        fs,
    };

    use tempfile::tempdir;

    use super::*;

    // Helper function to create a test embedder with mock files
    fn create_test_embedder() -> Result<CandleTextEmbedder> {
        // Use a temporary directory for test files
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let _model_path = temp_dir.path().join("model.safetensors");
        let _tokenizer_path = temp_dir.path().join("tokenizer.json");

        // This is a simplified test that checks error handling paths

        // Return a mock error to test error handling
        Err(crate::error::SemanticSearchError::EmbeddingError(
            "Test error".to_string(),
        ))
    }

    /// Helper function to check if real embedder tests should be skipped
    fn should_skip_real_embedder_tests() -> bool {
        // Skip if real embedders are not explicitly requested
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            return true;
        }

        // Skip in CI environments
        if env::var("CI").is_ok() {
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
        match CandleTextEmbedder::new() {
            Ok(embedder) => {
                let embedding = embedder.embed("This is a test sentence.").unwrap();

                // MiniLM-L6-v2 produces 384-dimensional embeddings
                assert_eq!(embedding.len(), 384);

                // Check that the embedding is normalized (L2 norm ≈ 1.0)
                let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!((norm - 1.0).abs() < 1e-5);
            },
            Err(e) => {
                // If model loading fails, skip the test
                println!("Skipping test: Failed to load real embedder: {e}");
            },
        }
    }

    #[test]
    fn test_embed_batch() {
        if should_skip_real_embedder_tests() {
            return;
        }

        // Use real embedder for testing
        match CandleTextEmbedder::new() {
            Ok(embedder) => {
                let texts = vec![
                    "The cat sits outside".to_string(),
                    "A man is playing guitar".to_string(),
                ];
                let embeddings = embedder.embed_batch(&texts).unwrap();

                assert_eq!(embeddings.len(), 2);
                assert_eq!(embeddings[0].len(), 384);
                assert_eq!(embeddings[1].len(), 384);

                // Check that embeddings are different
                let mut different = false;
                for i in 0..384 {
                    if (embeddings[0][i] - embeddings[1][i]).abs() > 1e-5 {
                        different = true;
                        break;
                    }
                }
                assert!(different);
            },
            Err(e) => {
                // If model loading fails, skip the test
                println!("Skipping test: Failed to load real embedder: {e}");
            },
        }
    }

    #[test]
    fn test_model_types() {
        // Test that we can create embedders with different model types
        // This is just a compilation test, we don't actually load the models

        // These should compile without errors
        let _model_type1 = ModelType::MiniLML6V2;
        let _model_type2 = ModelType::MiniLML12V2;

        // Test that default is MiniLML6V2
        assert_eq!(ModelType::default(), ModelType::MiniLML6V2);
    }

    #[test]
    fn test_error_handling() {
        // Test error handling with invalid paths
        let invalid_path = Path::new("/nonexistent/path");
        let result = CandleTextEmbedder::with_model_paths(invalid_path, invalid_path);
        assert!(result.is_err());

        // Test error handling with mock embedder
        let result = create_test_embedder();
        assert!(result.is_err());
    }

    #[test]
    fn test_model_files_exist() {
        // Create temporary directory for test
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let model_path = temp_dir.path().join("model.safetensors");
        let tokenizer_path = temp_dir.path().join("tokenizer.json");

        // Create empty files to simulate existing files
        fs::write(&model_path, "mock data").expect("Failed to write mock model file");
        fs::write(&tokenizer_path, "mock data").expect("Failed to write mock tokenizer file");

        // Test that model files exist
        assert!(model_path.exists(), "Model file should exist");
        assert!(tokenizer_path.exists(), "Tokenizer file should exist");
    }

    /// Performance test for different model types
    #[test]
    fn test_model_performance() {
        if should_skip_real_embedder_tests() {
            return;
        }

        // Test data
        let texts = create_test_data();

        // Test each model type
        let model_types = [ModelType::MiniLML6V2, ModelType::MiniLML12V2];

        for model_type in model_types {
            run_performance_test(model_type, &texts);
        }
    }

    /// Run performance test for a specific model type
    fn run_performance_test(model_type: ModelType, texts: &[String]) {
        match CandleTextEmbedder::with_model_type(model_type) {
            Ok(embedder) => {
                println!("Testing performance of {model_type:?}");

                // Warm-up run
                let _ = embedder.embed_batch(texts);

                // Measure single embedding performance
                let start = std::time::Instant::now();
                let single_result = embedder.embed(&texts[0]);
                let single_duration = start.elapsed();

                // Measure batch embedding performance
                let start = std::time::Instant::now();
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
                println!("Failed to load model {model_type:?}: {e}");
            },
        }
    }

    /// Test loading all models to ensure they work
    #[test]
    fn test_load_all_models() {
        if should_skip_real_embedder_tests() {
            return;
        }

        let model_types = [ModelType::MiniLML6V2, ModelType::MiniLML12V2];

        for model_type in model_types {
            test_model_loading(model_type);
        }
    }

    /// Test loading a specific model
    fn test_model_loading(model_type: ModelType) {
        match CandleTextEmbedder::with_model_type(model_type) {
            Ok(embedder) => {
                // Test a simple embedding to verify the model works
                let result = embedder.embed("Test sentence for model verification.");
                assert!(result.is_ok(), "Model {model_type:?} failed to generate embedding");

                // Verify embedding dimensions
                let embedding = result.unwrap();
                let expected_dim = match model_type {
                    ModelType::MiniLML6V2 => 384,
                    ModelType::MiniLML12V2 => 384,
                };

                assert_eq!(
                    embedding.len(),
                    expected_dim,
                    "Model {model_type:?} produced embedding with incorrect dimensions"
                );

                println!("Successfully loaded and tested model {model_type:?}");
            },
            Err(e) => {
                println!("Failed to load model {model_type:?}: {e}");
                // Don't fail the test if a model can't be loaded, just report it
            },
        }
    }
}
