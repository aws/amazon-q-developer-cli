//! Standardized benchmark tests for embedding models
//!
//! This module provides standardized benchmark tests for comparing
//! different embedding model implementations.

use std::env;

use crate::embedding::run_standard_benchmark;
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
use crate::embedding::{
    CandleTextEmbedder,
    ModelType,
};

/// Helper function to check if real embedder tests should be skipped
fn should_skip_real_embedder_tests() -> bool {
    // Skip if real embedders are not explicitly requested
    if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
        println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
        return true;
    }

    // Skip in CI environments
    if env::var("CI").is_ok() {
        println!("Skipping test: Running in CI environment");
        return true;
    }

    false
}

/// Run benchmark for a Candle model
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
fn benchmark_candle_model(model_type: ModelType) {
    match CandleTextEmbedder::with_model_type(model_type) {
        Ok(embedder) => {
            println!("Benchmarking Candle model: {model_type:?}");
            let results = run_standard_benchmark(&embedder);
            println!(
                "Model: {}, Embedding dim: {}, Single time: {:?}, Batch time: {:?}, Avg per text: {:?}",
                results.model_name,
                results.embedding_dim,
                results.single_time,
                results.batch_time,
                results.avg_time_per_text()
            );
        },
        Err(e) => {
            println!("Failed to load Candle model {model_type:?}: {e}");
        },
    }
}

/// Standardized benchmark test for embedding models
#[test]
fn test_standard_benchmark() {
    if should_skip_real_embedder_tests() {
        return;
    }

    println!("Running standardized benchmark tests for embedding models");
    println!("--------------------------------------------------------");

    // Benchmark Candle models (not available on Linux ARM)
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    {
        benchmark_candle_model(ModelType::MiniLML6V2);
        benchmark_candle_model(ModelType::MiniLML12V2);
    }

    println!("--------------------------------------------------------");
    println!("Benchmark tests completed");
}
