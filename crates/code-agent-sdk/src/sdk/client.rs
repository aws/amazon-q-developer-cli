use crate::model::entities::{DefinitionInfo, ReferenceInfo, SymbolInfo};
use crate::model::types::*;
use crate::sdk::services::*;
use crate::sdk::workspace_manager::WorkspaceManager;
use crate::sdk::CodeIntelligenceBuilder;
use anyhow::Result;
use std::path::PathBuf;

/// **Language-agnostic code intelligence client for LLM tools**
///
/// Provides semantic code understanding capabilities through Language Server Protocol (LSP)
/// integration. Enables AI agents to navigate codebases, find symbols, understand references,
/// and perform code operations across different programming languages.
///
/// # Features
/// - Multi-language support (TypeScript/JavaScript, Rust, Python)
/// - Symbol discovery with fuzzy search
/// - Reference finding and go-to-definition
/// - Code formatting and symbol renaming
/// - Workspace detection and management
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::{CodeIntelligence, FindSymbolsRequest};
/// use std::path::PathBuf;
///
/// # async fn example() {
/// // Create client with auto-detected languages
/// let mut client = CodeIntelligence::builder()
///     .workspace_root(PathBuf::from("."))
///     .auto_detect_languages()
///     .build()
///     .expect("Failed to build client");
///
/// // Initialize language servers
/// client.initialize().await.expect("Failed to initialize");
///
/// // Find symbols in workspace
/// let symbols = client.find_symbols(FindSymbolsRequest {
///     symbol_name: "function_name".to_string(),
///     file_path: None,
///     symbol_type: None,
///     limit: Some(10),
///     exact_match: false,
/// }).await.expect("Failed to find symbols");
/// # }
/// # }
/// ```ignore
pub struct CodeIntelligence {
    symbol_service: Box<dyn SymbolService>,
    coding_service: Box<dyn CodingService>,
    workspace_service: Box<dyn WorkspaceService>,
    workspace_manager: WorkspaceManager,
}

impl std::fmt::Debug for CodeIntelligence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeIntelligence")
            .field("workspace_root", &self.workspace_manager.workspace_root())
            .finish()
    }
}

impl Clone for CodeIntelligence {
    fn clone(&self) -> Self {
        // Create a new instance with the same workspace root
        // Since services are stateless, we can create new instances
        let workspace_root = self.workspace_manager.workspace_root().to_path_buf();
        Self::new(workspace_root)
    }
}

impl CodeIntelligence {
    /// **Create a new CodeIntelligence instance**
    ///
    /// Initializes a client with the specified workspace root directory.
    /// Use the builder pattern for more advanced configuration options.
    ///
    /// # Arguments
    /// * `workspace_root` - Root directory of the workspace to analyze
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::CodeIntelligence;
    /// use std::path::PathBuf;
    ///
    /// let client = CodeIntelligence::new(PathBuf::from("/path/to/workspace"));
    /// ```ignore
    pub fn new(workspace_root: PathBuf) -> Self {
        let workspace_manager = WorkspaceManager::new(workspace_root);

        let workspace_service = Box::new(LspWorkspaceService::new());
        let symbol_service = Box::new(LspSymbolService::new(Box::new(LspWorkspaceService::new())));
        let coding_service = Box::new(LspCodingService::new(Box::new(LspWorkspaceService::new())));

        Self {
            symbol_service,
            coding_service,
            workspace_service,
            workspace_manager,
        }
    }

    /// **Create a builder for advanced configuration**
    ///
    /// Returns a builder instance for fluent configuration of the CodeIntelligence client.
    /// Recommended for complex setups with multiple languages or custom configurations.
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::CodeIntelligence;
    /// use std::path::PathBuf;
    ///
    /// let client = CodeIntelligence::builder()
    ///     .workspace_root(PathBuf::from("."))
    ///     .add_language("typescript")
    ///     .add_language("rust")
    ///     .build()
    ///     .expect("Failed to build CodeIntelligence");
    /// ```ignore
    pub fn builder() -> CodeIntelligenceBuilder {
        CodeIntelligenceBuilder::new()
    }

