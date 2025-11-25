mod config;
pub mod lsp;
pub mod mcp;
pub mod model;
pub mod sdk;
pub mod utils;

// Export model types with explicit names to avoid conflicts
pub use model::entities::{
    DefinitionInfo as ApiDefinitionInfo,
    DiagnosticInfo as ApiDiagnosticInfo,
    DiagnosticSeverity as ApiDiagnosticSeverity,
    ReferenceInfo as ApiReferenceInfo,
    SymbolInfo as ApiSymbolInfo,
};
pub use model::*;
pub use sdk::CodeIntelligenceBuilder;
pub use sdk::client::CodeIntelligence;
