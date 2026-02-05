//! Code intelligence tool implementation for the Agent crate.
//!
//! Provides LSP-powered and tree-sitter-based code analysis operations.

use std::borrow::Cow;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use code_agent_sdk::CodeIntelligence;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::RwLock;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::util::providers::SystemProvider;

/// Code intelligence operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Code {
    SearchSymbols(SearchSymbolsParams),
    FindReferences(FindReferencesParams),
    GotoDefinition(GotoDefinitionParams),
    RenameSymbol(RenameSymbolParams),
    Format(FormatCodeParams),
    GetDocumentSymbols(GetDocumentSymbolsParams),
    LookupSymbols(LookupSymbolsParams),
    GetDiagnostics(GetDiagnosticsParams),
    GetHover(GetHoverParams),
    GetCompletions(GetCompletionsParams),
    InitializeWorkspace,
    PatternSearch(PatternSearchParams),
    PatternRewrite(PatternRewriteParams),
    GenerateCodebaseOverview(GenerateCodebaseOverviewParams),
    SearchCodebaseMap(SearchCodebaseMapParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSymbolsParams {
    pub symbol_name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub symbol_type: Option<String>,
    #[serde(default)]
    pub limit: Option<i32>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub exact_match: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindReferencesParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub workspace_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotoDefinitionParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    #[serde(default)]
    pub show_source: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSymbolParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    pub new_name: String,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatCodeParams {
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default = "default_tab_size")]
    pub tab_size: u32,
    #[serde(default = "default_insert_spaces")]
    pub insert_spaces: bool,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDocumentSymbolsParams {
    pub file_path: String,
    #[serde(default)]
    pub top_level_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupSymbolsParams {
    pub symbols: Vec<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub include_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDiagnosticsParams {
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetHoverParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCompletionsParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    #[serde(default)]
    pub trigger_character: Option<String>,
    #[serde(default = "default_completion_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub symbol_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSearchParams {
    pub pattern: String,
    pub language: String,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRewriteParams {
    pub pattern: String,
    pub replacement: String,
    pub language: String,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateCodebaseOverviewParams {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCodebaseMapParams {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
}

fn default_dry_run() -> bool {
    true
}
fn default_completion_limit() -> usize {
    50
}
fn default_tab_size() -> u32 {
    4
}
fn default_insert_spaces() -> bool {
    true
}

fn validate_position(row: i32, column: i32) -> Result<(), String> {
    if row < 1 {
        return Err(format!("row must be >= 1 (got {})", row));
    }
    if column < 1 {
        return Err(format!("column must be >= 1 (got {})", column));
    }
    Ok(())
}

fn validate_file_exists<P: SystemProvider>(provider: &P, file_path: &str) -> Result<(), String> {
    let cwd = provider.cwd().unwrap_or_default();
    let path = if PathBuf::from(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        cwd.join(file_path)
    };
    if !path.exists() {
        return Err(format!("file '{}' does not exist", file_path));
    }
    Ok(())
}

/// Check if operation is a write operation (requires permission)
pub fn is_write_operation(op: &Code) -> bool {
    matches!(op, Code::RenameSymbol(_) | Code::Format(_) | Code::PatternRewrite(_))
}

// Re-export for convenience
pub use super::code_spec::get_code_tool_spec;
use super::code_spec::{
    CODE_TOOL_DESCRIPTION,
    CODE_TOOL_SCHEMA,
};

impl BuiltInToolTrait for Code {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Code
    }

    fn description() -> Cow<'static, str> {
        CODE_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        CODE_TOOL_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["code"])
    }
}

impl Code {
    /// Check if this is a write operation (requires permission)
    pub fn is_write_operation(&self) -> bool {
        matches!(self, Code::RenameSymbol(_) | Code::Format(_) | Code::PatternRewrite(_))
    }

    /// Validate code operation parameters
    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        match self {
            Code::SearchSymbols(p) => {
                if p.symbol_name.trim().is_empty() {
                    return Err("symbol_name cannot be empty".to_string());
                }
                Ok(())
            },
            Code::FindReferences(p) => {
                validate_position(p.row, p.column)?;
                validate_file_exists(provider, &p.file_path)
            },
            Code::GotoDefinition(p) => {
                validate_position(p.row, p.column)?;
                validate_file_exists(provider, &p.file_path)
            },
            Code::GetHover(p) => {
                validate_position(p.row, p.column)?;
                validate_file_exists(provider, &p.file_path)
            },
            Code::RenameSymbol(p) => {
                validate_position(p.row, p.column)?;
                if p.new_name.trim().is_empty() {
                    return Err("new_name cannot be empty".to_string());
                }
                validate_file_exists(provider, &p.file_path)
            },
            Code::GetDocumentSymbols(p) => validate_file_exists(provider, &p.file_path),
            Code::GetDiagnostics(p) => validate_file_exists(provider, &p.file_path),
            Code::GetCompletions(p) => {
                validate_position(p.row, p.column)?;
                validate_file_exists(provider, &p.file_path)
            },
            Code::LookupSymbols(p) => {
                if p.symbols.is_empty() {
                    return Err("symbols list cannot be empty".to_string());
                }
                Ok(())
            },
            Code::PatternSearch(p) => {
                if p.pattern.trim().is_empty() {
                    return Err("pattern cannot be empty".to_string());
                }
                if p.language.trim().is_empty() {
                    return Err("language must be specified".to_string());
                }
                Ok(())
            },
            Code::PatternRewrite(p) => {
                if p.pattern.trim().is_empty() {
                    return Err("pattern cannot be empty".to_string());
                }
                if p.language.trim().is_empty() {
                    return Err("language must be specified".to_string());
                }
                if p.replacement.trim().is_empty() {
                    return Err("replacement cannot be empty".to_string());
                }
                Ok(())
            },
            Code::Format(p) => {
                if p.tab_size < 1 {
                    return Err(format!("tab_size must be >= 1 (got {})", p.tab_size));
                }
                Ok(())
            },
            Code::InitializeWorkspace | Code::GenerateCodebaseOverview(_) | Code::SearchCodebaseMap(_) => Ok(()),
        }
    }

    /// Execute the code intelligence operation
    pub async fn execute(
        &self,
        code_intelligence: &Arc<RwLock<CodeIntelligence>>,
        sys: &dyn SystemProvider,
    ) -> ToolExecutionResult {
        let cwd = sys.cwd().unwrap_or_default();
        let mut client = code_intelligence.write().await;

        match self {
            Code::InitializeWorkspace => {
                if let Err(e) = client.initialize().await {
                    return Err(ToolExecutionError::Custom(format!("Failed to initialize: {e}")));
                }
                Ok(text_output("Workspace initialized successfully"))
            },

            Code::SearchSymbols(params) => {
                let symbol_type = params
                    .symbol_type
                    .as_ref()
                    .and_then(|s| s.parse::<code_agent_sdk::model::types::ApiSymbolKind>().ok());
                let request = code_agent_sdk::FindSymbolsRequest {
                    symbol_name: params.symbol_name.clone(),
                    file_path: params.path.as_ref().map(|p| resolve_path(&cwd, p)),
                    symbol_type,
                    limit: params.limit.map(|l| l as u32),
                    exact_match: params.exact_match.unwrap_or(false),
                    language: params.language.clone(),
                    timeout_secs: None,
                };
                match client.find_symbols(request).await {
                    Ok(symbols) => Ok(format_symbols(&symbols)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Search failed: {e}"))),
                }
            },

            Code::LookupSymbols(params) => {
                let request = code_agent_sdk::GetSymbolsRequest {
                    symbols: params.symbols.clone(),
                    file_path: params.file_path.as_ref().map(|p| resolve_path(&cwd, p)),
                    include_source: params.include_source,
                    start_row: None,
                    start_column: None,
                };
                match client.get_symbols(request).await {
                    Ok(symbols) => Ok(format_symbols(&symbols)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Lookup failed: {e}"))),
                }
            },

            Code::FindReferences(params) => {
                let request = code_agent_sdk::FindReferencesByLocationRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    row: params.row as u32,
                    column: params.column as u32,
                    limit: params.limit.map(|l| l as u32),
                    offset: None,
                    workspace_only: Some(params.workspace_only.unwrap_or(true)),
                };
                match client.find_references_by_location(request).await {
                    Ok(result) => Ok(format_references(&result, params.limit)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Find references failed: {e}"))),
                }
            },

            Code::GotoDefinition(params) => {
                let request = code_agent_sdk::GotoDefinitionRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    row: params.row as u32,
                    column: params.column as u32,
                    show_source: params.show_source.unwrap_or(true),
                };
                match client.goto_definition(request).await {
                    Ok(Some(def)) => Ok(format_definition(&def)),
                    Ok(None) => Ok(text_output("No definition found")),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Goto definition failed: {e}"))),
                }
            },

            Code::GetDocumentSymbols(params) => {
                let request = code_agent_sdk::GetDocumentSymbolsRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    top_level_only: params.top_level_only,
                };
                match client.get_document_symbols(request).await {
                    Ok(symbols) => Ok(format_symbols(&symbols)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Get document symbols failed: {e}"))),
                }
            },

            Code::GetDiagnostics(params) => {
                let request = code_agent_sdk::GetDocumentDiagnosticsRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    identifier: None,
                    previous_result_id: None,
                };
                match client.get_document_diagnostics(request).await {
                    Ok(diagnostics) => Ok(format_diagnostics(&diagnostics)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Get diagnostics failed: {e}"))),
                }
            },

            Code::GetHover(params) => {
                let request = code_agent_sdk::model::types::HoverRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    row: params.row as u32,
                    column: params.column as u32,
                };
                match client.hover(request).await {
                    Ok(Some(hover)) => Ok(format_hover(&hover)),
                    Ok(None) => Ok(text_output("No hover information available")),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Get hover failed: {e}"))),
                }
            },

            Code::GetCompletions(params) => {
                let request = code_agent_sdk::model::types::CompletionRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    row: params.row as u32,
                    column: params.column as u32,
                    trigger_character: params.trigger_character.clone(),
                    filter: params.filter.clone(),
                    symbol_type: params.symbol_type.as_ref().and_then(|s| s.parse().ok()),
                    limit: Some(params.limit),
                    offset: None,
                };
                match client.completion(request).await {
                    Ok(Some(completions)) => {
                        Ok(format_completions(&completions, params.limit, params.filter.as_deref()))
                    },
                    Ok(None) => Ok(text_output("No completions available")),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Get completions failed: {e}"))),
                }
            },

            Code::RenameSymbol(params) => {
                let request = code_agent_sdk::RenameSymbolRequest {
                    file_path: resolve_path(&cwd, &params.file_path),
                    row: params.row as u32,
                    column: params.column as u32,
                    new_name: params.new_name.clone(),
                    dry_run: params.dry_run,
                };
                match client.rename_symbol(request).await {
                    Ok(Some(result)) => Ok(format_rename_result(&result, params.dry_run)),
                    Ok(None) => Ok(text_output("Cannot rename at this location")),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Rename failed: {e}"))),
                }
            },

            Code::Format(params) => {
                let request = code_agent_sdk::FormatCodeRequest {
                    file_path: params.file_path.as_ref().map(|p| resolve_path(&cwd, p)),
                    tab_size: params.tab_size,
                    insert_spaces: params.insert_spaces,
                };
                match client.format_code(request).await {
                    Ok(count) => {
                        let msg = if params.dry_run {
                            format!("Would apply {count} formatting edits")
                        } else {
                            format!("Applied {count} formatting edits")
                        };
                        Ok(text_output(msg))
                    },
                    Err(e) => Err(ToolExecutionError::Custom(format!("Format failed: {e}"))),
                }
            },

            Code::PatternSearch(params) => {
                let request = code_agent_sdk::PatternSearchRequest {
                    pattern: params.pattern.clone(),
                    language: params.language.clone(),
                    file_path: params
                        .file_path
                        .as_ref()
                        .map(|p| resolve_path(&cwd, p).to_string_lossy().to_string()),
                    limit: params.limit,
                    offset: params.offset,
                };
                match client.pattern_search(request).await {
                    Ok(matches) => Ok(format_pattern_matches(&matches)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Pattern search failed: {e}"))),
                }
            },

            Code::PatternRewrite(params) => {
                let request = code_agent_sdk::PatternRewriteRequest {
                    pattern: params.pattern.clone(),
                    replacement: params.replacement.clone(),
                    language: params.language.clone(),
                    file_path: params
                        .file_path
                        .as_ref()
                        .map(|p| resolve_path(&cwd, p).to_string_lossy().to_string()),
                    dry_run: params.dry_run,
                    limit: params.limit,
                };
                match client.pattern_rewrite(request).await {
                    Ok(result) => Ok(format_rewrite_result(&result, params.dry_run)),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Pattern rewrite failed: {e}"))),
                }
            },

            Code::GenerateCodebaseOverview(params) => {
                let request = code_agent_sdk::model::types::GenerateCodebaseOverviewRequest {
                    path: params
                        .path
                        .as_ref()
                        .map(|p| resolve_path(&cwd, p).to_string_lossy().to_string()),
                    timeout_secs: None,
                    token_budget: None,
                };
                match client.generate_codebase_overview(request).await {
                    Ok(overview) => Ok(text_output(serde_json::to_string(&overview).unwrap_or_default())),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Generate overview failed: {e}"))),
                }
            },

            Code::SearchCodebaseMap(params) => {
                let request = code_agent_sdk::model::types::SearchCodebaseMapRequest {
                    path: params.path.clone(),
                    file_path: params.file_path.clone(),
                    timeout_secs: None,
                    token_budget: None,
                };
                match client.search_codebase_map(request).await {
                    Ok(map) => Ok(text_output(serde_json::to_string(&map).unwrap_or_default())),
                    Err(e) => Err(ToolExecutionError::Custom(format!("Search codebase map failed: {e}"))),
                }
            },
        }
    }
}

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() { p } else { cwd.join(p) }
}

fn text_output(s: impl Into<String>) -> ToolExecutionOutput {
    ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(s.into())])
}

