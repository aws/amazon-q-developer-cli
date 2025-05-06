//! Memory Bank Client - A library for managing semantic memory contexts
//!
//! This crate provides functionality for creating, managing, and searching
//! semantic memory contexts. It uses vector embeddings to enable semantic search
//! across text and code.

#![warn(missing_docs)]

/// Client implementation for memory bank operations
pub mod client;
/// Text embedding functionality
pub mod embedding;
/// Error types for memory bank operations
pub mod error;
/// Vector index implementation
pub mod index;
/// File processing utilities
pub mod processing;
/// Data types for memory operations
pub mod types;

pub use client::MemoryBankClient;
pub use error::{
    MemoryBankError,
    Result,
};
pub use types::{
    DataPoint,
    FileType,
    MemoryContext,
    ProgressStatus,
    SearchResult,
};