    // Workspace operations

    /// **Detect workspace languages and available language servers**
    ///
    /// Scans the workspace directory to identify programming languages based on
    /// file extensions and checks which language servers are available on the system.
    ///
    /// # Returns
    /// * `Result<WorkspaceInfo>` - Information about detected languages and available LSPs
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::CodeIntelligence;
    /// use std::path::PathBuf;
    ///
    /// let mut client = CodeIntelligence::new(PathBuf::from("."));
    /// let workspace_info = client.detect_workspace().expect("Failed to detect workspace");
    /// println!("Detected languages: {:?}", workspace_info.detected_languages);
    /// ```ignore
    pub fn detect_workspace(&mut self) -> Result<WorkspaceInfo> {
        self.workspace_manager.detect_workspace()
    }

    /// **Initialize all configured language servers**
    ///
    /// Starts and initializes all registered language servers. This should be called
    /// before performing any code intelligence operations.
    ///
    /// # Returns
    /// * `Result<()>` - Success or initialization error
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::CodeIntelligence;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(PathBuf::from("."));
    /// client.initialize().await.expect("Operation failed");
    /// 
    /// # }
    /// ```ignore
    pub async fn initialize(&mut self) -> Result<()> {
        self.workspace_manager.initialize().await
    }

    // Symbol operations

