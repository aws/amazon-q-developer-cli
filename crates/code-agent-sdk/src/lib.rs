mod config;
pub mod error;
pub mod lsp;
pub mod mcp;
pub mod model;
pub mod sdk;
pub(crate) mod tree_sitter;
pub mod utils;

// Export error types
// Re-export config helper
pub use config::ConfigManager;
pub use error::{
    CodeIntelligenceError,
    LanguageError,
    Result as CodeResult,
};
// Export model types with explicit names to avoid conflicts
pub use model::entities::{
    DefinitionInfo as ApiDefinitionInfo,
    DiagnosticInfo as ApiDiagnosticInfo,
    DiagnosticSeverity as ApiDiagnosticSeverity,
    EnclosingSymbol,
    PatternMatch,
    ReferenceInfo as ApiReferenceInfo,
    ReferencesResult as ApiReferencesResult,
    RewriteResult,
    SourceRange,
    SymbolInfo as ApiSymbolInfo,
};
pub use model::types::{
    PatternRewriteRequest,
    PatternSearchRequest,
};
pub use model::*;
pub use sdk::CodeIntelligenceBuilder;
pub use sdk::client::CodeIntelligence;
