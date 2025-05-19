//! Standardized benchmark tests for embedding models
//!
//! This module provides standardized benchmark tests for comparing
//! different embedding model implementations.

use std::env;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::embedding::TextEmbedder;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::embedding::onnx_models::OnnxModelType;
use crate::embedding::{
    BM25TextEmbedder,
    run_standard_benchmark,
};
#[cfg(not(target_arch = "aarch64"))]
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
#[cfg(not(target_arch = "aarch64"))]
fn benchmark_candle_model(model_type: ModelType) {
    match CandleTextEmbedder::with_model_type(model_type) {
        Ok(embedder) => {
            println!("Benchmarking Candle model: {:?}", model_type);
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
            println!("Failed to load Candle model {:?}: {}", model_type, e);
        },
    }
}

/// Run benchmark for an ONNX model
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn benchmark_onnx_model(model_type: OnnxModelType) {
    match TextEmbedder::with_model_type(model_type) {
        Ok(embedder) => {
            println!("Benchmarking ONNX model: {:?}", model_type);
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
            println!("Failed to load ONNX model {:?}: {}", model_type, e);
        },
    }
}

/// Run benchmark for BM25 model
fn benchmark_bm25_model() {
    match BM25TextEmbedder::new() {
        Ok(embedder) => {
            println!("Benchmarking BM25 model");
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
            println!("Failed to load BM25 model: {}", e);
        },
    }
}

/// Standardized benchmark test for all embedding models
#[test]
fn test_standard_benchmark() {
    if should_skip_real_embedder_tests() {
        return;
    }

    println!("Running standardized benchmark tests for embedding models");
    println!("--------------------------------------------------------");

    // Benchmark BM25 model (available on all platforms)
    benchmark_bm25_model();

    // Benchmark Candle models (not available on arm64)
    #[cfg(not(target_arch = "aarch64"))]
    {
        benchmark_candle_model(ModelType::MiniLML6V2);
        benchmark_candle_model(ModelType::MiniLML12V2);
    }

    // Benchmark ONNX models (available on macOS and Windows)
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        benchmark_onnx_model(OnnxModelType::MiniLML6V2Q);
        benchmark_onnx_model(OnnxModelType::MiniLML12V2Q);
    }

    println!("--------------------------------------------------------");
    println!("Benchmark tests completed");
}
