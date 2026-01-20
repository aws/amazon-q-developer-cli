use anyhow::Result;

use crate::model::entities::{
    PatternMatch,
    SymbolInfo,
};
use crate::model::types::{
    CodebaseMapResponse,
    CodebaseOverviewResponse,
    DEFAULT_TIMEOUT_SECS,
    FindSymbolsRequest,
    GenerateCodebaseOverviewRequest,
    GetSymbolsRequest,
    PatternSearchRequest,
    SearchCodebaseMapRequest,
};
use crate::sdk::WorkspaceManager;
use crate::tree_sitter::{
    lang_from_extension,
    pattern_search,
    symbol_extractor,
    workspace_analyzer,
};

/// Check if a symbol is a top-level symbol (no container and allowlisted type)
fn is_top_level_symbol(symbol: &SymbolInfo) -> bool {
    symbol.container_name.is_none()
        && symbol.symbol_type.as_deref().is_some_and(|t| {
            matches!(
                t,
                "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Module" | "Namespace" | "Method" | "Function"
            )
        })
}

/// **TreeSitter Symbol Service**
///
/// High-performance symbol analysis service using TreeSitter AST parsing.
/// Provides semantic code understanding with caching, parallel processing,
/// and resource limits for production use.
///
/// ## Core Capabilities
/// - **Symbol Extraction**: Parse symbols from individual files with caching
/// - **Workspace Search**: Find symbols across entire codebase with fuzzy matching
/// - **Pattern Search**: AST-based structural pattern matching using ast-grep
/// - **Codebase Analysis**: Generate architectural overviews and directory maps
///
/// ## Performance Features
/// - Content-hash based caching for parsed symbols
/// - Parallel processing with configurable thread limits
/// - Timeout handling for long-running operations
/// - Memory-efficient streaming for large codebases
///
/// ## Usage Patterns
/// This service complements LSP-based analysis by providing:
/// - Cross-language symbol search when LSP is unavailable
/// - Structural pattern matching beyond LSP capabilities
/// - Codebase-wide architectural analysis
/// - Fallback parsing for unsupported or broken LSP configurations
#[derive(Debug, Clone)]
pub struct TreeSitterSymbolService;

impl TreeSitterSymbolService {
    /// Create a new TreeSitter symbol service instance
    pub fn new() -> Self {
        Self
    }

    // ============================================================================
    // CORE SYMBOL OPERATIONS
    // ============================================================================

