use std::path::PathBuf;

use anyhow::Result;

use crate::model::entities::{
    DefinitionInfo,
    ReferencesResult,
    SymbolInfo,
};
use crate::model::types::*;
use crate::sdk::CodeIntelligenceBuilder;
use crate::sdk::services::tree_sitter_coding_service::TreeSitterCodingService;
use crate::sdk::services::tree_sitter_symbol_service::TreeSitterSymbolService;
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
/// use std::path::PathBuf;
///
/// use code_agent_sdk::{
///     CodeIntelligence,
///     FindSymbolsRequest,
/// };
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
/// let symbols = client
///     .find_symbols(FindSymbolsRequest {
///         symbol_name: "function_name".to_string(),
///         file_path: None,
///         symbol_type: None,
///         limit: Some(10),
///         exact_match: false,
///     })
///     .await
///     .expect("Failed to find symbols");
/// # }
/// # }
/// ```ignore
use crate::sdk::services::*;
use crate::sdk::workspace_manager::WorkspaceManager;

pub struct CodeIntelligence {
    lsp_symbol_service: LspSymbolService,
    tree_sitter_symbol_service: TreeSitterSymbolService,
    lsp_coding_service: LspCodingService,
    tree_sitter_coding_service: TreeSitterCodingService,
    lsp_workspace_service: LspWorkspaceService,
    pub workspace_manager: WorkspaceManager,
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
    /// use std::path::PathBuf;
    ///
    /// use code_agent_sdk::CodeIntelligence;
    ///
    /// let client = CodeIntelligence::new(PathBuf::from("/path/to/workspace"));
    /// ```ignore
    pub fn new(workspace_root: PathBuf) -> Self {
        let workspace_manager = WorkspaceManager::new(workspace_root);

        let lsp_workspace_service = LspWorkspaceService::new();
        let lsp_symbol_service = LspSymbolService::new(Box::new(LspWorkspaceService::new()));
        let tree_sitter_symbol_service = TreeSitterSymbolService::new();
        let lsp_coding_service = LspCodingService::new(Box::new(LspWorkspaceService::new()));
        let tree_sitter_coding_service = TreeSitterCodingService::new();

        Self {
            lsp_symbol_service,
            tree_sitter_symbol_service,
            lsp_coding_service,
            tree_sitter_coding_service,
            lsp_workspace_service,
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
    /// use std::path::PathBuf;
    ///
    /// use code_agent_sdk::CodeIntelligence;
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
    /// use std::path::PathBuf;
    ///
    /// use code_agent_sdk::CodeIntelligence;
    ///
    /// let mut client = CodeIntelligence::new(PathBuf::from("."));
    /// let workspace_info = client
    ///     .detect_workspace()
    ///     .expect("Failed to detect workspace");
    /// println!(
    ///     "Detected languages: {:?}",
    ///     workspace_info.detected_languages
    /// );
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
    /// use std::path::PathBuf;
    ///
    /// use code_agent_sdk::CodeIntelligence;
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

    /// **Check if language servers are initialized**
    ///
    /// # Returns
    /// * `bool` - True if workspace has been initialized
    pub fn is_initialized(&self) -> bool {
        self.workspace_manager.is_initialized()
    }

    /// **Get current workspace initialization status**
    ///
    /// # Returns
    /// * `WorkspaceStatus` - Current status (NotInitialized, Initializing, Initialized)
    pub fn workspace_status(&self) -> crate::sdk::WorkspaceStatus {
        self.workspace_manager.workspace_status()
    }

    /// **Get mutable reference to workspace manager**
    ///
    /// Used by TreeSitter services for pattern search/rewrite operations.
    pub fn workspace_manager_mut(&mut self) -> &mut crate::sdk::WorkspaceManager {
        &mut self.workspace_manager
    }

    /// **Check if code intelligence has been initialized**
    ///
    /// # Returns
    /// * `bool` - True if workspace has been initialized
    pub fn should_auto_initialize(&self) -> bool {
        self.workspace_manager.config_exists()
    }

    /// **Check if code intelligence has been initialized**
    ///
    /// Returns true if lsp.json exists in .kiro/settings, indicating code intelligence
    /// was previously initialized and should remain active.
    pub fn is_code_intelligence_initialized(&self) -> bool {
        self.workspace_manager.is_code_intelligence_initialized()
    }

    /// **Reset initialization state to allow re-initialization**
    pub async fn reset_initialization(&mut self) {
        self.workspace_manager.reset_initialization().await;
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     FindSymbolsRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client
    ///     .find_symbols(FindSymbolsRequest {
    ///         symbol_name: "function_name".to_string(),
    ///         file_path: Some(Path::new("src/main.rs").to_path_buf()),
    ///         symbol_type: None,
    ///         limit: Some(10),
    ///         exact_match: false,
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// # }
    /// ```ignore
    ///
    /// Filter languages based on initialized LSPs and request constraints
    fn filter_languages(&self, initialized_languages: &[String], language_filter: &Option<String>) -> Vec<String> {
        if let Some(lang) = language_filter {
            // If language filter specified, only include it if initialized
            if initialized_languages.iter().any(|l| l.eq_ignore_ascii_case(lang)) {
                vec![lang.clone()]
            } else {
                vec![]
            }
        } else {
            // No filter, use all initialized languages
            initialized_languages.to_vec()
        }
    }

    pub async fn find_symbols(&mut self, request: FindSymbolsRequest) -> Result<Vec<SymbolInfo>> {
        let initialized_lsp_languages = self.workspace_manager.get_initialized_lsp_languages();
        let lsp_languages = self.filter_languages(&initialized_lsp_languages, &request.language);

        let mut all_symbols = Vec::new();
        let mut last_error: Option<anyhow::Error> = None;

        // Try LSP if available for the language
        if !lsp_languages.is_empty() {
            tracing::debug!("Using LSP search for languages: {:?}", lsp_languages);
            match self
                .lsp_symbol_service
                .find_symbols(&mut self.workspace_manager, request.clone())
                .await
            {
                Ok(symbols) => {
                    tracing::debug!("LSP search returned {} symbols", symbols.len());
                    all_symbols.extend(symbols);
                },
                Err(e) => {
                    tracing::warn!("LSP search failed: {}", e);
                    last_error = Some(e);
                },
            }
        }

        // Always try tree-sitter (it filters by language internally)
        tracing::debug!("Using tree-sitter for symbol search");
        match self
            .tree_sitter_symbol_service
            .find_symbols(&mut self.workspace_manager, &request)
            .await
        {
            Ok(symbols) => {
                tracing::debug!("TreeSitter search returned {} symbols", symbols.len());
                all_symbols.extend(symbols);
            },
            Err(e) => {
                tracing::warn!("TreeSitter search failed: {}", e);
                if last_error.is_none() {
                    last_error = Some(e);
                }
            },
        }

        if all_symbols.is_empty()
            && let Some(e) = last_error
        {
            return Err(e);
        }

        Self::deduplicate_and_score_symbols(all_symbols, &request.symbol_name, request.limit.map(|l| l as usize))
    }

    /// Deduplicate and score symbols with case-sensitive boosting
    fn deduplicate_and_score_symbols(
        symbols: Vec<SymbolInfo>,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SymbolInfo>> {
        use std::collections::HashMap;

        // Deduplicate by (file_path, start_row, start_column, name)
        let mut unique: HashMap<(String, u32, u32, String), SymbolInfo> = HashMap::new();
        for symbol in symbols {
            let key = (
                symbol.file_path.clone(),
                symbol.start_row,
                symbol.start_column,
                symbol.name.clone(),
            );
            unique.entry(key).or_insert(symbol);
        }

        // Score all symbols uniformly
        let query_lower = query.to_lowercase();
        let mut scored: Vec<(f64, SymbolInfo)> = unique
            .into_values()
            .map(|symbol| {
                let score = crate::utils::scoring::calculate_fuzzy_score(
                    &query_lower,
                    &symbol.name.to_lowercase(),
                    query,
                    &symbol.name,
                );
                (score, symbol)
            })
            .collect();

        // Sort by score (higher first)
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(_, s)| s)
            .collect())
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
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     GetSymbolsRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client
    ///     .get_symbols(GetSymbolsRequest {
    ///         symbols: vec!["main".to_string(), "init".to_string()],
    ///         file_path: None,
    ///         include_source: false,
    ///         row: None,
    ///         column: None,
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    /// # }
    /// ```ignore
    pub async fn get_symbols(&mut self, request: GetSymbolsRequest) -> Result<Vec<SymbolInfo>> {
        // Try TreeSitter first (fast), fallback to LSP on error or unsupported language
        match self
            .tree_sitter_symbol_service
            .get_symbols(&mut self.workspace_manager, &request)
            .await
        {
            Ok(symbols) if !symbols.is_empty() => Ok(symbols),
            _ => {
                // Fallback to LSP
                self.lsp_symbol_service
                    .get_symbols(&mut self.workspace_manager, request)
                    .await
            },
        }
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     GetDocumentSymbolsRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let symbols = client
    ///     .get_document_symbols(GetDocumentSymbolsRequest {
    ///         file_path: Path::new("src/main.rs").to_path_buf(),
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    /// for symbol in symbols {
    ///     println!(
    ///         "{} {} at line {}",
    ///         symbol.symbol_type.as_deref().unwrap_or("Unknown"),
    ///         symbol.name,
    ///         symbol.start_row
    ///     );
    /// }
    ///
    /// # }
    /// ```ignore
    pub async fn get_document_symbols(&mut self, request: GetDocumentSymbolsRequest) -> Result<Vec<SymbolInfo>> {
        let top_level_only = request.top_level_only.unwrap_or(true);

        let ext = request.file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let has_lsp = self.workspace_manager.has_initialized_lsp_for_extension(ext);

        if has_lsp {
            let result = self
                .lsp_symbol_service
                .get_document_symbols(&mut self.workspace_manager, &request.file_path, top_level_only)
                .await?;
            // Fallback to tree-sitter if LSP returns empty
            if result.is_empty() {
                return self
                    .tree_sitter_symbol_service
                    .get_document_symbols(&mut self.workspace_manager, &request.file_path, top_level_only)
                    .await;
            }
            Ok(result)
        } else if crate::tree_sitter::lang_from_extension(ext).is_some() {
            self.tree_sitter_symbol_service
                .get_document_symbols(&mut self.workspace_manager, &request.file_path, top_level_only)
                .await
        } else {
            Err(anyhow::anyhow!("Unsupported file extension: {}", ext))
        }
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     GotoDefinitionRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// if let Some(definition) = client
    ///     .goto_definition(GotoDefinitionRequest {
    ///         file_path: Path::new("src/main.rs").to_path_buf(),
    ///         row: 10,           // line 10
    ///         column: 5,         // column 5
    ///         show_source: true, // include source
    ///     })
    ///     .await?
    /// {
    ///     println!(
    ///         "Definition found at {}:{}",
    ///         definition.start_row, definition.start_column
    ///     );
    /// }
    ///
    /// # }
    /// ```ignore
    pub async fn goto_definition(&mut self, request: GotoDefinitionRequest) -> Result<Option<DefinitionInfo>> {
        self.lsp_symbol_service
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     FindReferencesByLocationRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let references = client
    ///     .find_references_by_location(FindReferencesByLocationRequest {
    ///         file_path: Path::new("src/main.rs").to_path_buf(),
    ///         row: 10,   // 0-based line number
    ///         column: 5, // 0-based column number
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// for reference in references {
    ///     println!(
    ///         "Reference in {} at {}:{}",
    ///         reference.file_path, reference.start_row, reference.start_column
    ///     );
    /// }
    ///
    /// # }
    /// ```ignore
    pub async fn find_references_by_location(
        &mut self,
        request: FindReferencesByLocationRequest,
    ) -> Result<ReferencesResult> {
        self.lsp_symbol_service
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
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     FindReferencesByNameRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let references = client
    ///     .find_references_by_name(FindReferencesByNameRequest {
    ///         symbol_name: "myFunction".to_string(),
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// for reference in references {
    ///     println!(
    ///         "Reference at {}:{}",
    ///         reference.start_row, reference.start_column
    ///     );
    /// }
    ///
    /// # }
    /// ```ignore
    pub async fn find_references_by_name(&mut self, request: FindReferencesByNameRequest) -> Result<ReferencesResult> {
        self.lsp_symbol_service
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     RenameSymbolRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let workspace_edit = client
    ///     .rename_symbol(RenameSymbolRequest {
    ///         file_path: Path::new("src/main.rs").to_path_buf(),
    ///         row: 10,
    ///         column: 5,
    ///         new_name: "newFunctionName".to_string(),
    ///         dry_run: true, // Preview changes without applying
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// if let Some(edit) = workspace_edit {
    ///     println!(
    ///         "Rename would affect {} files",
    ///         edit.changes.as_ref().map(|c| c.len()).unwrap_or(0)
    ///     );
    /// }
    ///
    /// # }
    /// ```ignore
    pub async fn rename_symbol(
        &mut self,
        request: RenameSymbolRequest,
    ) -> Result<Option<crate::model::entities::RenameResult>> {
        let lsp_edit = self
            .lsp_coding_service
            .rename_symbol(&mut self.workspace_manager, request)
            .await?;

        Ok(lsp_edit.map(|edit| crate::model::entities::RenameResult::from_lsp_workspace_edit(&edit)))
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     FormatCodeRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let edits = client
    ///     .format_code(FormatCodeRequest {
    ///         file_path: Some(Path::new("src/main.ts").to_path_buf()),
    ///         tab_size: 2,
    ///         insert_spaces: true,
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// println!("Applied {} formatting edits", edits.len());
    ///
    /// # }
    /// ```ignore
    pub async fn format_code(&mut self, request: FormatCodeRequest) -> Result<usize> {
        self.lsp_coding_service
            .format_code(&mut self.workspace_manager, request)
            .await
    }

    /// **Rewrite code patterns using AST matching**
    ///
    /// Performs structural pattern replacement across files using TreeSitter AST parsing.
    /// Supports complex patterns with variables and wildcards for automated refactoring.
    ///
    /// # Arguments
    /// * `request` - Pattern rewrite parameters including pattern, replacement, and options
    ///
    /// # Returns
    /// * `Ok(RewriteResult)` - Summary of files modified and replacements made
    /// * `Err` - If pattern is invalid or rewrite encounters errors
    ///
    /// # Example
    /// ```ignore
    /// use code_agent_sdk::{CodeIntelligence, PatternRewriteRequest};
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = CodeIntelligence::new(PathBuf::from("."));
    ///
    /// let result = client.pattern_rewrite(PatternRewriteRequest {
    ///     pattern: "$X.unwrap()".to_string(),
    ///     replacement: "$X.expect(\"error\")".to_string(),
    ///     language: "rust".to_string(),
    ///     dry_run: true,
    ///     ..Default::default()
    /// }).await?;
    ///
    /// println!("Would modify {} files with {} replacements",
    ///          result.files_modified, result.replacements);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pattern_rewrite(
        &mut self,
        request: crate::model::types::PatternRewriteRequest,
    ) -> Result<crate::model::entities::RewriteResult> {
        self.tree_sitter_coding_service
            .pattern_rewrite(&mut self.workspace_manager, request)
            .await
    }

    /// **Search for AST patterns across workspace**
    ///
    /// Performs structural pattern matching using ast-grep syntax for finding code patterns.
    ///
    /// # Arguments
    /// * `request` - Pattern search parameters including pattern, language, and limits
    ///
    /// # Returns
    /// * `Ok(Vec<PatternMatch>)` - Matches found with location and context
    /// * `Err` - If pattern is invalid or search encounters errors
    ///
    /// # Example
    /// ```ignore
    /// let matches = client.pattern_search(PatternSearchRequest {
    ///     pattern: "$X.unwrap()".to_string(),
    ///     language: "rust".to_string(),
    ///     limit: Some(20),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    pub async fn pattern_search(
        &mut self,
        request: crate::model::types::PatternSearchRequest,
    ) -> Result<Vec<crate::model::entities::PatternMatch>> {
        self.tree_sitter_symbol_service
            .pattern_search(&mut self.workspace_manager, &request)
            .await
    }

    /// **Generate high-level codebase overview**
    ///
    /// Analyzes workspace structure to provide architectural insights including
    /// primary languages, key directories, entry points, and technology stack.
    ///
    /// # Arguments
    /// * `request` - Overview generation parameters
    ///
    /// # Returns
    /// * `Ok(CodebaseOverviewResponse)` - Structured overview with metrics and insights
    /// * `Err` - If workspace cannot be analyzed
    ///
    /// # Example
    /// ```ignore
    /// let overview = client.generate_codebase_overview(GenerateCodebaseOverviewRequest {
    ///     path: None,
    ///     timeout_secs: None,
    ///     token_budget: None,
    /// }).await?;
    /// ```
    pub async fn generate_codebase_overview(
        &mut self,
        request: crate::model::types::GenerateCodebaseOverviewRequest,
    ) -> Result<crate::model::types::CodebaseOverviewResponse> {
        self.tree_sitter_symbol_service
            .generate_codebase_overview(&mut self.workspace_manager, &request)
            .await
    }

    /// **Search codebase directory structure**
    ///
    /// Provides focused exploration of specific directories with file type analysis
    /// and structure insights for navigation and understanding.
    ///
    /// # Arguments
    /// * `request` - Directory search parameters
    ///
    /// # Returns
    /// * `Ok(CodebaseMapResponse)` - Directory structure with file analysis
    /// * `Err` - If directory cannot be accessed or analyzed
    ///
    /// # Example
    /// ```ignore
    /// let map = client.search_codebase_map(SearchCodebaseMapRequest {
    ///     path: Some("src/components".to_string()),
    ///     file_path: None,
    /// }).await?;
    /// ```
    pub async fn search_codebase_map(
        &mut self,
        request: crate::model::types::SearchCodebaseMapRequest,
    ) -> Result<crate::model::types::CodebaseMapResponse> {
        self.tree_sitter_symbol_service
            .search_codebase_map(&mut self.workspace_manager, &request)
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
    /// use std::path::Path;
    ///
    /// use code_agent_sdk::{
    ///     CodeIntelligence,
    ///     OpenFileRequest,
    /// };
    ///
    /// # async fn example() {
    /// let mut client = CodeIntelligence::new(std::env::current_dir().unwrap());
    /// client.initialize().await.expect("Operation failed");
    ///
    /// let content = std::fs::read_to_string("src/main.rs")?;
    /// client
    ///     .open_file(OpenFileRequest {
    ///         file_path: Path::new("src/main.rs").to_path_buf(),
    ///         content,
    ///     })
    ///     .await
    ///     .expect("Operation failed");
    ///
    /// # }
    /// ```ignore
    pub async fn open_file(&mut self, request: OpenFileRequest) -> Result<()> {
        self.lsp_workspace_service
            .open_file(&mut self.workspace_manager, &request.file_path, request.content)
            .await
    }

    /// Get diagnostics for a document using the pull model.
    ///
    /// This method requests diagnostics for a specific document, giving the client
    /// control over when diagnostics are computed. This is useful for prioritizing
    /// diagnostics for files currently being edited or viewed.
    ///
    /// # Arguments
    /// * `request` - Document path and optional parameters for diagnostic retrieval
    ///
    /// # Returns
    /// * `Result<Vec<DiagnosticInfo>>` - List of diagnostics or empty if none
    ///
    /// # Example
    /// ```ignore
    /// let request = GetDocumentDiagnosticsRequest {
    ///     file_path: PathBuf::from("src/main.rs"),
    ///     identifier: None,
    ///     previous_result_id: None,
    /// };
    /// let diagnostics = code_intel.get_document_diagnostics(request).await?;
    /// for diagnostic in diagnostics {
    ///     println!("{}:{} - {}", diagnostic.start_row, diagnostic.start_column, diagnostic.message);
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// # }
    /// ```ignore
    pub async fn get_document_diagnostics(
        &mut self,
        request: GetDocumentDiagnosticsRequest,
    ) -> Result<Vec<crate::model::entities::DiagnosticInfo>> {
        let lsp_diagnostics = self
            .lsp_symbol_service
            .get_document_diagnostics(&mut self.workspace_manager, request)
            .await?;

        let workspace_root = self.workspace_manager.workspace_root();
        Ok(lsp_diagnostics
            .iter()
            .map(|d| crate::model::entities::DiagnosticInfo::from_lsp_diagnostic(d, workspace_root))
            .collect())
    }

    /// **Get hover information at a specific location**
    ///
    /// Retrieves hover information (type info, documentation) for the symbol
    /// at the specified position in a file.
    ///
    /// # Arguments
    /// * `request` - Hover request parameters including file path and position
    ///
    /// # Returns
    /// * `Result<Option<HoverInfo>>` - Hover information, or None if not available
    pub async fn hover(
        &mut self,
        request: crate::model::types::HoverRequest,
    ) -> Result<Option<crate::model::entities::HoverInfo>> {
        self.lsp_symbol_service
            .hover(&mut self.workspace_manager, request)
            .await
    }

    /// **Get code completion suggestions at a specific location**
    ///
    /// Retrieves code completion suggestions for the specified position in a file.
    ///
    /// # Arguments
    /// * `request` - Completion request parameters including file path and position
    ///
    /// # Returns
    /// * `Result<Option<CompletionInfo>>` - Completion suggestions, or None if not available
    pub async fn completion(
        &mut self,
        request: crate::model::types::CompletionRequest,
    ) -> Result<Option<crate::model::entities::CompletionInfo>> {
        self.lsp_symbol_service
            .completion(&mut self.workspace_manager, request)
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