fn format_symbols(symbols: &[code_agent_sdk::SymbolInfo]) -> ToolExecutionOutput {
    if symbols.is_empty() {
        return text_output("No symbols found");
    }
    let mut output = String::new();
    for s in symbols {
        output.push_str(&format!(
            "[{} {} @ {}:{}-{} | {}]\n",
            s.symbol_type.as_deref().unwrap_or("Symbol"),
            s.name,
            s.file_path,
            s.start_row,
            s.end_row,
            s.detail.as_deref().unwrap_or(&s.name)
        ));
        if let Some(source) = &s.source_code {
            output.push_str(source);
            output.push('\n');
        }
    }
    text_output(output)
}

fn format_references(result: &code_agent_sdk::ApiReferencesResult, limit: Option<usize>) -> ToolExecutionOutput {
    let refs = &result.references;
    if refs.is_empty() {
        return text_output("No references found");
    }
    let limit = limit.unwrap_or(100);
    let mut output = format!("Found {} references:\n", refs.len());
    for r in refs.iter().take(limit) {
        output.push_str(&format!("  {}:{}:{}\n", r.file_path, r.start_row, r.start_column));
    }
    if refs.len() > limit {
        output.push_str(&format!("  ... and {} more\n", refs.len() - limit));
    }
    text_output(output)
}

