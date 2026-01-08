//! Service layer for LSP operations
//!
//! This module provides a clean separation of concerns with 3 focused services:
//! - WorkspaceService: Shared file operations (open_file, initialization)
//! - SymbolService: All symbol-related operations (find, goto, references, document symbols)
//! - CodingService: Code manipulation operations (rename, format)
//! - TreeSitterSymbolService: AST-based pattern search using ast-grep
//! - TreeSitterCodingService: AST-based pattern rewrite using ast-grep

pub mod coding_service;
pub mod symbol_service;
pub mod tree_sitter_coding_service;
pub mod tree_sitter_symbol_service;
#[cfg(test)]
pub mod tree_sitter_symbol_tests;
pub mod workspace_service;

pub use coding_service::*;
pub use symbol_service::*;
pub use workspace_service::*;