    /// **Find symbols by name across workspace or within a specific file**
    ///
    /// Searches for symbols using semantic understanding from language servers.
    /// Supports filtering by symbol type and limiting results.
    ///
    /// # Arguments
    /// * `request` - Search parameters including symbol name, optional file path, and filters
    ///
    /// # Returns
    /// * `Result<Vec<SymbolInfo>>` - List of matching symbols with location and metadata
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, FindSymbolsRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client.find_symbols(FindSymbolsRequest {
    ///     symbol_name: "function_name".to_string(),
    ///     file_path: Some(Path::new("src/main.rs").to_path_buf()),
    ///     symbol_type: None,
    ///     limit: Some(10),
    ///     exact_match: false,
    /// }).await.expect("Operation failed");
    /// 
    /// # }
    /// ```ignore
    pub async fn find_symbols(&mut self, request: FindSymbolsRequest) -> Result<Vec<SymbolInfo>> {
        self.symbol_service
            .find_symbols(&mut self.workspace_manager, request)
            .await
    }

    /// **Get symbols by exact names**
    ///
    /// Direct symbol retrieval for existence checking or code extraction.
    /// Useful when you have specific symbol names to look up.
    ///
    /// # Arguments
    /// * `request` - Request containing list of symbol names to retrieve
    ///
    /// # Returns
    /// * `Result<Vec<SymbolInfo>>` - List of found symbols
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, GetSymbolsRequest};
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client.get_symbols(GetSymbolsRequest {
    ///     symbols: vec!["main".to_string(), "init".to_string()],
    ///     file_path: None,
    ///     include_source: false,
    ///     row: None,
    ///     column: None,
    /// }).await.expect("Operation failed");
    /// # }
    /// ```ignore
    pub async fn get_symbols(&mut self, request: GetSymbolsRequest) -> Result<Vec<SymbolInfo>> {
        self.symbol_service
            .get_symbols(&mut self.workspace_manager, request)
            .await
    }

    /// **Get all symbols from a document/file**
    ///
    /// Retrieves complete symbol hierarchy from a specific file, providing
    /// a comprehensive overview of the file's structure and contents.
    ///
    /// # Arguments
    /// * `request` - Document symbols request parameters
    ///
    /// # Returns
    /// * `Result<Vec<SymbolInfo>>` - All symbols found in the file
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, GetDocumentSymbolsRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client.get_document_symbols(GetDocumentSymbolsRequest {
    ///     file_path: Path::new("src/main.rs").to_path_buf(),
    /// }).await.expect("Operation failed");
    /// for symbol in symbols {
    ///     println!("{} {} at line {}",
    ///         symbol.symbol_type.as_deref().unwrap_or("Unknown"),
    ///         symbol.name,
    ///         symbol.start_row
    ///     );
    /// }
    /// 
    /// # }
    /// ```ignore
    pub async fn get_document_symbols(
        &mut self,
        request: GetDocumentSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>> {
        self.symbol_service
            .get_document_symbols(&mut self.workspace_manager, &request.file_path, true)
            .await
    }

    /// **Navigate to symbol definition**
    ///
    /// Finds the definition location of a symbol at the specified position.
    /// Equivalent to "Go to Definition" functionality in IDEs.
    ///
    /// # Arguments
    /// * `request` - Definition request parameters including file path, position, and options
    ///
    /// # Returns
    /// * `Result<Option<DefinitionInfo>>` - Definition location, or None if not found
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, GotoDefinitionRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// if let Some(definition) = client.goto_definition(GotoDefinitionRequest {
    ///     file_path: Path::new("src/main.rs").to_path_buf(),
    ///     row: 10,        // line 10
    ///     column: 5,    // column 5
    ///     show_source: true // include source
    /// }).await? {
    ///     println!("Definition found at {}:{}", definition.start_row, definition.start_column);
    /// }
    /// 
    /// # }
    /// ```ignore
    pub async fn goto_definition(
        &mut self,
        request: GotoDefinitionRequest,
    ) -> Result<Option<DefinitionInfo>> {
        self.symbol_service
            .goto_definition(
                &mut self.workspace_manager,
                &request.file_path,
                request.row,
                request.column,
                request.show_source,
            )
            .await
    }

    /// **Find all references to a symbol at a specific location**
    ///
    /// Locates all references to the symbol at the specified file position.
    /// Provides precise reference analysis based on cursor position.
    ///
    /// # Arguments
    /// * `request` - Location parameters including file path, line, and column
    ///
    /// # Returns
    /// * `Result<Vec<ReferenceInfo>>` - List of all references to the symbol
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, FindReferencesByLocationRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let references = client.find_references_by_location(FindReferencesByLocationRequest {
    ///     file_path: Path::new("src/main.rs").to_path_buf(),
    ///     row: 10,    // 0-based line number
    ///     column: 5,   // 0-based column number
    /// }).await.expect("Operation failed");
    ///
    /// for reference in references {
    ///     println!("Reference in {} at {}:{}",
    ///         reference.file_path, reference.start_row, reference.start_column);
    /// }
    /// 
    /// # }
    /// ```ignore
    pub async fn find_references_by_location(
        &mut self,
        request: FindReferencesByLocationRequest,
    ) -> Result<Vec<ReferenceInfo>> {
        self.symbol_service
            .find_references_by_location(&mut self.workspace_manager, request)
            .await
    }

    /// **Find all references to a symbol by name**
    ///
    /// Searches for a symbol by name first, then locates all references to that symbol
    /// across the workspace. Useful when you know the symbol name but not its exact location.
    ///
    /// # Arguments
    /// * `request` - Search parameters including the symbol name
    ///
    /// # Returns
    /// * `Result<Vec<ReferenceInfo>>` - List of all references to the symbol
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, FindReferencesByNameRequest};
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let references = client.find_references_by_name(FindReferencesByNameRequest {
    ///     symbol_name: "myFunction".to_string(),
    /// }).await.expect("Operation failed");
    ///
    /// for reference in references {
    ///     println!("Reference at {}:{}", reference.start_row, reference.start_column);
    /// }
    /// 
    /// # }
    /// ```ignore
    pub async fn find_references_by_name(
        &mut self,
        request: FindReferencesByNameRequest,
    ) -> Result<Vec<ReferenceInfo>> {
        self.symbol_service
            .find_references_by_name(&mut self.workspace_manager, request)
            .await
    }

    // Coding operations - delegate to CodingService

    /// **Rename a symbol with workspace-wide updates**
    ///
    /// Performs intelligent renaming of a symbol at the specified position, updating
    /// all references across the workspace. Uses semantic understanding to ensure
    /// safe and complete renaming.
    ///
    /// # Arguments
    /// * `request` - Rename parameters including file path, position, and new name
    ///
    /// # Returns
    /// * `Result<Option<WorkspaceEdit>>` - Workspace edits to apply, or None if rename not possible
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, RenameSymbolRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let workspace_edit = client.rename_symbol(RenameSymbolRequest {
    ///     file_path: Path::new("src/main.rs").to_path_buf(),
    ///     row: 10,
    ///     column: 5,
    ///     new_name: "newFunctionName".to_string(),
    ///     dry_run: true, // Preview changes without applying
    /// }).await.expect("Operation failed");
    ///
    /// if let Some(edit) = workspace_edit {
    ///     println!("Rename would affect {} files",
    ///         edit.changes.as_ref().map(|c| c.len()).unwrap_or(0));
    /// }
    /// 
    /// # }
    /// ```ignore
    pub async fn rename_symbol(
        &mut self,
        request: RenameSymbolRequest,
    ) -> Result<Option<crate::model::entities::RenameResult>> {
        let lsp_edit = self.coding_service
            .rename_symbol(&mut self.workspace_manager, request)
            .await?;
            
        Ok(lsp_edit.map(|edit| {
            crate::model::entities::RenameResult::from_lsp_workspace_edit(&edit)
        }))
    }

    /// **Format code in a file using the appropriate language server**
    ///
    /// Applies language-specific formatting to code using the configured language
    /// server's formatting capabilities. Supports customizable formatting options.
    ///
    /// # Arguments
    /// * `request` - Formatting parameters including file path and formatting options
    ///
    /// # Returns
    /// * `Result<Vec<TextEdit>>` - List of text edits to apply for formatting
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, FormatCodeRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let edits = client.format_code(FormatCodeRequest {
    ///     file_path: Some(Path::new("src/main.ts").to_path_buf()),
    ///     tab_size: 2,
    ///     insert_spaces: true,
    /// }).await.expect("Operation failed");
    ///
    /// println!("Applied {} formatting edits", edits.len());
    /// 
    /// # }
    /// ```ignore
    pub async fn format_code(
        &mut self,
        request: FormatCodeRequest,
    ) -> Result<usize> {
        self.coding_service
            .format_code(&mut self.workspace_manager, request)
            .await
    }

    // File operations - delegate to WorkspaceService

    /// **Open a file in the language server for analysis**
    ///
    /// Opens a file in the appropriate language server, making it available for
    /// code intelligence operations. Files are automatically opened when needed,
    /// but this method allows explicit control.
    ///
    /// # Arguments
    /// * `request` - Open file request parameters
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    ///
    /// # Examples
    /// ```no_run
    /// use code_agent_sdk::{CodeIntelligence, OpenFileRequest};
    /// use std::path::Path;
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let content = std::fs::read_to_string("src/main.rs")?;
    /// client.open_file(OpenFileRequest {
    ///     file_path: Path::new("src/main.rs").to_path_buf(),
    ///     content,
    /// }).await.expect("Operation failed");
    /// 
    /// # }
    /// ```ignore
    pub async fn open_file(&mut self, request: OpenFileRequest) -> Result<()> {
        self.workspace_service
            .open_file(
                &mut self.workspace_manager,
                &request.file_path,
                request.content,
            )
            .await
    }

    /// **Add a language server configuration**
    ///
    /// Registers a new language server that will be used for files matching
    /// the specified extensions. The language server will be initialized when needed.
    ///
    /// # Arguments
    /// * `config` - Language server configuration including command, args, and file extensions
    pub(crate) fn add_language_server(&mut self, config: LanguageServerConfig) {
        self.workspace_manager.add_language_server(config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = CodeIntelligence::builder();
        // Verify it returns a new builder instance
        assert!(builder.workspace_root.is_none());
        assert_eq!(builder.languages.len(), 0);
        assert!(!builder.auto_detect);
    }
}