fn format_definition(def: &code_agent_sdk::DefinitionInfo) -> ToolExecutionOutput {
    let mut output = format!(
        "Definition at {}:{}:{}\n",
        def.file_path, def.start_row, def.start_column
    );
    if let Some(source) = &def.source_line {
        output.push_str(&format!("Source:\n{}\n", source));
    }
    text_output(output)
}

fn format_diagnostics(diagnostics: &[code_agent_sdk::ApiDiagnosticInfo]) -> ToolExecutionOutput {
    if diagnostics.is_empty() {
        return text_output("No diagnostics");
    }
    let mut output = String::new();
    for d in diagnostics {
        output.push_str(&format!(
            "[{:?}] {}:{}: {}\n",
            d.severity, d.start_row, d.start_column, d.message
        ));
    }
    text_output(output)
}

fn format_hover(hover: &code_agent_sdk::model::entities::HoverInfo) -> ToolExecutionOutput {
    let mut output = String::new();
    if let Some(content) = &hover.content {
        output.push_str(content);
        output.push('\n');
    }
    text_output(output)
}

fn format_completions(
    info: &code_agent_sdk::model::entities::CompletionInfo,
    limit: usize,
    filter: Option<&str>,
) -> ToolExecutionOutput {
    let items: Vec<_> = info
        .items
        .iter()
        .filter(|item| filter.is_none_or(|f| item.label.contains(f)))
        .take(limit)
        .collect();

    if items.is_empty() {
        return text_output("No completions");
    }

    let mut output = String::new();
    for item in items {
        output.push_str(&format!("- {}", item.label));
        if let Some(detail) = &item.detail {
            output.push_str(&format!(" ({})", detail));
        }
        output.push('\n');
    }
    text_output(output)
}

fn format_rename_result(result: &code_agent_sdk::model::entities::RenameResult, dry_run: bool) -> ToolExecutionOutput {
    let prefix = if dry_run { "Would rename" } else { "Renamed" };
    text_output(format!(
        "{} {} edits across {} files",
        prefix, result.edit_count, result.file_count
    ))
}

fn format_pattern_matches(matches: &[code_agent_sdk::PatternMatch]) -> ToolExecutionOutput {
    if matches.is_empty() {
        return text_output("No matches found");
    }
    let mut output = format!("Found {} matches:\n", matches.len());
    for m in matches {
        output.push_str(&format!(
            "  {}:{}:{} - {}\n",
            m.file_path,
            m.start_row,
            m.start_column,
            m.matched_code.lines().next().unwrap_or("")
        ));
    }
    text_output(output)
}

fn format_rewrite_result(result: &code_agent_sdk::RewriteResult, dry_run: bool) -> ToolExecutionOutput {
    let prefix = if dry_run { "Would modify" } else { "Modified" };
    text_output(format!(
        "{} {} files with {} replacements",
        prefix, result.files_modified, result.replacements
    ))
}
