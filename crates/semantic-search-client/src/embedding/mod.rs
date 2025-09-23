#[cfg(test)]
mod benchmark_test;
mod benchmark_utils;
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
mod candle;
mod candle_models;
/// Mock embedder for testing and as placeholder for BM25
pub mod mock;
mod trait_def;

pub use benchmark_utils::{
    BenchmarkResults,
    BenchmarkableEmbedder,
    create_standard_test_data,
    run_standard_benchmark,
};
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
pub use candle::CandleTextEmbedder;
pub use candle_models::{
    ModelConfig,
    ModelType,
};
pub use mock::MockTextEmbedder;
pub use trait_def::{
    EmbeddingType,
    TextEmbedderTrait,
};
