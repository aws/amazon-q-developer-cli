//! Data models and type definitions for the code intelligence library.
//!
//! This module contains all the data structures used for requests, responses,
//! and internal representation of code symbols and workspace information.

pub mod entities;
pub mod types;

// Re-export all types for convenience
pub use entities::{DefinitionInfo, ReferenceInfo, SymbolInfo};
pub use types::{
    FindReferencesByLocationRequest, FindReferencesByNameRequest, FindSymbolsRequest,
    FormatCodeRequest, GetDocumentDiagnosticsRequest, GetDocumentSymbolsRequest, GetSymbolsRequest, GotoDefinitionRequest,
    LanguageServerConfig, LspInfo, OpenFileRequest, RenameSymbolRequest, WorkspaceInfo,
};

// Re-export crate-internal types
pub(crate) use types::{FsEvent, FsEventKind};
