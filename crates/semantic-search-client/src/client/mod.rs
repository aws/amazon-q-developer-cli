/// Main client implementation
pub mod async_implementation;
/// Synchronous client implementation
pub mod implementation;

/// Background processing modules
pub mod background;
/// Context management modules
pub mod context;
/// Model management modules
pub mod model;
/// Operation management modules
pub mod operation;

/// Embedder factory utilities
pub mod embedder_factory;
/// Utility functions
pub mod utils;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
/// Hosted model client for downloading models
pub mod hosted_model_client;

pub use async_implementation::AsyncSemanticSearchClient;
pub use context::{
    BM25Context,
    SemanticContext,
};
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
pub use hosted_model_client::HostedModelClient;
pub use implementation::SemanticSearchClient;
