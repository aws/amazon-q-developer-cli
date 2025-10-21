//! Service layer for LSP operations
//!
//! This module provides a clean separation of concerns with 3 focused services:
//! - WorkspaceService: Shared file operations (open_file, initialization)
//! - SymbolService: All symbol-related operations (find, goto, references, document symbols)
//! - CodingService: Code manipulation operations (rename, format)

pub mod coding_service;
pub mod symbol_service;
pub mod workspace_service;

pub use coding_service::*;
pub use symbol_service::*;
pub use workspace_service::*;
