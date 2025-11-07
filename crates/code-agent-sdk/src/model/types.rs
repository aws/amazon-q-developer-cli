use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

/// Symbol kind for API requests - internal enum to avoid exposing lsp_types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiSymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Constant,
    Variable,
    Module,
    Import,
}

impl ApiSymbolKind {
    /// Convert to lsp_types::SymbolKind for internal use
    pub fn to_lsp_symbol_kind(&self) -> lsp_types::SymbolKind {
        match self {
            ApiSymbolKind::Function => lsp_types::SymbolKind::FUNCTION,
            ApiSymbolKind::Method => lsp_types::SymbolKind::METHOD,
            ApiSymbolKind::Class => lsp_types::SymbolKind::CLASS,
            ApiSymbolKind::Struct => lsp_types::SymbolKind::STRUCT,
            ApiSymbolKind::Enum => lsp_types::SymbolKind::ENUM,
            ApiSymbolKind::Interface => lsp_types::SymbolKind::INTERFACE,
            ApiSymbolKind::Constant => lsp_types::SymbolKind::CONSTANT,
            ApiSymbolKind::Variable => lsp_types::SymbolKind::VARIABLE,
            ApiSymbolKind::Module => lsp_types::SymbolKind::MODULE,
            ApiSymbolKind::Import => lsp_types::SymbolKind::MODULE, // Map import to module
        }
    }
}

impl FromStr for ApiSymbolKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "function" => Ok(ApiSymbolKind::Function),
            "method" => Ok(ApiSymbolKind::Method),
            "class" => Ok(ApiSymbolKind::Class),
            "struct" => Ok(ApiSymbolKind::Struct),
            "enum" => Ok(ApiSymbolKind::Enum),
            "interface" => Ok(ApiSymbolKind::Interface),
            "constant" => Ok(ApiSymbolKind::Constant),
            "variable" => Ok(ApiSymbolKind::Variable),
            "module" => Ok(ApiSymbolKind::Module),
            "import" => Ok(ApiSymbolKind::Import),
            _ => Err(format!("Unknown symbol kind: {}", s)),
        }
    }
}

/// Request to find symbols by name with optional filtering.
///
/// This request supports fuzzy searching for symbols across the workspace
/// or within specific files, with optional type filtering and result limiting.
#[derive(Debug, Clone)]
pub struct FindSymbolsRequest {
    /// The symbol name to search for (empty string returns all symbols)
    pub symbol_name: String,
    /// Optional file path to search within (searches workspace if None)
    pub file_path: Option<PathBuf>,
    /// Optional symbol type filter (e.g., Function, Class, Variable)
    pub symbol_type: Option<ApiSymbolKind>,
    /// Maximum number of results to return (default: 20, max: 50)
    pub limit: Option<u32>,
    /// Whether to prioritize exact matches over fuzzy matches
    pub exact_match: bool,
}

/// Request to get specific symbols by name.
///
/// This request retrieves symbols directly without fuzzy matching,
/// useful for checking symbol existence or extracting specific code.
#[derive(Debug, Clone)]
pub struct GetSymbolsRequest {
    /// List of symbol names to retrieve
    pub symbols: Vec<String>,
    /// Whether to include source code in the response
    pub include_source: bool,
    /// Optional file path to search within
    pub file_path: Option<PathBuf>,
    /// Optional starting row for context
    pub start_row: Option<u32>,
    /// Optional starting column for context
    pub start_column: Option<u32>,
}

/// Request to find references by symbol name.
///
/// This request first finds the symbol definition, then locates all references to it.
#[derive(Debug, Clone)]
pub struct FindReferencesByNameRequest {
    /// The symbol name to find references for
    pub symbol_name: String,
}

/// Request to find references by file location.
///
/// This request finds all references to the symbol at a specific position in a file.
/// Uses 1-based line and column numbers for user-friendly ergonomics.
#[derive(Debug, Clone)]
pub struct FindReferencesByLocationRequest {
    /// File path containing the symbol
    pub file_path: PathBuf,
    /// Line number (1-based) of the symbol
    pub row: u32,
    /// Column number (1-based) of the symbol
    pub column: u32,
}

