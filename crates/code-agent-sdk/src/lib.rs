mod config;
pub mod lsp;
pub mod mcp;
pub mod model;
pub mod sdk;
pub mod utils;

pub use model::*;
pub use sdk::client::CodeIntelligence;
pub use sdk::CodeIntelligenceBuilder;

// Export model types with explicit names to avoid conflicts
pub use model::entities::{
    DefinitionInfo as ApiDefinitionInfo, ReferenceInfo as ApiReferenceInfo,
    SymbolInfo as ApiSymbolInfo, DiagnosticInfo as ApiDiagnosticInfo,
    DiagnosticSeverity as ApiDiagnosticSeverity,
};
