//! Utility functions for file operations, workspace management, and common tasks.
//!
//! This module provides helper functions that are used throughout the code intelligence
//! library for handling files, paths, and workspace operations.

pub mod file;
pub mod fuzzy_search;
pub mod position;

// Re-export commonly used functions for convenience
pub use file::{apply_text_edits, apply_workspace_edit, canonicalize_path, ensure_absolute_path};
pub use position::{from_lsp_position, to_lsp_position};