/// Request to rename a symbol.
///
/// This request renames a symbol at a specific location, with optional dry-run mode
/// to preview changes without applying them.
#[derive(Debug, Clone)]
pub struct RenameSymbolRequest {
    /// File path containing the symbol to rename
    pub file_path: PathBuf,
    /// Starting line number (1-based) of the symbol
    pub row: u32,
    /// Starting column number (1-based) of the symbol
    pub column: u32,
    /// New name for the symbol
    pub new_name: String,
    /// Whether to preview changes without applying them
    pub dry_run: bool,
}

/// Request to format code.
///
/// This request formats code in a specific file or across the entire workspace,
/// with configurable formatting options.
#[derive(Debug, Clone)]
pub struct FormatCodeRequest {
    /// Optional file path to format (formats workspace if None)
    pub file_path: Option<PathBuf>,
    /// Tab size for indentation
    pub tab_size: u32,
    /// Whether to use spaces instead of tabs
    pub insert_spaces: bool,
}

/// Request to go to symbol definition.
///
/// This request finds the definition location of a symbol at a specific position.
/// Uses 1-based line and column numbers for user-friendly ergonomics.
#[derive(Debug, Clone)]
pub struct GotoDefinitionRequest {
    /// File path containing the symbol
    pub file_path: PathBuf,
    /// Line number (1-based) where the symbol is located
    pub row: u32,
    /// Column number (1-based) where the symbol is located
    pub column: u32,
    /// Whether to include source code in the response
    pub show_source: bool,
}

/// Request to get all symbols from a document/file.
///
/// This request retrieves the complete symbol hierarchy from a specific file.
#[derive(Debug, Clone)]
pub struct GetDocumentSymbolsRequest {
    /// Path to the file to analyze
    pub file_path: PathBuf,
}

/// Request to get diagnostics for a document (pull model).
///
/// This request retrieves diagnostics for a specific document using the pull model,
/// giving the client control over when diagnostics are computed.
#[derive(Debug, Clone)]
pub struct GetDocumentDiagnosticsRequest {
    /// Path to the file to get diagnostics for
    pub file_path: PathBuf,
    /// Optional identifier provided during registration
    pub identifier: Option<String>,
    /// Optional result ID from a previous response for incremental updates
    pub previous_result_id: Option<String>,
}

/// Request to open a file in the language server.
///
/// This request opens a file for analysis, making it available for code intelligence operations.
#[derive(Debug, Clone)]
pub struct OpenFileRequest {
    /// Path to the file to open
    pub file_path: PathBuf,
    /// File content as string
    pub content: String,
}

/// Configuration for a language server.
///
/// This struct defines how to start and communicate with a specific language server,
/// including the command, arguments, and supported file extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageServerConfig {
    /// Unique name for this language server
    pub name: String,
    /// Command to execute the language server
    pub command: String,
    /// Command-line arguments for the language server
    pub args: Vec<String>,
    /// File extensions this language server handles (e.g., ["rs", "toml"])
    pub file_extensions: Vec<String>,
    /// Patterns to exclude from file watching (e.g., ["**/target/**", "**/node_modules/**"])
    pub exclude_patterns: Vec<String>,
    /// Optional initialization options sent to the language server
    pub initialization_options: Option<serde_json::Value>,
}

/// Information about workspace detection results.
///
/// This struct contains the results of analyzing a workspace to determine
/// what programming languages are present and which language servers are available.
#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    /// Root path of the workspace
    pub root_path: PathBuf,
    /// List of detected programming languages
    pub detected_languages: Vec<String>,
    /// List of available language servers with their status
    pub available_lsps: Vec<LspInfo>,
}

/// Information about a language server's availability.
///
/// This struct provides details about whether a language server is installed
/// and available for use.
#[derive(Debug, Clone)]
pub struct LspInfo {
    /// Name of the language server
    pub name: String,
    /// Command used to start the language server
    pub command: String,
    /// Programming languages supported by this server
    pub languages: Vec<String>,
    /// Whether the language server is installed and available
    pub is_available: bool,
    /// Version information if available
    pub version: Option<String>,
}

// ============================================================================
// File Watching Types (Crate Internal)
// ============================================================================

/// File system event for internal file watching system
#[derive(Debug, Clone)]
pub(crate) struct FsEvent {
    pub(crate) uri: url::Url,
    pub(crate) kind: FsEventKind,
    pub(crate) timestamp: std::time::Instant,
}

/// Types of file system events
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FsEventKind {
    Created,
    Modified,
    Deleted,
    Renamed { from: url::Url },
}