    /// **Get all symbols defined in a document**
    ///
    /// Extracts symbols (functions, classes, methods, etc.) from a single source file
    /// using TreeSitter AST parsing. Results are cached for performance.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context and caching
    /// * `file_path` - Path to the source file to analyze
    ///
    /// # Returns
    /// * `Ok(Vec<SymbolInfo>)` - List of symbols found in the file
    /// * `Err` - If file cannot be read or language is unsupported
    ///
    /// # Example
    /// ```ignore
    /// let symbols = service.get_document_symbols(&mut workspace, &path, true).await?;
    /// for symbol in symbols {
    ///     println!("{}: {} at {}:{}", symbol.name, symbol.symbol_type, symbol.start_row, symbol.start_column);
    /// }
    /// ```
    pub async fn get_document_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &std::path::Path,
        top_level_only: bool,
    ) -> Result<Vec<SymbolInfo>> {
        let workspace_root = workspace_manager.workspace_root();
        let code_store = workspace_manager.code_store();

        // Try cache first (cache stores all symbols, filter after)
        let symbols = if let Some(cached) = code_store.get_cached_symbols(file_path) {
            tracing::debug!("Cache HIT for get_document_symbols: {}", file_path.display());
            cached
        } else {
            tracing::debug!("Cache MISS for get_document_symbols: {}", file_path.display());

            // Get extension and detect language
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang_name =
                lang_from_extension(ext).ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {ext}"))?;

            let lang: ast_grep_language::SupportLang = lang_name
                .parse()
                .map_err(|_| anyhow::anyhow!("Failed to parse language: {lang_name}"))?;

            // Parse file and cache
            let parsed = symbol_extractor::parse_file_symbols(file_path, workspace_root, &lang, lang_name);
            code_store.cache_symbols(file_path, parsed.clone());
            parsed
        };

        if top_level_only {
            Ok(symbols.into_iter().filter(is_top_level_symbol).collect())
        } else {
            Ok(symbols)
        }
    }

    /// **Find symbols by name across workspace**
    ///
    /// Performs fuzzy search for symbols across the entire workspace or within a specific file.
    /// Uses parallel processing with resource limits and timeout handling.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context and caching
    /// * `request` - Search parameters including symbol name, filters, and limits
    ///
    /// # Returns
    /// * `Ok(Vec<SymbolInfo>)` - Symbols matching the search criteria, sorted by relevance
    /// * `Err` - If search times out or encounters critical errors
    ///
    /// # Example
    /// ```ignore
    /// let request = FindSymbolsRequest {
    ///     symbol_name: "UserService".to_string(),
    ///     language: Some("typescript".to_string()),
    ///     limit: Some(10),
    ///     ..Default::default()
    /// };
    /// let symbols = service.find_symbols(&mut workspace, &request).await?;
    /// ```
    pub async fn find_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &FindSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>> {
        let timeout_secs = request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);

        // Delegate to workspace_analyzer for the actual search
        workspace_analyzer::find_symbols_with_timeout(workspace_manager, request, timeout_secs).await
    }

    /// **Get symbols by exact names (batch lookup)**
    ///
    /// Efficiently retrieves multiple symbols by exact name match. Supports both
    /// file-specific and workspace-wide lookup with optional source code inclusion.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context and caching
    /// * `request` - Lookup parameters including symbol names and options
    ///
    /// # Returns
    /// * `Ok(Vec<SymbolInfo>)` - Found symbols with optional source code
    /// * `Err` - If lookup encounters critical errors
    ///
    /// # Example
    /// ```ignore
    /// let request = GetSymbolsRequest {
    ///     symbols: vec!["UserService".to_string(), "AuthService".to_string()],
    ///     file_path: Some(path.to_path_buf()),
    ///     include_source: true,
    /// };
    /// let symbols = service.get_symbols(&mut workspace, &request).await?;
    /// ```
    pub async fn get_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &GetSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>> {
        let mut results = Vec::new();

        if let Some(file_path) = &request.file_path {
            // File-specific lookup - need all symbols to find specific ones
            let all_symbols = self.get_document_symbols(workspace_manager, file_path, false).await?;
            for symbol_name in &request.symbols {
                if let Some(symbol) = all_symbols.iter().find(|s| &s.name == symbol_name) {
                    results.push(symbol.clone());
                }
            }
        } else {
            // Workspace-wide lookup - search for each symbol
            for symbol_name in &request.symbols {
                let search_request = FindSymbolsRequest {
                    symbol_name: symbol_name.clone(),
                    file_path: None,
                    symbol_type: None,
                    limit: Some(1),
                    language: None,
                    exact_match: true,
                    timeout_secs: None,
                };

                if let Ok(mut found) = self.find_symbols(workspace_manager, &search_request).await {
                    results.append(&mut found);
                }
            }
        }

        // Populate source_code if requested
        if request.include_source {
            let workspace_root = workspace_manager.workspace_root();
            for symbol in &mut results {
                if symbol.source_code.is_none() {
                    let symbol_path = std::path::Path::new(&symbol.file_path);
                    let full_path = if symbol_path.is_absolute() {
                        symbol_path.to_path_buf()
                    } else {
                        workspace_root.join(symbol_path)
                    };
                    if let Ok(content) = std::fs::read_to_string(&full_path) {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = symbol.start_row.saturating_sub(1) as usize;
                        let end = symbol.end_row as usize;
                        if end <= lines.len() && (end - start) <= 500 {
                            symbol.source_code = Some(lines[start..end].join("\n"));
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    // ============================================================================
    // PATTERN SEARCH OPERATIONS
    // ============================================================================

    /// **Search for AST patterns across workspace**
    ///
    /// Performs structural pattern matching using ast-grep syntax. Supports complex
    /// patterns with variables ($VAR) and wildcards ($$$) for flexible code search.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context and caching
    /// * `request` - Pattern search parameters including pattern, language, and limits
    ///
    /// # Returns
    /// * `Ok(Vec<PatternMatch>)` - Matches found with location and context
    /// * `Err` - If pattern is invalid or search encounters errors
    ///
    /// # Pattern Examples
    /// * `$X.unwrap()` - Find all unwrap calls
    /// * `async fn $NAME($$$)` - Find async function definitions
    /// * `if let Err($E) = $X { $$$ }` - Find error handling patterns
    ///
    /// # Example
    /// ```ignore
    /// let request = PatternSearchRequest {
    ///     pattern: "$X.unwrap()".to_string(),
    ///     language: "rust".to_string(),
    ///     limit: Some(20),
    ///     ..Default::default()
    /// };
    /// let matches = service.pattern_search(&mut workspace, &request).await?;
    /// ```
    pub async fn pattern_search(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &PatternSearchRequest,
    ) -> Result<Vec<PatternMatch>> {
        pattern_search::search_pattern(workspace_manager, request).await
    }

    // ============================================================================
    // WORKSPACE ANALYSIS OPERATIONS
    // ============================================================================

    /// **Generate high-level codebase overview**
    ///
    /// Analyzes workspace structure to provide architectural insights including
    /// primary languages, key directories, entry points, and technology stack.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context
    /// * `request` - Overview generation parameters
    ///
    /// # Returns
    /// * `Ok(CodebaseOverviewResponse)` - Structured overview with metrics and insights
    /// * `Err` - If workspace cannot be analyzed
    ///
    /// # Example
    /// ```ignore
    /// let request = GenerateCodebaseOverviewRequest::default();
    /// let overview = service.generate_codebase_overview(&mut workspace, &request).await?;
    /// println!("Primary language: {}", overview.primary_language);
    /// ```
    pub async fn generate_codebase_overview(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &GenerateCodebaseOverviewRequest,
    ) -> Result<CodebaseOverviewResponse> {
        workspace_analyzer::generate_overview(workspace_manager, request).await
    }

    /// **Search codebase directory structure**
    ///
    /// Provides focused exploration of specific directories with file type analysis
    /// and structure insights for navigation and understanding.
    ///
    /// # Arguments
    /// * `workspace_manager` - Workspace context
    /// * `request` - Directory search parameters
    ///
    /// # Returns
    /// * `Ok(CodebaseMapResponse)` - Directory structure with file analysis
    /// * `Err` - If directory cannot be accessed or analyzed
    ///
    /// # Example
    /// ```ignore
    /// let request = SearchCodebaseMapRequest {
    ///     path: Some("src/components".to_string()),
    ///     ..Default::default()
    /// };
    /// let map = service.search_codebase_map(&mut workspace, &request).await?;
    /// ```
    pub async fn search_codebase_map(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &SearchCodebaseMapRequest,
    ) -> Result<CodebaseMapResponse> {
        workspace_analyzer::search_codebase_map(workspace_manager, request).await
    }
}

impl Default for TreeSitterSymbolService {
    fn default() -> Self {
        Self::new()
    }
}
