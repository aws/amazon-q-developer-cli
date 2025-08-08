/// Async client implementation for semantic search operations with proper cancellation
mod async_implementation;
/// Factory for creating embedders
pub mod embedder_factory;
/// Hosted model client for downloading models from CDN
pub mod hosted_model_client;
/// Client implementation for semantic search operations
mod implementation;
/// Semantic context implementation for search operations
pub mod semantic_context;
/// Utility functions for semantic search operations
pub mod utils;

// Re-export types for external use
pub use async_implementation::AsyncSemanticSearchClient;
pub use hosted_model_client::HostedModelClient;
pub use implementation::SemanticSearchClient;
pub use semantic_context::SemanticContext;
