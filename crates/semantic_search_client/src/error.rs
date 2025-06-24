use std::{
    fmt,
    io,
};

/// Result type for semantic search operations
pub type Result<T> = std::result::Result<T, SemanticSearchError>;

/// Error types for semantic search operations
#[derive(Debug)]
pub enum SemanticSearchError {
    /// I/O error
    IoError(io::Error),
    /// JSON serialization/deserialization error
    SerdeError(serde_json::Error),
    /// JSON serialization/deserialization error (string variant)
    SerializationError(String),
    /// Invalid path
    InvalidPath(String),
    /// Context not found
    ContextNotFound(String),
    /// Operation failed
    OperationFailed(String),
    /// Invalid argument
    InvalidArgument(String),
    /// Embedding error
    EmbeddingError(String),
}

impl fmt::Display for SemanticSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticSearchError::IoError(e) => write!(f, "I/O error: {}", e),
            SemanticSearchError::SerdeError(e) => write!(f, "Serialization error: {}", e),
            SemanticSearchError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            SemanticSearchError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            SemanticSearchError::ContextNotFound(id) => write!(f, "Context not found: {}", id),
            SemanticSearchError::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
            SemanticSearchError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            SemanticSearchError::EmbeddingError(msg) => write!(f, "Embedding error: {}", msg),
        }
    }
}

impl std::error::Error for SemanticSearchError {}

impl From<io::Error> for SemanticSearchError {
    fn from(error: io::Error) -> Self {
        SemanticSearchError::IoError(error)
    }
}

impl From<serde_json::Error> for SemanticSearchError {
    fn from(error: serde_json::Error) -> Self {
        SemanticSearchError::SerdeError(error)
    }
}
