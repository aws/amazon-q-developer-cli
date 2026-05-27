//! Code intelligence tool implementation for the Agent crate.
//!
//! Provides LSP-powered and tree-sitter-based code analysis operations.

use std::borrow::Cow;
use std::path::PathBuf;
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

    /// Return the filesystem paths this operation will read from.
    ///
    /// When empty, the operation applies to the workspace/CWD (which is auto-allowed
    /// by the default CWD read permission). When non-empty, each path must pass the
    /// fs_read permission check before the operation is allowed.
    pub fn read_paths(&self) -> Vec<String> {
        match self {
            Code::SearchSymbols(p) => p.path.iter().cloned().collect(),
            Code::FindReferences(p) => vec![p.file_path.clone()],
            Code::GotoDefinition(p) => vec![p.file_path.clone()],
            Code::GetDocumentSymbols(p) => vec![p.file_path.clone()],
            Code::LookupSymbols(p) => p.file_path.iter().cloned().collect(),
            Code::GetDiagnostics(p) => vec![p.file_path.clone()],
            Code::GetHover(p) => vec![p.file_path.clone()],
            Code::GetCompletions(p) => vec![p.file_path.clone()],
            Code::PatternSearch(p) => p.file_path.iter().cloned().collect(),
            Code::GenerateCodebaseOverview(p) => p.path.iter().cloned().collect(),
            Code::SearchCodebaseMap(p) => p.path.iter().chain(p.file_path.iter()).cloned().collect(),
            Code::InitializeWorkspace => vec![],
            // Write operations are handled separately
            Code::RenameSymbol(_) | Code::Format(_) | Code::PatternRewrite(_) => vec![],
        }
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
                let warnings = client.lsp_init_warnings();
                if warnings.is_empty() {
                    Ok(text_output("Workspace initialized successfully"))
                } else {
                    let mut msg = String::from("Workspace initialized with warnings:\n");
                    for w in &warnings {
                        msg.push_str(&format!("⚠ {w}\n"));
                    }
                    Ok(text_output(&msg))
                }
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

use std::path::Path;

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
    let files = if result.modified_files.is_empty() {
        String::new()
    } else {
        format!("\nFiles: {}", result.modified_files.join(", "))
    };
    if dry_run {
        text_output(format!(
            "[Dry Run] Would modify {} files with {} replacements.{}\nSet dry_run=false to apply.",
            result.files_modified, result.replacements, files
        ))
    } else {
        text_output(format!(
            "Modified {} files with {} replacements.{}",
            result.files_modified, result.replacements, files
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test::{
        TestDir,
        TestProvider,
    };

    // ===== Default helpers =====
    #[test]
    fn test_default_dry_run() {
        assert!(default_dry_run());
    }

    #[test]
    fn test_default_completion_limit() {
        assert_eq!(default_completion_limit(), 50);
    }

    #[test]
    fn test_default_tab_size() {
        assert_eq!(default_tab_size(), 4);
    }

    #[test]
    fn test_default_insert_spaces() {
        assert!(default_insert_spaces());
    }

    // ===== validate_position =====
    #[test]
    fn test_validate_position_ok() {
        assert!(validate_position(1, 1).is_ok());
        assert!(validate_position(100, 200).is_ok());
    }

    #[test]
    fn test_validate_position_zero_row() {
        let err = validate_position(0, 1).unwrap_err();
        assert!(err.contains("row must be >= 1"));
    }

    #[test]
    fn test_validate_position_negative_row() {
        let err = validate_position(-5, 1).unwrap_err();
        assert!(err.contains("row must be >= 1"));
    }

    #[test]
    fn test_validate_position_zero_column() {
        let err = validate_position(1, 0).unwrap_err();
        assert!(err.contains("column must be >= 1"));
    }

    #[test]
    fn test_validate_position_negative_column() {
        let err = validate_position(1, -1).unwrap_err();
        assert!(err.contains("column must be >= 1"));
    }

    // ===== validate_file_exists =====
    #[tokio::test]
    async fn test_validate_file_exists_relative() {
        let test_dir = TestDir::new();
        let provider = TestProvider::new_with_base(test_dir.path());
        let test_dir = test_dir.with_file_sys(("test.txt", "hi"), &provider).await;

        assert!(validate_file_exists(&provider, "test.txt").is_ok());
        let err = validate_file_exists(&provider, "missing.txt").unwrap_err();
        assert!(err.contains("does not exist"));
        drop(test_dir);
    }

    #[tokio::test]
    async fn test_validate_file_exists_absolute() {
        let test_dir = TestDir::new();
        let provider = TestProvider::new_with_base(test_dir.path());
        let test_dir = test_dir.with_file_sys(("file.txt", "x"), &provider).await;

        let abs_path = test_dir.join("file.txt");
        assert!(validate_file_exists(&provider, &abs_path.to_string_lossy()).is_ok());
        drop(test_dir);
    }

    // ===== resolve_path =====
    #[test]
    fn test_resolve_path_absolute() {
        let cwd = PathBuf::from("/home/user");
        let result = resolve_path(&cwd, "/abs/path");
        assert_eq!(result, PathBuf::from("/abs/path"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let cwd = PathBuf::from("/home/user");
        let result = resolve_path(&cwd, "rel/path");
        assert_eq!(result, PathBuf::from("/home/user/rel/path"));
    }

    // ===== is_write_operation =====
    #[test]
    fn test_is_write_operation_rename() {
        let op = Code::RenameSymbol(RenameSymbolParams {
            file_path: "x.rs".into(),
            row: 1,
            column: 1,
            new_name: "new".into(),
            dry_run: true,
        });
        assert!(is_write_operation(&op));
        assert!(op.is_write_operation());
    }

    #[test]
    fn test_is_write_operation_format() {
        let op = Code::Format(FormatCodeParams {
            file_path: None,
            tab_size: 4,
            insert_spaces: true,
            dry_run: true,
        });
        assert!(is_write_operation(&op));
        assert!(op.is_write_operation());
    }

    #[test]
    fn test_is_write_operation_pattern_rewrite() {
        let op = Code::PatternRewrite(PatternRewriteParams {
            pattern: "$X".into(),
            replacement: "$Y".into(),
            language: "rust".into(),
            file_path: None,
            dry_run: true,
            limit: None,
        });
        assert!(is_write_operation(&op));
    }

    #[test]
    fn test_is_write_operation_read_ops() {
        let ops = vec![
            Code::SearchSymbols(SearchSymbolsParams {
                symbol_name: "foo".into(),
                path: None,
                symbol_type: None,
                limit: None,
                language: None,
                exact_match: None,
            }),
            Code::InitializeWorkspace,
            Code::GetDiagnostics(GetDiagnosticsParams {
                file_path: "x.rs".into(),
            }),
        ];
        for op in ops {
            assert!(!is_write_operation(&op));
            assert!(!op.is_write_operation());
        }
    }

    // ===== validate() method =====
    #[tokio::test]
    async fn test_validate_search_symbols_ok() {
        let p = TestProvider::new();
        let op = Code::SearchSymbols(SearchSymbolsParams {
            symbol_name: "foo".into(),
            path: None,
            symbol_type: None,
            limit: None,
            language: None,
            exact_match: None,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_search_symbols_empty() {
        let p = TestProvider::new();
        let op = Code::SearchSymbols(SearchSymbolsParams {
            symbol_name: "  ".into(),
            path: None,
            symbol_type: None,
            limit: None,
            language: None,
            exact_match: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("symbol_name cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_lookup_symbols_empty() {
        let p = TestProvider::new();
        let op = Code::LookupSymbols(LookupSymbolsParams {
            symbols: vec![],
            file_path: None,
            include_source: false,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("symbols list cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_lookup_symbols_ok() {
        let p = TestProvider::new();
        let op = Code::LookupSymbols(LookupSymbolsParams {
            symbols: vec!["foo".into()],
            file_path: None,
            include_source: false,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_pattern_search_empty_pattern() {
        let p = TestProvider::new();
        let op = Code::PatternSearch(PatternSearchParams {
            pattern: " ".into(),
            language: "rust".into(),
            file_path: None,
            limit: None,
            offset: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("pattern cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_pattern_search_empty_lang() {
        let p = TestProvider::new();
        let op = Code::PatternSearch(PatternSearchParams {
            pattern: "x".into(),
            language: "".into(),
            file_path: None,
            limit: None,
            offset: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("language must be specified"));
    }

    #[tokio::test]
    async fn test_validate_pattern_search_ok() {
        let p = TestProvider::new();
        let op = Code::PatternSearch(PatternSearchParams {
            pattern: "$X".into(),
            language: "rust".into(),
            file_path: None,
            limit: None,
            offset: None,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_pattern_rewrite_empty_replacement() {
        let p = TestProvider::new();
        let op = Code::PatternRewrite(PatternRewriteParams {
            pattern: "$X".into(),
            replacement: "".into(),
            language: "rust".into(),
            file_path: None,
            dry_run: true,
            limit: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("replacement cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_pattern_rewrite_empty_pattern() {
        let p = TestProvider::new();
        let op = Code::PatternRewrite(PatternRewriteParams {
            pattern: "".into(),
            replacement: "y".into(),
            language: "rust".into(),
            file_path: None,
            dry_run: true,
            limit: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("pattern cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_pattern_rewrite_empty_lang() {
        let p = TestProvider::new();
        let op = Code::PatternRewrite(PatternRewriteParams {
            pattern: "$X".into(),
            replacement: "y".into(),
            language: "".into(),
            file_path: None,
            dry_run: true,
            limit: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("language must be specified"));
    }

    #[tokio::test]
    async fn test_validate_pattern_rewrite_ok() {
        let p = TestProvider::new();
        let op = Code::PatternRewrite(PatternRewriteParams {
            pattern: "$X".into(),
            replacement: "y".into(),
            language: "rust".into(),
            file_path: None,
            dry_run: true,
            limit: None,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_format_zero_tab_size() {
        let p = TestProvider::new();
        let op = Code::Format(FormatCodeParams {
            file_path: None,
            tab_size: 0,
            insert_spaces: true,
            dry_run: true,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("tab_size must be >= 1"));
    }

    #[tokio::test]
    async fn test_validate_format_ok() {
        let p = TestProvider::new();
        let op = Code::Format(FormatCodeParams {
            file_path: None,
            tab_size: 4,
            insert_spaces: true,
            dry_run: true,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_initialize_workspace() {
        let p = TestProvider::new();
        assert!(Code::InitializeWorkspace.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_generate_codebase_overview() {
        let p = TestProvider::new();
        let op = Code::GenerateCodebaseOverview(GenerateCodebaseOverviewParams { path: None });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_search_codebase_map() {
        let p = TestProvider::new();
        let op = Code::SearchCodebaseMap(SearchCodebaseMapParams {
            path: None,
            file_path: None,
        });
        assert!(op.validate(&p).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_find_references_invalid_position() {
        let p = TestProvider::new();
        let op = Code::FindReferences(FindReferencesParams {
            file_path: "x.rs".into(),
            row: 0,
            column: 1,
            limit: None,
            workspace_only: None,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_find_references_missing_file() {
        let p = TestProvider::new();
        let op = Code::FindReferences(FindReferencesParams {
            file_path: "/nonexistent/file.rs".into(),
            row: 1,
            column: 1,
            limit: None,
            workspace_only: None,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_validate_goto_definition_invalid_position() {
        let p = TestProvider::new();
        let op = Code::GotoDefinition(GotoDefinitionParams {
            file_path: "x.rs".into(),
            row: 1,
            column: 0,
            show_source: None,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_get_hover_invalid_position() {
        let p = TestProvider::new();
        let op = Code::GetHover(GetHoverParams {
            file_path: "x.rs".into(),
            row: -1,
            column: 1,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_rename_symbol_empty_name() {
        let p = TestProvider::new();
        let op = Code::RenameSymbol(RenameSymbolParams {
            file_path: "x.rs".into(),
            row: 1,
            column: 1,
            new_name: "  ".into(),
            dry_run: true,
        });
        let err = op.validate(&p).await.unwrap_err();
        assert!(err.contains("new_name cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_rename_symbol_invalid_pos() {
        let p = TestProvider::new();
        let op = Code::RenameSymbol(RenameSymbolParams {
            file_path: "x.rs".into(),
            row: 0,
            column: 1,
            new_name: "new".into(),
            dry_run: true,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_get_completions_invalid_pos() {
        let p = TestProvider::new();
        let op = Code::GetCompletions(GetCompletionsParams {
            file_path: "x.rs".into(),
            row: 1,
            column: -1,
            trigger_character: None,
            limit: 50,
            filter: None,
            symbol_type: None,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_get_document_symbols_missing_file() {
        let p = TestProvider::new();
        let op = Code::GetDocumentSymbols(GetDocumentSymbolsParams {
            file_path: "/missing.rs".into(),
            top_level_only: None,
        });
        assert!(op.validate(&p).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_get_diagnostics_missing_file() {
        let p = TestProvider::new();
        let op = Code::GetDiagnostics(GetDiagnosticsParams {
            file_path: "/missing.rs".into(),
        });
        assert!(op.validate(&p).await.is_err());
    }

    // ===== Format helpers =====
    fn extract_text(out: &ToolExecutionOutput) -> String {
        match &out.items[0] {
            ToolExecutionOutputItem::Text(s) => s.clone(),
            _ => panic!("expected text output"),
        }
    }

    fn make_symbol(name: &str) -> code_agent_sdk::SymbolInfo {
        code_agent_sdk::SymbolInfo {
            name: name.to_string(),
            symbol_type: Some("Function".to_string()),
            file_path: "src/lib.rs".to_string(),
            start_row: 10,
            end_row: 20,
            start_column: 1,
            end_column: 1,
            container_name: None,
            detail: Some("fn foo()".to_string()),
            source_line: None,
            source_code: None,
            language: Some("rust".to_string()),
        }
    }

    #[test]
    fn test_format_symbols_empty() {
        let out = format_symbols(&[]);
        assert_eq!(extract_text(&out), "No symbols found");
    }

    #[test]
    fn test_format_symbols_with_data() {
        let symbols = vec![make_symbol("foo"), make_symbol("bar")];
        let text = extract_text(&format_symbols(&symbols));
        assert!(text.contains("foo"));
        assert!(text.contains("bar"));
        assert!(text.contains("Function"));
        assert!(text.contains("src/lib.rs"));
    }

    #[test]
    fn test_format_symbols_with_source() {
        let mut sym = make_symbol("foo");
        sym.source_code = Some("fn foo() {}".to_string());
        let text = extract_text(&format_symbols(&[sym]));
        assert!(text.contains("fn foo() {}"));
    }

    #[test]
    fn test_format_symbols_no_type_no_detail() {
        let mut sym = make_symbol("foo");
        sym.symbol_type = None;
        sym.detail = None;
        let text = extract_text(&format_symbols(&[sym]));
        assert!(text.contains("Symbol"));
        assert!(text.contains("foo"));
    }

    #[test]
    fn test_format_references_empty() {
        let result = code_agent_sdk::ApiReferencesResult {
            references: vec![],
            total_count: 0,
        };
        let out = format_references(&result, None);
        assert_eq!(extract_text(&out), "No references found");
    }

    #[test]
    fn test_format_references_with_data() {
        let result = code_agent_sdk::ApiReferencesResult {
            references: vec![code_agent_sdk::model::entities::ReferenceInfo {
                file_path: "src/main.rs".to_string(),
                start_row: 5,
                start_column: 10,
                end_row: 5,
                end_column: 15,
                source_line: None,
            }],
            total_count: 1,
        };
        let text = extract_text(&format_references(&result, None));
        assert!(text.contains("Found 1 references"));
        assert!(text.contains("src/main.rs:5:10"));
    }

    #[test]
    fn test_format_references_truncated() {
        let refs: Vec<_> = (0..5)
            .map(|i| code_agent_sdk::model::entities::ReferenceInfo {
                file_path: format!("file{i}.rs"),
                start_row: i + 1,
                start_column: 1,
                end_row: i + 1,
                end_column: 5,
                source_line: None,
            })
            .collect();
        let result = code_agent_sdk::ApiReferencesResult {
            references: refs,
            total_count: 5,
        };
        let text = extract_text(&format_references(&result, Some(2)));
        assert!(text.contains("and 3 more"));
    }

    #[test]
    fn test_format_definition() {
        let def = code_agent_sdk::DefinitionInfo {
            file_path: "src/lib.rs".to_string(),
            start_row: 10,
            start_column: 5,
            end_row: 10,
            end_column: 8,
            source_line: Some("fn foo() {}".to_string()),
        };
        let text = extract_text(&format_definition(&def));
        assert!(text.contains("src/lib.rs:10:5"));
        assert!(text.contains("fn foo() {}"));
    }

    #[test]
    fn test_format_definition_no_source() {
        let def = code_agent_sdk::DefinitionInfo {
            file_path: "src/lib.rs".to_string(),
            start_row: 10,
            start_column: 5,
            end_row: 10,
            end_column: 8,
            source_line: None,
        };
        let text = extract_text(&format_definition(&def));
        assert!(text.contains("src/lib.rs:10:5"));
        assert!(!text.contains("Source:"));
    }

    #[test]
    fn test_format_diagnostics_empty() {
        let text = extract_text(&format_diagnostics(&[]));
        assert_eq!(text, "No diagnostics");
    }

    #[test]
    fn test_format_diagnostics_with_data() {
        let diag = code_agent_sdk::ApiDiagnosticInfo {
            message: "unused variable".to_string(),
            severity: code_agent_sdk::model::entities::DiagnosticSeverity::Warning,
            start_row: 5,
            start_column: 10,
            end_row: 5,
            end_column: 15,
            source: None,
            code: None,
            related_information: vec![],
        };
        let text = extract_text(&format_diagnostics(&[diag]));
        assert!(text.contains("Warning"));
        assert!(text.contains("5:10"));
        assert!(text.contains("unused variable"));
    }

    #[test]
    fn test_format_hover_empty() {
        let hover = code_agent_sdk::model::entities::HoverInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            content: None,
        };
        let text = extract_text(&format_hover(&hover));
        assert_eq!(text, "");
    }

    #[test]
    fn test_format_hover_with_content() {
        let hover = code_agent_sdk::model::entities::HoverInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            content: Some("fn foo()".to_string()),
        };
        let text = extract_text(&format_hover(&hover));
        assert!(text.contains("fn foo()"));
    }

    #[test]
    fn test_format_completions_empty() {
        let info = code_agent_sdk::model::entities::CompletionInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            items: vec![],
            total_count: 0,
        };
        let text = extract_text(&format_completions(&info, 50, None));
        assert_eq!(text, "No completions");
    }

    #[test]
    fn test_format_completions_with_data() {
        let info = code_agent_sdk::model::entities::CompletionInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            items: vec![
                code_agent_sdk::model::entities::CompletionItem {
                    label: "foo".to_string(),
                    kind: None,
                    detail: Some("fn foo()".to_string()),
                    documentation: None,
                },
                code_agent_sdk::model::entities::CompletionItem {
                    label: "bar".to_string(),
                    kind: None,
                    detail: None,
                    documentation: None,
                },
            ],
            total_count: 2,
        };
        let text = extract_text(&format_completions(&info, 50, None));
        assert!(text.contains("- foo"));
        assert!(text.contains("(fn foo())"));
        assert!(text.contains("- bar"));
    }

    #[test]
    fn test_format_completions_with_filter() {
        let info = code_agent_sdk::model::entities::CompletionInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            items: vec![
                code_agent_sdk::model::entities::CompletionItem {
                    label: "foo_bar".to_string(),
                    kind: None,
                    detail: None,
                    documentation: None,
                },
                code_agent_sdk::model::entities::CompletionItem {
                    label: "baz".to_string(),
                    kind: None,
                    detail: None,
                    documentation: None,
                },
            ],
            total_count: 2,
        };
        let text = extract_text(&format_completions(&info, 50, Some("foo")));
        assert!(text.contains("foo_bar"));
        assert!(!text.contains("baz"));
    }

    #[test]
    fn test_format_completions_with_limit() {
        let items: Vec<_> = (0..10)
            .map(|i| code_agent_sdk::model::entities::CompletionItem {
                label: format!("item{i}"),
                kind: None,
                detail: None,
                documentation: None,
            })
            .collect();
        let info = code_agent_sdk::model::entities::CompletionInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            items,
            total_count: 10,
        };
        let text = extract_text(&format_completions(&info, 2, None));
        assert!(text.contains("item0"));
        assert!(text.contains("item1"));
        assert!(!text.contains("item5"));
    }

    #[test]
    fn test_format_completions_no_match_with_filter() {
        let info = code_agent_sdk::model::entities::CompletionInfo {
            file_path: "x.rs".to_string(),
            row: 1,
            column: 1,
            items: vec![code_agent_sdk::model::entities::CompletionItem {
                label: "xxx".to_string(),
                kind: None,
                detail: None,
                documentation: None,
            }],
            total_count: 1,
        };
        let text = extract_text(&format_completions(&info, 50, Some("nomatch")));
        // No items should match "nomatch"
        assert_eq!(text, "No completions");
    }

    #[test]
    fn test_format_definition_minimal() {
        let def = code_agent_sdk::DefinitionInfo {
            file_path: "x.rs".to_string(),
            start_row: 1,
            start_column: 1,
            end_row: 1,
            end_column: 1,
            source_line: None,
        };
        let text = extract_text(&format_definition(&def));
        assert!(text.contains("x.rs:1:1"));
    }

    #[test]
    fn test_format_diagnostics_multiple() {
        let diags = vec![
            code_agent_sdk::ApiDiagnosticInfo {
                message: "first error".to_string(),
                severity: code_agent_sdk::model::entities::DiagnosticSeverity::Error,
                start_row: 1,
                start_column: 1,
                end_row: 1,
                end_column: 5,
                source: Some("rust".to_string()),
                code: None,
                related_information: vec![],
            },
            code_agent_sdk::ApiDiagnosticInfo {
                message: "warning".to_string(),
                severity: code_agent_sdk::model::entities::DiagnosticSeverity::Warning,
                start_row: 5,
                start_column: 1,
                end_row: 5,
                end_column: 5,
                source: None,
                code: None,
                related_information: vec![],
            },
        ];
        let text = extract_text(&format_diagnostics(&diags));
        assert!(text.contains("first error"));
        assert!(text.contains("warning"));
    }

    #[test]
    fn test_format_rename_result_dry_run() {
        let result = code_agent_sdk::model::entities::RenameResult {
            file_count: 3,
            edit_count: 7,
        };
        let text = extract_text(&format_rename_result(&result, true));
        assert!(text.contains("Would rename 7 edits"));
        assert!(text.contains("3 files"));
    }

    #[test]
    fn test_format_rename_result_applied() {
        let result = code_agent_sdk::model::entities::RenameResult {
            file_count: 3,
            edit_count: 7,
        };
        let text = extract_text(&format_rename_result(&result, false));
        assert!(text.contains("Renamed 7 edits"));
        assert!(text.contains("3 files"));
    }

    #[test]
    fn test_format_pattern_matches_empty() {
        let text = extract_text(&format_pattern_matches(&[]));
        assert_eq!(text, "No matches found");
    }

    #[test]
    fn test_format_pattern_matches_with_data() {
        let m = code_agent_sdk::PatternMatch {
            file_path: "src/lib.rs".to_string(),
            matched_code: "x.unwrap()\nnext line".to_string(),
            start_row: 5,
            start_column: 1,
            end_row: 5,
            end_column: 10,
            enclosing_symbols: vec![],
        };
        let text = extract_text(&format_pattern_matches(&[m]));
        assert!(text.contains("Found 1 matches"));
        assert!(text.contains("src/lib.rs:5:1"));
        assert!(text.contains("x.unwrap()"));
        // should only have first line
        assert!(!text.contains("next line"));
    }

    #[test]
    fn test_format_rewrite_result_dry_run() {
        let result = code_agent_sdk::RewriteResult {
            files_modified: 2,
            replacements: 5,
            modified_files: vec!["a.rs".to_string(), "b.rs".to_string()],
            dry_run: true,
        };
        let text = extract_text(&format_rewrite_result(&result, true));
        assert!(text.contains("Dry Run"));
        assert!(text.contains("Would modify 2 files"));
        assert!(text.contains("5 replacements"));
        assert!(text.contains("a.rs"));
        assert!(text.contains("b.rs"));
    }

    #[test]
    fn test_format_rewrite_result_applied() {
        let result = code_agent_sdk::RewriteResult {
            files_modified: 1,
            replacements: 3,
            modified_files: vec!["x.rs".to_string()],
            dry_run: false,
        };
        let text = extract_text(&format_rewrite_result(&result, false));
        assert!(text.contains("Modified 1 files"));
        assert!(text.contains("3 replacements"));
        assert!(text.contains("x.rs"));
        assert!(!text.contains("Dry Run"));
    }

    #[test]
    fn test_format_rewrite_result_no_files() {
        let result = code_agent_sdk::RewriteResult {
            files_modified: 0,
            replacements: 0,
            modified_files: vec![],
            dry_run: true,
        };
        let text = extract_text(&format_rewrite_result(&result, true));
        assert!(!text.contains("Files:"));
    }

    // ===== Trait impl =====
    #[test]
    fn test_built_in_tool_trait() {
        assert!(matches!(Code::name(), BuiltInToolName::Code));
        assert!(!Code::description().is_empty());
        assert!(!Code::input_schema().is_empty());
        let aliases = Code::aliases().unwrap();
        assert!(aliases.contains(&"code"));
    }

    // ===== Serde =====
    #[test]
    fn test_serde_search_symbols() {
        let json = r#"{"operation":"search_symbols","symbol_name":"foo"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::SearchSymbols(_)));
    }

    #[test]
    fn test_serde_initialize_workspace() {
        let json = r#"{"operation":"initialize_workspace"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::InitializeWorkspace));
    }

    #[test]
    fn test_serde_pattern_search() {
        let json = r#"{"operation":"pattern_search","pattern":"$X","language":"rust"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::PatternSearch(_)));
    }

    #[test]
    fn test_serde_format() {
        let json = r#"{"operation":"format"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::Format(p) = op {
            assert_eq!(p.tab_size, 4);
            assert!(p.insert_spaces);
            assert!(p.dry_run);
        } else {
            panic!("expected Format");
        }
    }

    #[test]
    fn test_serde_lookup_symbols() {
        let json = r#"{"operation":"lookup_symbols","symbols":["foo","bar"]}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::LookupSymbols(p) = op {
            assert_eq!(p.symbols, vec!["foo", "bar"]);
            assert!(!p.include_source);
        } else {
            panic!("expected LookupSymbols");
        }
    }

    #[test]
    fn test_serde_find_references() {
        let json = r#"{"operation":"find_references","file_path":"x.rs","row":1,"column":1}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::FindReferences(_)));
    }

    #[test]
    fn test_serde_goto_definition() {
        let json = r#"{"operation":"goto_definition","file_path":"x.rs","row":1,"column":1}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::GotoDefinition(_)));
    }

    #[test]
    fn test_serde_rename_symbol() {
        let json = r#"{"operation":"rename_symbol","file_path":"x.rs","row":1,"column":1,"new_name":"foo"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::RenameSymbol(p) = op {
            assert_eq!(p.new_name, "foo");
            assert!(p.dry_run); // default true
        } else {
            panic!("expected RenameSymbol");
        }
    }

    #[test]
    fn test_serde_get_document_symbols() {
        let json = r#"{"operation":"get_document_symbols","file_path":"x.rs","top_level_only":true}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::GetDocumentSymbols(p) = op {
            assert_eq!(p.top_level_only, Some(true));
        } else {
            panic!("expected GetDocumentSymbols");
        }
    }

    #[test]
    fn test_serde_get_diagnostics() {
        let json = r#"{"operation":"get_diagnostics","file_path":"x.rs"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::GetDiagnostics(_)));
    }

    #[test]
    fn test_serde_get_hover() {
        let json = r#"{"operation":"get_hover","file_path":"x.rs","row":5,"column":3}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Code::GetHover(_)));
    }

    #[test]
    fn test_serde_get_completions() {
        let json = r#"{"operation":"get_completions","file_path":"x.rs","row":1,"column":1}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::GetCompletions(p) = op {
            assert_eq!(p.limit, 50); // default
        } else {
            panic!("expected GetCompletions");
        }
    }

    #[test]
    fn test_serde_pattern_rewrite() {
        let json = r#"{"operation":"pattern_rewrite","pattern":"$X","replacement":"$Y","language":"rust"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::PatternRewrite(p) = op {
            assert_eq!(p.pattern, "$X");
            assert_eq!(p.replacement, "$Y");
            assert!(p.dry_run);
        } else {
            panic!("expected PatternRewrite");
        }
    }

    #[test]
    fn test_serde_generate_codebase_overview() {
        let json = r#"{"operation":"generate_codebase_overview"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::GenerateCodebaseOverview(p) = op {
            assert!(p.path.is_none());
        } else {
            panic!("expected GenerateCodebaseOverview");
        }
    }

    #[test]
    fn test_serde_search_codebase_map() {
        let json = r#"{"operation":"search_codebase_map","path":"/p"}"#;
        let op: Code = serde_json::from_str(json).unwrap();
        if let Code::SearchCodebaseMap(p) = op {
            assert_eq!(p.path, Some("/p".to_string()));
        } else {
            panic!("expected SearchCodebaseMap");
        }
    }

    #[test]
    fn test_resolve_path_empty() {
        let cwd = PathBuf::from("/home/user");
        let result = resolve_path(&cwd, "");
        assert_eq!(result, cwd);
    }

    #[test]
    fn test_format_rewrite_result_no_modified() {
        let result = code_agent_sdk::RewriteResult {
            files_modified: 0,
            replacements: 0,
            modified_files: vec![],
            dry_run: false,
        };
        let text = extract_text(&format_rewrite_result(&result, false));
        assert!(text.contains("0 files"));
        assert!(text.contains("0 replacements"));
    }
}
