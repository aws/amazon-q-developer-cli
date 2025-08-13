//! Semantic Search Client - A library for managing semantic memory contexts
//!
//! This crate provides functionality for creating, managing, and searching
//! semantic memory contexts. It uses vector embeddings to enable semantic search
//! across text and code.

#![warn(missing_docs)]

/// Client implementation for semantic search operations
pub mod client;
/// Configuration management for semantic search
pub mod config;
/// Error types for semantic search operations
pub mod error;
/// Vector index implementation
pub mod index;
/// Model validation for SHA verification
pub mod model_validator;
/// Pattern filtering for file selection
pub mod pattern_filter;
/// File processing utilities
pub mod processing;
/// Data types for semantic search operations
pub mod types;

/// Text embedding functionality
pub mod embedding;

pub use client::SemanticSearchClient;
pub use config::SemanticSearchConfig;
pub use error::{
    Result,
    SemanticSearchError,
};
pub use types::{
    DataPoint,
    FileType,
    KnowledgeContext,
    OperationStatus,
    OperationType,
    ProgressInfo,
    ProgressStatus,
    SearchResult,
    SystemStatus,
};
