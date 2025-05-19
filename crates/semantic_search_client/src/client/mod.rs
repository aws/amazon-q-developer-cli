/// Factory for creating embedders
pub mod embedder_factory;
/// Client implementation for semantic search operations
mod implementation;
/// Semantic context implementation for search operations
pub mod semantic_context;
/// Utility functions for semantic search operations
pub mod utils;

pub use implementation::SemanticSearchClient;
pub use semantic_context::SemanticContext;
