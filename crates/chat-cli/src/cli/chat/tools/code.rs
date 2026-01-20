use std::io::Write;

use code_agent_sdk::{
    ApiDiagnosticInfo,
    SymbolInfo,
};
use eyre::Result;
use serde::Deserialize;
use spinners::{
    Spinner,
    Spinners,
};

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::os::Os;

/// Maximum number of results to display before showing "(x more items found)"
const MAX_VISIBLE_RESULTS: usize = 5;
/// Maximum visible pattern matches (more verbose with code snippets)
const MAX_VISIBLE_PATTERN_MATCHES: usize = 5;

/// Default limit for find_references results
const DEFAULT_REFERENCES_LIMIT: usize = 500;

/// Maximum limit for find_references results
const MAX_REFERENCES_LIMIT: usize = 1000;

/// Check if code intelligence feature is enabled
pub(crate) fn is_enabled(_os: &Os) -> bool {
    if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
        return false;
    }
    std::env::current_dir()
        .map(|cwd| code_agent_sdk::ConfigManager::lsp_config_exists(&cwd))
        .unwrap_or(false)
}

/// Check if an operation is a write operation
pub fn is_write_operation(op: &Code) -> bool {
    matches!(op, Code::RenameSymbol(_) | Code::Format(_) | Code::PatternRewrite(_))
}

/// Code intelligence operations - single unified tool with permission-based scoping
#[derive(Debug, Clone, Deserialize)]
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

impl Code {
    pub const INFO: super::ToolInfo = super::ToolInfo {
        spec_name: "code",
        preferred_alias: "code",
        aliases: &["code", "code/read", "code/write"],
    };

    /// Check if code tool should be included based on agent's tool list
    pub fn should_include(tool_list: &[String]) -> bool {
        use crate::cli::chat::tools::ToolMetadata;
        use crate::util::consts::BUILTIN_TOOLS_PREFIX;
        use crate::util::pattern_matching::matches_any_pattern;

        let patterns: std::collections::HashSet<&str> = tool_list.iter().map(|s| s.as_str()).collect();

        // For native tools, check if @builtin matches
        if matches_any_pattern(&patterns, BUILTIN_TOOLS_PREFIX) {
            return true;
        }

        // Check if any tool alias matches any pattern
        ToolMetadata::CODE
            .aliases
            .iter()
            .any(|alias| matches_any_pattern(&patterns, alias))
    }
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct FindReferencesParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub workspace_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GotoDefinitionParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    #[serde(default)]
    pub show_source: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenameSymbolParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
    pub new_name: String,
    #[serde(default)]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormatCodeParams {
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default = "default_tab_size")]
    pub tab_size: i32,
    #[serde(default = "default_insert_spaces")]
    pub insert_spaces: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetDocumentSymbolsParams {
    pub file_path: String,
    #[serde(default)]
    pub top_level_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LookupSymbolsParams {
    pub symbols: Vec<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub include_source: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetDiagnosticsParams {
    pub file_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetHoverParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct PatternSearchParams {
    /// AST pattern to search for (e.g., "$X.unwrap()", "console.log($$$)")
    pub pattern: String,
    /// Programming language (rust, javascript, typescript, python, go, java, etc.)
    pub language: String,
    /// Optional file path to scope search to a single file
    #[serde(default)]
    pub file_path: Option<String>,
    /// Maximum number of results (default: 50)
    #[serde(default)]
    pub limit: Option<u32>,
    /// Number of results to skip (for pagination)
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatternRewriteParams {
    /// AST pattern to find (e.g., "$X.unwrap()")
    pub pattern: String,
    /// Replacement pattern (e.g., "$X.expect(\"error\")")
    pub replacement: String,
    /// Programming language (rust, javascript, typescript, python, go, java, etc.)
    pub language: String,
    /// Optional file path to scope rewrite to a single file
    #[serde(default)]
    pub file_path: Option<String>,
    /// Preview changes without applying (default: true)
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    /// Maximum number of files to modify (default: 50)
    #[serde(default)]
    pub limit: Option<u32>,
}

fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateCodebaseOverviewParams {
    /// Directory path (defaults to current workspace)
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchCodebaseMapParams {
    /// Directory path to search in (defaults to current workspace)
    #[serde(default)]
    pub path: Option<String>,
    /// File path/name pattern to match (e.g., "main.rs" or "src/tools")
    #[serde(default)]
    pub file_path: Option<String>,
}

fn default_tab_size() -> i32 {
    4
}

fn default_completion_limit() -> usize {
    50
}

fn default_insert_spaces() -> bool {
    true
}

// Module-level helper functions accessible to all code tool types
pub(super) fn validate_file_exists(os: &Os, file_path: &str) -> Result<()> {
    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, file_path);
    if !path.exists() {
        eyre::bail!("File path '{}' does not exist", file_path);
    }
    Ok(())
}

fn validate_directory_exists(os: &Os, dir_path: &str) -> Result<()> {
    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, dir_path);
    if !path.is_dir() {
        eyre::bail!("Directory '{}' does not exist", dir_path);
    }
    Ok(())
}

pub(super) fn validate_position(row: i32, column: i32) -> Result<()> {
    if row < 1 {
        eyre::bail!("Row number must be >= 1 (got {})", row);
    }
    if column < 1 {
        eyre::bail!("Column number must be >= 1 (got {})", column);
    }
    Ok(())
}

impl Code {
    /// Evaluate permission for code intelligence operation
    pub fn eval_perm(
        _os: &Os,
        agent: &crate::cli::agent::Agent,
        code: &Code,
    ) -> crate::cli::agent::PermissionEvalResult {
        use crate::cli::agent::PermissionEvalResult;
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return PermissionEvalResult::Deny(vec!["Code intelligence feature is not available".to_string()]);
        }

        // Read operations are always allowed
        if !is_write_operation(code) {
            return PermissionEvalResult::Allow;
        }

        // Write operations: check if code tool is trusted
        let is_trusted = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));

        if is_trusted {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    /// Validate code operation
    pub async fn validate(&self, os: &Os) -> Result<()> {
        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return Err(eyre::eyre!("Code intelligence feature is not available"));
        }

        // Validate operation-specific parameters
        match self {
            Code::SearchSymbols(params) => {
                if params.symbol_name.trim().is_empty() {
                    eyre::bail!("Symbol name cannot be empty");
                }
                if let Some(path) = &params.path {
                    validate_file_exists(os, path)?;
                }
                Ok(())
            },
            Code::FindReferences(params) => {
                validate_file_exists(os, &params.file_path)?;
                validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::GotoDefinition(params) => {
                validate_file_exists(os, &params.file_path)?;
                validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::RenameSymbol(params) => {
                validate_file_exists(os, &params.file_path)?;
                validate_position(params.row, params.column)?;
                if params.new_name.trim().is_empty() {
                    eyre::bail!("New name cannot be empty");
                }
                Ok(())
            },
            Code::Format(params) => {
                if let Some(file_path) = &params.file_path {
                    validate_file_exists(os, file_path)?;
                }
                if params.tab_size < 1 {
                    eyre::bail!("Tab size must be >= 1 (got {})", params.tab_size);
                }
                Ok(())
            },
            Code::GetDocumentSymbols(params) => {
                validate_file_exists(os, &params.file_path)?;
                Ok(())
            },
            Code::LookupSymbols(params) => {
                if params.symbols.is_empty() {
                    eyre::bail!("Symbols list cannot be empty");
                }
                if let Some(file_path) = &params.file_path {
                    validate_file_exists(os, file_path)?;
                }
                Ok(())
            },
            Code::GetDiagnostics(params) => {
                validate_file_exists(os, &params.file_path)?;
                Ok(())
            },
            Code::GetHover(params) => {
                validate_file_exists(os, &params.file_path)?;
                validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::GetCompletions(params) => {
                validate_file_exists(os, &params.file_path)?;
                validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::PatternSearch(params) => {
                if params.pattern.trim().is_empty() {
                    eyre::bail!("Pattern cannot be empty");
                }
                if params.language.trim().is_empty() {
                    eyre::bail!("Language must be specified");
                }
                if let Some(file_path) = &params.file_path {
                    validate_file_exists(os, file_path)?;
                }
                Ok(())
            },
            Code::PatternRewrite(params) => {
                if params.pattern.trim().is_empty() {
                    eyre::bail!("Pattern cannot be empty");
                }
                if params.replacement.trim().is_empty() {
                    eyre::bail!("Replacement cannot be empty");
                }
                if params.language.trim().is_empty() {
                    eyre::bail!("Language must be specified");
                }
                if let Some(file_path) = &params.file_path {
                    validate_file_exists(os, file_path)?;
                }
                Ok(())
            },
            Code::InitializeWorkspace => Ok(()),
            Code::GenerateCodebaseOverview(params) => {
                if let Some(path) = &params.path {
                    validate_directory_exists(os, path)?;
                }
                Ok(())
            },
            Code::SearchCodebaseMap(params) => {
                if let Some(path) = &params.path {
                    validate_directory_exists(os, path)?;
                }
                Ok(())
            },
        }
    }

    fn stop_spinner(spinner: &mut Option<Spinner>, stdout: &mut impl Write) -> Result<()> {
        use crossterm::{
            cursor,
            queue,
            terminal,
        };

        drop(spinner.take());
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToColumn(0),
            cursor::MoveUp(1),
            cursor::Hide,
        )?;
        Ok(())
    }

    pub async fn invoke(
        &self,
        _os: &Os,
        _stdout: &mut impl Write,
        code_intelligence_client: &Option<std::sync::Arc<tokio::sync::RwLock<code_agent_sdk::CodeIntelligence>>>,
    ) -> Result<InvokeOutput> {
        tracing::info!("🔧 Invoking code tool operation: {:?}", self);

        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        #[allow(unused_assignments)]
        let mut result = String::new();

        // Check if feature is enabled but client wasn't initialized at startup
        if code_intelligence_client.is_none() && crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return Err(eyre::eyre!(
                "Code intelligence feature was enabled after chat started. Please restart the chat session to use code intelligence features."
            ));
        }

        if let Some(client_lock) = code_intelligence_client {
            let mut client = client_lock.write().await;

            // Check if lsp.json exists (user has run /code init)
            let is_configured = client.is_code_intelligence_initialized();

            // Auto-initialize if json exists but workspace not initialized yet
            if is_configured
                && client.workspace_status() == code_agent_sdk::sdk::WorkspaceStatus::NotInitialized
                && let Err(e) = client.initialize().await
            {
                return Err(eyre::eyre!("Failed to initialize code intelligence: {e}"));
            }

            // Check if workspace is initialized (only if LSP is configured)
            if !matches!(self, Code::InitializeWorkspace) && is_configured {
                match client.workspace_status() {
                    code_agent_sdk::sdk::WorkspaceStatus::NotInitialized => {
                        return Ok(InvokeOutput {
                            output: OutputKind::Text(
                                "Workspace initialization failed. Run '/code init' to retry.".to_string(),
                            ),
                        });
                    },
                    code_agent_sdk::sdk::WorkspaceStatus::Initializing => {
                        return Ok(InvokeOutput {
                            output: OutputKind::Text(
                                "Workspace is still initializing. LSP servers are starting up. Please wait a moment and try again.".to_string()
                            ),
                        });
                    },
                    code_agent_sdk::sdk::WorkspaceStatus::Initialized => {
                        // Good to proceed
                    },
                }
            }

            queue!(_stdout, style::Print("\n"))?;
            let mut spinner = Some(Spinner::new(Spinners::Dots, "Loading...".to_string()));
            match self {
                Code::SearchSymbols(params) => {
                    const MAX_RESULTS: u32 = 50;
                    const DEFAULT_RESULTS: u32 = 20;

                    let limit = params
                        .limit
                        .map(|l| (l as u32).min(MAX_RESULTS))
                        .or(Some(DEFAULT_RESULTS));

                    let request = code_agent_sdk::model::types::FindSymbolsRequest {
                        symbol_name: params.symbol_name.clone(),
                        file_path: params.path.as_ref().map(std::path::PathBuf::from),
                        symbol_type: params.symbol_type.as_ref().and_then(|s| s.parse().ok()),
                        limit,
                        language: params.language.clone(),
                        exact_match: params.exact_match.unwrap_or(false),
                        timeout_secs: None,
                    };

                    match client.find_symbols(request).await {
                        Ok(symbols) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let scope_path = params.path.as_ref();
                            if symbols.is_empty() {
                                let scope_msg = scope_path.map(|p| format!(" (scoped to: {})", p)).unwrap_or_default();
                                queue!(_stdout, style::Print(&format!("\nNo symbols found{}\n", scope_msg)),)?;
                                result = format!("No symbols found{}", scope_msg);
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_symbols(&symbols, _stdout)?;
                                result = Self::format_symbols_compact(&symbols, false);
                                if let Some(path) = scope_path {
                                    result = format!("Scoped to: {}\n{}", path, result);
                                }
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Search failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to search symbols: {e}");
                        },
                    }
                },
                Code::FindReferences(params) => {
                    let request = code_agent_sdk::model::types::FindReferencesByLocationRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        limit: Some(
                            params
                                .limit
                                .unwrap_or(DEFAULT_REFERENCES_LIMIT)
                                .min(MAX_REFERENCES_LIMIT) as u32,
                        ),
                        offset: None,
                        workspace_only: Some(params.workspace_only.unwrap_or(true)),
                    };

                    match client.find_references_by_location(request).await {
                        Ok(refs_result) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if refs_result.references.is_empty() {
                                queue!(_stdout, style::Print("\nNo references found\n"),)?;
                                result = "No references found".to_string();
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_references(&refs_result.references, _stdout)?;

                                let truncated = refs_result.total_count - refs_result.references.len();
                                result = Self::format_references_compact(&refs_result.references, truncated);
                                if truncated > 0 {
                                    result.push_str(&format!(
                                        "\nNote: Increase limit to see more (max: {MAX_REFERENCES_LIMIT})."
                                    ));
                                }
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let (display_msg, model_msg) =
                                if let Some(code_agent_sdk::CodeIntelligenceError::InvalidPosition { .. }) =
                                    e.downcast_ref::<code_agent_sdk::CodeIntelligenceError>()
                                {
                                    (
                                        format!("Invalid position: line {}, column {}", params.row, params.column),
                                        format!(
                                            "Position error at line {}, column {}. Try adjusting the position.",
                                            params.row, params.column
                                        ),
                                    )
                                } else {
                                    let msg = format!("Failed to find references: {e}");
                                    (msg.clone(), msg)
                                };

                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Find References Error: "),
                                StyledText::reset(),
                                style::Print(&format!("{display_msg}\n")),
                            )?;
                            result = model_msg;
                        },
                    }
                },
                Code::GotoDefinition(params) => {
                    let request = code_agent_sdk::model::types::GotoDefinitionRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        show_source: params.show_source.unwrap_or(true),
                    };

                    match client.goto_definition(request).await {
                        Ok(Some(definition)) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            // Show location with context (max 3 lines, then show count of remaining)
                            let context = if let Some(source) = &definition.source_line {
                                let lines: Vec<&str> = source.lines().collect();
                                if !lines.is_empty() {
                                    let display_lines: Vec<String> =
                                        lines.iter().take(3).map(|line| line.trim().to_string()).collect();
                                    let remaining = lines.len().saturating_sub(3);

                                    let mut context_str = format!(": {}", display_lines.join(" | "));
                                    if remaining > 0 {
                                        context_str.push_str(&format!(" ... ({remaining} more lines)"));
                                    }
                                    context_str
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };

                            queue!(
                                _stdout,
                                style::Print("\n"),
                                StyledText::brand_fg(),
                                style::Print(&definition.file_path),
                                StyledText::reset(),
                                style::Print(":"),
                                StyledText::secondary_fg(),
                                style::Print(&format!("{}:{}", definition.start_row, definition.start_column)),
                                StyledText::reset(),
                            )?;
                            if !context.is_empty() {
                                queue!(
                                    _stdout,
                                    style::Print(": "),
                                    style::Print(&context[2..]), // skip ": " prefix
                                )?;
                            }
                            queue!(_stdout, style::Print("\n"))?;

                            result = Self::format_definition_compact(&definition);
                        },
                        Ok(None) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                style::Print("\nNo definition found for symbol at "),
                                StyledText::brand_fg(),
                                style::Print(&params.file_path),
                                StyledText::reset(),
                                style::Print(":"),
                                StyledText::secondary_fg(),
                                style::Print(&format!("{}:{}", params.row, params.column)),
                                StyledText::reset(),
                                style::Print("\n"),
                            )?;
                            result = "No definition found".to_string();
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                style::Print("\nFailed to find definition: "),
                                StyledText::error_fg(),
                                style::Print(&format!("{e}\n")),
                                StyledText::reset(),
                            )?;
                            result = format!("Failed to find definition: {e}");
                        },
                    }
                },
                Code::RenameSymbol(params) => {
                    let request = code_agent_sdk::model::types::RenameSymbolRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        new_name: params.new_name.clone(),
                        dry_run: params.dry_run.unwrap_or(true),
                    };

                    match client.rename_symbol(request).await {
                        Ok(Some(rename_result)) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let is_dry_run = params.dry_run.unwrap_or(true);

                            queue!(_stdout, style::Print("\n"))?;

                            let mode = if is_dry_run { "Dry Run" } else { "Applied" };
                            queue!(
                                _stdout,
                                StyledText::info_fg(),
                                style::Print(&format!("[{mode}] ")),
                                StyledText::reset(),
                            )?;

                            if is_dry_run {
                                queue!(
                                    _stdout,
                                    style::Print("Would rename "),
                                    StyledText::success_fg(),
                                    style::Print(&format!("{}", rename_result.edit_count)),
                                    StyledText::reset(),
                                    style::Print(" occurrences in "),
                                    StyledText::success_fg(),
                                    style::Print(&format!("{}", rename_result.file_count)),
                                    StyledText::reset(),
                                    style::Print(" files\n"),
                                )?;
                                result = format!("{rename_result:?}");
                            } else {
                                queue!(
                                    _stdout,
                                    style::Print("Renamed "),
                                    StyledText::success_fg(),
                                    style::Print(&format!("{}", rename_result.edit_count)),
                                    StyledText::reset(),
                                    style::Print(" occurrences in "),
                                    StyledText::success_fg(),
                                    style::Print(&format!("{}", rename_result.file_count)),
                                    StyledText::reset(),
                                    style::Print(" files\n"),
                                )?;
                                result = format!("{rename_result:?}");
                            }
                        },
                        Ok(None) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(_stdout, style::Print("\nNo symbol found at the specified location\n"),)?;
                            result = "No symbol found at the specified location".to_string();
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Failed to rename symbol: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to rename symbol: {e}");
                        },
                    }
                },
                Code::Format(params) => {
                    let request = code_agent_sdk::model::types::FormatCodeRequest {
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        tab_size: params.tab_size as u32,
                        insert_spaces: params.insert_spaces,
                    };

                    match client.format_code(request).await {
                        Ok(lines_formatted) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if lines_formatted > 0 {
                                queue!(
                                    _stdout,
                                    style::Print("\nApplied formatting to "),
                                    StyledText::brand_fg(),
                                    style::Print(&lines_formatted.to_string()),
                                    StyledText::reset(),
                                    style::Print(" lines\n"),
                                )?;
                            } else {
                                queue!(_stdout, style::Print("\nNo formatting changes needed\n"),)?;
                            }
                            result = format!("{lines_formatted:?}");
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Failed to format code: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to format code: {e}");
                        },
                    }
                },
                Code::GetDocumentSymbols(params) => {
                    let request = code_agent_sdk::model::types::GetDocumentSymbolsRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        top_level_only: params.top_level_only,
                    };

                    match client.get_document_symbols(request).await {
                        Ok(symbols) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if symbols.is_empty() {
                                queue!(
                                    _stdout,
                                    style::Print("\nNo symbols found in "),
                                    StyledText::brand_fg(),
                                    style::Print(&params.file_path),
                                    StyledText::reset(),
                                    style::Print("\n"),
                                )?;
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_symbols(&symbols, _stdout)?;
                            }
                            result = Self::format_symbols_compact(&symbols, false);
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                style::Print("\nFailed to get document symbols: "),
                                StyledText::error_fg(),
                                style::Print(&format!("{e}\n")),
                                StyledText::reset(),
                            )?;
                            result = format!("Failed to get document symbols: {e}");
                        },
                    }
                },
                Code::LookupSymbols(params) => {
                    let request = code_agent_sdk::model::types::GetSymbolsRequest {
                        symbols: params.symbols.clone(),
                        include_source: params.include_source,
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        start_row: None,
                        start_column: None,
                    };

                    match client.get_symbols(request).await {
                        Ok(symbols) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let requested_count = params.symbols.len();
                            let found_count = symbols.len();
                            let scope_info = params.file_path.as_ref().map(|p| format!(" in {}", p));

                            if symbols.is_empty() {
                                queue!(
                                    _stdout,
                                    StyledText::warning_fg(),
                                    style::Print(&format!(
                                        "\nNo symbols found (0 of {requested_count} requested){}\n",
                                        scope_info.as_deref().unwrap_or("")
                                    )),
                                    StyledText::reset(),
                                )?;
                            } else {
                                queue!(
                                    _stdout,
                                    style::Print("\nFound "),
                                    StyledText::brand_fg(),
                                    style::Print(&found_count.to_string()),
                                    StyledText::reset(),
                                    style::Print(" of "),
                                    StyledText::brand_fg(),
                                    style::Print(&requested_count.to_string()),
                                    StyledText::reset(),
                                    style::Print(" symbols:\n"),
                                )?;
                                Self::render_symbols(&symbols, _stdout)?;
                            }
                            result = Self::format_symbols_compact(&symbols, params.include_source);
                            if let Some(path) = &params.file_path {
                                result = format!("Scoped to: {}\n{}", path, result);
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Lookup failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to lookup symbols: {e}");
                        },
                    }
                },
                Code::GetDiagnostics(params) => {
                    let request = code_agent_sdk::model::types::GetDocumentDiagnosticsRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        identifier: None,
                        previous_result_id: None,
                    };

                    match client.get_document_diagnostics(request).await {
                        Ok(diagnostics) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if diagnostics.is_empty() {
                                queue!(_stdout, style::Print("\nNo diagnostics found\n"),)?;
                                result = "No diagnostics found".to_string();
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_diagnostics(&diagnostics, _stdout)?;
                                result = format!("{diagnostics:?}");
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Failed to get diagnostics: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to get diagnostics: {e}");
                        },
                    }
                },
                Code::GetHover(params) => {
                    let request = code_agent_sdk::model::types::HoverRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                    };

                    match client.hover(request).await {
                        Ok(Some(hover_info)) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(_stdout, style::Print("\n"))?;

                            // Display hover information
                            if let Some(contents) = &hover_info.content {
                                queue!(
                                    _stdout,
                                    StyledText::info_fg(),
                                    style::Print("Hover Info: "),
                                    StyledText::reset(),
                                    style::Print(contents),
                                    style::Print("\n"),
                                )?;
                            }

                            result = format!("{hover_info:?}");
                        },
                        Ok(None) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                style::Print("\nNo hover information available at "),
                                StyledText::brand_fg(),
                                style::Print(&params.file_path),
                                StyledText::reset(),
                                style::Print(":"),
                                StyledText::secondary_fg(),
                                style::Print(&format!("{}:{}", params.row, params.column)),
                                StyledText::reset(),
                                style::Print("\n"),
                            )?;
                            result = "No hover information available".to_string();
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let (display_msg, model_msg) =
                                if let Some(code_agent_sdk::CodeIntelligenceError::InvalidPosition { .. }) =
                                    e.downcast_ref::<code_agent_sdk::CodeIntelligenceError>()
                                {
                                    (
                                        format!("Invalid position: line {}, column {}", params.row, params.column),
                                        format!(
                                            "Position error at line {}, column {}. Try adjusting the position.",
                                            params.row, params.column
                                        ),
                                    )
                                } else {
                                    let msg = format!("Failed to get hover information: {e}");
                                    (msg.clone(), msg)
                                };

                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Hover Error: "),
                                StyledText::reset(),
                                style::Print(&format!("{display_msg}\n")),
                            )?;
                            result = model_msg;
                        },
                    }
                },
                Code::GetCompletions(params) => {
                    let symbol_type = params
                        .symbol_type
                        .as_ref()
                        .map(|k| k.parse::<code_agent_sdk::model::types::ApiSymbolKind>())
                        .transpose()
                        .map_err(|e| eyre::eyre!(e))?;
                    let request = code_agent_sdk::model::types::CompletionRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        trigger_character: params.trigger_character.clone(),
                        filter: params.filter.clone(),
                        symbol_type,
                        limit: Some(params.limit),
                        offset: None,
                    };

                    match client.completion(request).await {
                        Ok(Some(completion_info)) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let completions = &completion_info.items;

                            if completions.is_empty() {
                                let msg = if params.filter.is_some() {
                                    format!(
                                        "No completions found matching filter '{}'",
                                        params.filter.as_ref().unwrap()
                                    )
                                } else {
                                    "No completions available".to_string()
                                };
                                queue!(_stdout, style::Print(&format!("\n{msg}\n")))?;
                                result = msg;
                            } else {
                                let total_count = completion_info.total_count;
                                let remaining = total_count.saturating_sub(completions.len());

                                queue!(
                                    _stdout,
                                    style::Print("\nFound "),
                                    StyledText::brand_fg(),
                                    style::Print(&total_count.to_string()),
                                    StyledText::reset(),
                                    style::Print(" completions"),
                                )?;

                                if params.symbol_type.is_some() {
                                    queue!(
                                        _stdout,
                                        style::Print(" of type '"),
                                        StyledText::info_fg(),
                                        style::Print(params.symbol_type.as_ref().unwrap()),
                                        StyledText::reset(),
                                        style::Print("'"),
                                    )?;
                                }

                                if params.filter.is_some() {
                                    queue!(
                                        _stdout,
                                        style::Print(" matching '"),
                                        StyledText::info_fg(),
                                        style::Print(params.filter.as_ref().unwrap()),
                                        StyledText::reset(),
                                        style::Print("'"),
                                    )?;
                                }

                                if remaining > 0 {
                                    queue!(
                                        _stdout,
                                        style::Print(" (showing "),
                                        style::Print(&completions.len().to_string()),
                                        style::Print(")"),
                                    )?;
                                }

                                queue!(_stdout, style::Print(":\n"))?;

                                for (i, completion) in completions.iter().enumerate() {
                                    queue!(
                                        _stdout,
                                        style::Print(&format!("  {}. ", i + 1)),
                                        StyledText::brand_fg(),
                                        style::Print(&completion.label),
                                        StyledText::reset(),
                                    )?;

                                    if let Some(kind) = &completion.kind {
                                        queue!(
                                            _stdout,
                                            style::Print(" ("),
                                            StyledText::info_fg(),
                                            style::Print(kind),
                                            StyledText::reset(),
                                            style::Print(")"),
                                        )?;
                                    }

                                    if let Some(detail) = &completion.detail {
                                        queue!(_stdout, style::Print(" - "), style::Print(detail),)?;
                                    }

                                    queue!(_stdout, style::Print("\n"))?;
                                }

                                if remaining > 0 {
                                    queue!(
                                        _stdout,
                                        style::Print("  "),
                                        StyledText::secondary_fg(),
                                        style::Print(&format!("({remaining} more available)\n")),
                                        StyledText::reset(),
                                    )?;
                                }

                                result = format!("{completion_info:?}");
                                if remaining > 0 {
                                    result.push_str(&format!("\n\nNote: {remaining} more results available. Use filter or symbol_type parameters for more targeted results."));
                                }
                            }
                        },
                        Ok(None) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(_stdout, style::Print("\nNo completions available\n"),)?;
                            result = "No completions available".to_string();
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let (display_msg, model_msg) =
                                if let Some(code_agent_sdk::CodeIntelligenceError::InvalidPosition { .. }) =
                                    e.downcast_ref::<code_agent_sdk::CodeIntelligenceError>()
                                {
                                    (
                                        format!("Invalid position: line {}, column {}", params.row, params.column),
                                        format!(
                                            "Position error at line {}, column {}. Try adjusting the position.",
                                            params.row, params.column
                                        ),
                                    )
                                } else {
                                    let msg = format!("Failed to get completions: {e}");
                                    (msg.clone(), msg)
                                };

                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Completion Error: "),
                                StyledText::reset(),
                                style::Print(&format!("{display_msg}\n")),
                            )?;
                            result = model_msg;
                        },
                    }
                },
                Code::InitializeWorkspace => match client.initialize().await {
                    Ok(init_response) => {
                        Self::stop_spinner(&mut spinner, _stdout)?;
                        queue!(_stdout, style::Print("\nWorkspace initialized successfully\n"),)?;
                        result = format!("{init_response:?}");
                    },
                    Err(e) => {
                        Self::stop_spinner(&mut spinner, _stdout)?;
                        queue!(
                            _stdout,
                            style::Print("Failed to initialize workspace: "),
                            StyledText::error_fg(),
                            style::Print(&format!("{e}")),
                            StyledText::reset(),
                            style::Print("\n"),
                        )?;
                        result = format!("Failed to initialize workspace: {e}");
                    },
                },
                Code::PatternSearch(params) => {
                    let request = code_agent_sdk::PatternSearchRequest {
                        pattern: params.pattern.clone(),
                        language: params.language.clone(),
                        file_path: params.file_path.clone(),
                        limit: params.limit,
                        offset: params.offset,
                    };

                    match client.pattern_search(request).await {
                        Ok(matches) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let scope_info = params.file_path.as_ref().map(|p| format!(" (scoped to: {})", p));
                            if matches.is_empty() {
                                let msg =
                                    format!("\nNo pattern matches found{}\n", scope_info.as_deref().unwrap_or(""));
                                queue!(_stdout, style::Print(&msg),)?;
                                result = format!("No pattern matches found{}", scope_info.as_deref().unwrap_or(""));
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_pattern_matches(&matches, _stdout)?;
                                result = serde_json::to_string(&matches).unwrap_or_else(|_| format!("{matches:?}"));
                                if let Some(path) = &params.file_path {
                                    result = format!("Scoped to: {}\n{}", path, result);
                                }
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Pattern search failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Pattern search failed: {e}");
                        },
                    }
                },
                Code::PatternRewrite(params) => {
                    let request = code_agent_sdk::PatternRewriteRequest {
                        pattern: params.pattern.clone(),
                        replacement: params.replacement.clone(),
                        language: params.language.clone(),
                        file_path: params.file_path.clone(),
                        dry_run: params.dry_run,
                        limit: params.limit,
                    };

                    match client.pattern_rewrite(request).await {
                        Ok(rewrite_result) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let mode = if params.dry_run { "Dry Run" } else { "Applied" };
                            queue!(
                                _stdout,
                                style::Print("\n"),
                                StyledText::info_fg(),
                                style::Print(&format!("[{mode}] ")),
                                StyledText::reset(),
                                style::Print(&format!(
                                    "Modified {} files, {} replacements\n",
                                    rewrite_result.files_modified, rewrite_result.replacements
                                )),
                            )?;

                            // Show modified files
                            if !rewrite_result.modified_files.is_empty() {
                                queue!(_stdout, style::Print("Files modified:\n"))?;
                                for file in &rewrite_result.modified_files {
                                    queue!(_stdout, style::Print(&format!("  - {file}\n")))?;
                                }
                            }

                            if params.dry_run {
                                result = format!(
                                    "Dry Run: Would modify {} files with {} replacements.\nFiles: {}\nSet dry_run=false to apply.",
                                    rewrite_result.files_modified,
                                    rewrite_result.replacements,
                                    rewrite_result.modified_files.join(", ")
                                );
                            } else {
                                result = format!(
                                    "Applied: Modified {} files with {} replacements.\nFiles: {}",
                                    rewrite_result.files_modified,
                                    rewrite_result.replacements,
                                    rewrite_result.modified_files.join(", ")
                                );
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Pattern rewrite failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Pattern rewrite failed: {e}");
                        },
                    }
                },
                Code::GenerateCodebaseOverview(params) => {
                    // Get timeout from LSP config or use default
                    let timeout_secs = client
                        .workspace_manager
                        .config_manager
                        .all_configs()
                        .first()
                        .map_or(code_agent_sdk::model::types::DEFAULT_TIMEOUT_SECS, |c| {
                            c.request_timeout_secs
                        });

                    let request = code_agent_sdk::model::types::GenerateCodebaseOverviewRequest {
                        path: params.path.clone(),
                        timeout_secs: Some(timeout_secs),
                        token_budget: None,
                    };
                    match client.generate_codebase_overview(request).await {
                        Ok(overview) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let json = serde_json::to_string(&overview).unwrap_or_default();
                            let tokens = json.len() / 4;
                            let scope_info = params.path.as_ref().map(|p| format!(" for {}", p)).unwrap_or_default();
                            queue!(
                                _stdout,
                                style::Print("\n"),
                                StyledText::info_fg(),
                                style::Print(&format!(
                                    "[Overview{}] {} bytes (~{} tokens)\n",
                                    scope_info,
                                    json.len(),
                                    tokens
                                )),
                                StyledText::reset(),
                            )?;
                            result = if let Some(path) = &params.path {
                                format!("Scoped to: {}\n{}", path, json)
                            } else {
                                json
                            };
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            result = format!("Failed to generate codebase overview: {e}");
                        },
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
                        Ok(map) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let files = map.files_processed;
                            let tokens = map.token_count;
                            let truncated = map.truncated;
                            let scope_info = params.path.as_ref().map(|p| format!(" in {}", p)).unwrap_or_default();
                            queue!(
                                _stdout,
                                style::Print("\n"),
                                StyledText::info_fg(),
                                style::Print(&format!(
                                    "[CodebaseMap{}] {files} files, ~{tokens} tokens, truncated: {truncated}\n",
                                    scope_info
                                )),
                                StyledText::reset(),
                            )?;
                            result = serde_json::to_string(&map).unwrap_or_default();
                            if let Some(path) = &params.path {
                                result = format!("Scoped to: {}\n{}", path, result);
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            result = format!("Failed to search codebase map: {e}");
                        },
                    }
                },
            }
        } else {
            result = " Code intelligence client not initialized\n   Enable with: q settings chat.enableCodeIntelligence true".to_string();
        }
        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    /// Format symbols as compact strings: "Type name @ file:line | source"
    fn format_symbols_compact(symbols: &[SymbolInfo], include_source: bool) -> String {
        let lines: Vec<String> = symbols
            .iter()
            .map(|s| {
                let sym_type = s.symbol_type.as_deref().unwrap_or("Symbol");
                let name = if let Some(container) = &s.container_name {
                    format!("{} (in {})", s.name, container)
                } else {
                    s.name.clone()
                };
                let base = format!("{} {} @ {}:{}-{}", sym_type, name, s.file_path, s.start_row, s.end_row);
                let header = if let Some(src) = &s.source_line {
                    format!("{} | {}", base, src.trim())
                } else {
                    base
                };
                if include_source {
                    if let Some(code) = &s.source_code {
                        format!("{}\n{}", header, code)
                    } else {
                        header
                    }
                } else {
                    header
                }
            })
            .collect();
        format!("[{}]", lines.join(", "))
    }

    /// Format references as compact strings: "file:line:col | source"
    fn format_references_compact(refs: &[code_agent_sdk::model::entities::ReferenceInfo], truncated: usize) -> String {
        let lines: Vec<String> = refs
            .iter()
            .map(|r| {
                let base = format!("{}:{}:{}", r.file_path, r.start_row, r.start_column);
                if let Some(src) = &r.source_line {
                    format!("{} | {}", base, src.trim())
                } else {
                    base
                }
            })
            .collect();
        let mut out = format!("[{}]", lines.join(", "));
        if truncated > 0 {
            out.push_str(&format!(" ({} more)", truncated));
        }
        out
    }

    /// Format definition as compact string: "file:line:col | source"
    fn format_definition_compact(def: &code_agent_sdk::ApiDefinitionInfo) -> String {
        let base = format!("{}:{}:{}", def.file_path, def.start_row, def.start_column);
        if let Some(src) = &def.source_line {
            format!("{} | {}", base, src.trim())
        } else {
            base
        }
    }

    fn render_symbols(symbols: &[SymbolInfo], stdout: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        let visible_count = symbols.len().min(MAX_VISIBLE_RESULTS);
        let remaining = symbols.len().saturating_sub(MAX_VISIBLE_RESULTS);

        for (i, symbol) in symbols.iter().take(visible_count).enumerate() {
            let symbol_type = symbol.symbol_type.as_deref().unwrap_or("symbol");
            queue!(
                stdout,
                style::Print(&format!("  {}. ", i + 1)),
                StyledText::info_fg(),
                style::Print(symbol_type),
                StyledText::reset(),
                style::Print(" "),
                StyledText::brand_fg(),
                style::Print(&symbol.name),
                StyledText::reset(),
                style::Print(" at "),
                StyledText::brand_fg(),
                style::Print(&symbol.file_path),
                StyledText::reset(),
                style::Print(":"),
                StyledText::secondary_fg(),
                style::Print(&format!("{}:{}\n", symbol.start_row, symbol.start_column)),
                StyledText::reset(),
            )?;
        }

        if remaining > 0 {
            queue!(
                stdout,
                style::Print("  "),
                StyledText::secondary_fg(),
                style::Print(&format!("({remaining} more items found)\n")),
                StyledText::reset(),
            )?;
        }

        Ok(())
    }

    fn render_references(
        references: &[code_agent_sdk::model::entities::ReferenceInfo],
        stdout: &mut impl Write,
    ) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        let visible_count = references.len().min(MAX_VISIBLE_RESULTS);
        let remaining = references.len().saturating_sub(MAX_VISIBLE_RESULTS);

        for (i, reference) in references.iter().take(visible_count).enumerate() {
            queue!(
                stdout,
                style::Print(&format!("  {}. ", i + 1)),
                StyledText::brand_fg(),
                style::Print(&reference.file_path),
                StyledText::reset(),
                style::Print(":"),
                StyledText::secondary_fg(),
                style::Print(&format!("{}:{}", reference.start_row, reference.start_column)),
                StyledText::reset(),
            )?;

            // Show source line if available
            if let Some(source) = &reference.source_line {
                let trimmed = source.trim();
                if !trimmed.is_empty() {
                    queue!(stdout, style::Print(" - "), style::Print(trimmed),)?;
                }
            }

            queue!(stdout, style::Print("\n"))?;
        }

        if remaining > 0 {
            queue!(
                stdout,
                style::Print("  "),
                StyledText::secondary_fg(),
                style::Print(&format!("({remaining} more items found)\n")),
                StyledText::reset(),
            )?;
        }

        Ok(())
    }

    fn render_diagnostics(diagnostics: &[ApiDiagnosticInfo], stdout: &mut impl Write) -> Result<()> {
        use code_agent_sdk::ApiDiagnosticSeverity;
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        let visible_count = diagnostics.len().min(MAX_VISIBLE_RESULTS);
        let remaining = diagnostics.len().saturating_sub(MAX_VISIBLE_RESULTS);

        for (i, diagnostic) in diagnostics.iter().take(visible_count).enumerate() {
            queue!(stdout, style::Print(&format!("  {}. ", i + 1)))?;

            // Severity with color
            match diagnostic.severity {
                ApiDiagnosticSeverity::Error => {
                    queue!(
                        stdout,
                        StyledText::error_fg(),
                        style::Print("Error"),
                        StyledText::reset()
                    )?;
                },
                ApiDiagnosticSeverity::Warning => {
                    queue!(
                        stdout,
                        StyledText::warning_fg(),
                        style::Print("Warning"),
                        StyledText::reset()
                    )?;
                },
                ApiDiagnosticSeverity::Information => {
                    queue!(stdout, StyledText::info_fg(), style::Print("Info"), StyledText::reset())?;
                },
                ApiDiagnosticSeverity::Hint => {
                    queue!(stdout, StyledText::info_fg(), style::Print("Hint"), StyledText::reset())?;
                },
            }

            // Single line: "line X:Y: message"
            queue!(
                stdout,
                style::Print(&format!(
                    " line {}:{}: {}\n",
                    diagnostic.start_row, diagnostic.start_column, diagnostic.message
                )),
            )?;
        }

        if remaining > 0 {
            queue!(
                stdout,
                style::Print("  "),
                StyledText::secondary_fg(),
                style::Print(&format!("({remaining} more items found)\n")),
                StyledText::reset(),
            )?;
        }

        Ok(())
    }

    fn render_pattern_matches(matches: &[code_agent_sdk::PatternMatch], stdout: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        let visible_count = matches.len().min(MAX_VISIBLE_PATTERN_MATCHES);
        let remaining = matches.len().saturating_sub(MAX_VISIBLE_PATTERN_MATCHES);

        for (i, m) in matches.iter().take(visible_count).enumerate() {
            queue!(
                stdout,
                style::Print(&format!("  {}. ", i + 1)),
                StyledText::brand_fg(),
                style::Print(&m.file_path),
                StyledText::reset(),
                style::Print(":"),
                StyledText::secondary_fg(),
                style::Print(&format!("{}:{}", m.start_row, m.start_column)),
                StyledText::reset(),
            )?;

            // Show only first line of matched code, trimmed
            let first_line = m.matched_code.lines().next().unwrap_or("");
            let trimmed = first_line.trim();
            if !trimmed.is_empty() {
                queue!(stdout, style::Print(" - "), style::Print(trimmed),)?;
            }

            queue!(stdout, style::Print("\n"))?;
        }

        if remaining > 0 {
            queue!(
                stdout,
                style::Print("  "),
                StyledText::secondary_fg(),
                style::Print(&format!("({remaining} more matches found)\n")),
                StyledText::reset(),
            )?;
        }

        Ok(())
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        match self {
            Code::SearchSymbols(params) => {
                queue!(
                    output,
                    style::Print("Searching for symbols matching: "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.symbol_name)),
                    StyledText::reset(),
                )?;

                // Show applied filters like Knowledge tool
                let mut options = Vec::new();
                if let Some(limit) = params.limit {
                    options.push(format!("limit={limit}"));
                }
                if let Some(lang) = &params.language {
                    options.push(format!("language={lang}"));
                }
                if let Some(sym_type) = &params.symbol_type {
                    options.push(format!("type={sym_type}"));
                }
                if params.exact_match.unwrap_or(false) {
                    options.push("exact".to_string());
                }

                if !options.is_empty() {
                    queue!(
                        output,
                        style::Print(" ["),
                        StyledText::info_fg(),
                        style::Print(options.join(", ")),
                        StyledText::reset(),
                        style::Print("]"),
                    )?;
                }

                if let Some(path) = &params.path {
                    queue!(
                        output,
                        style::Print(" in "),
                        StyledText::secondary_fg(),
                        style::Print(path),
                        StyledText::reset(),
                    )?;
                }
            },
            Code::FindReferences(params) => {
                queue!(
                    output,
                    style::Print("Finding all references at: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::secondary_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                )?;

                let mut options = Vec::new();
                if let Some(limit) = params.limit {
                    options.push(format!("limit={limit}"));
                }
                if let Some(workspace_only) = params.workspace_only {
                    options.push(format!("workspace_only={workspace_only}"));
                }

                if !options.is_empty() {
                    queue!(
                        output,
                        style::Print(" ["),
                        StyledText::info_fg(),
                        style::Print(options.join(", ")),
                        StyledText::reset(),
                        style::Print("]"),
                    )?;
                }
            },
            Code::GotoDefinition(params) => {
                queue!(
                    output,
                    style::Print("Going to definition at: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::secondary_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                )?;

                let show_source = params.show_source.unwrap_or(true);
                if show_source {
                    queue!(output, style::Print(" (show source)"))?;
                }
            },
            Code::RenameSymbol(params) => {
                let is_dry_run = params.dry_run.unwrap_or(true);
                queue!(
                    output,
                    style::Print("Renaming symbol at: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::secondary_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                    style::Print(" to: "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.new_name)),
                    StyledText::reset(),
                )?;

                if is_dry_run {
                    queue!(
                        output,
                        style::Print(" ("),
                        StyledText::warning_fg(),
                        style::Print("Dry run"),
                        StyledText::reset(),
                        style::Print(")"),
                    )?;
                }
            },
            Code::Format(params) => {
                queue!(
                    output,
                    style::Print("Formatting code in: "),
                    StyledText::brand_fg(),
                    style::Print(params.file_path.as_deref().unwrap_or("entire workspace")),
                    StyledText::reset(),
                )?;

                // Show indentation settings only if non-default
                if params.tab_size != 4 || !params.insert_spaces {
                    let indent_type = if params.insert_spaces { "spaces" } else { "tabs" };
                    queue!(output, style::Print(&format!(" ({} {})", params.tab_size, indent_type)),)?;
                }
            },
            Code::GetDocumentSymbols(params) => {
                queue!(
                    output,
                    style::Print("Getting symbols from: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                )?;

                // Show filter if explicitly set
                if let Some(top_level_only) = params.top_level_only {
                    queue!(
                        output,
                        style::Print(" ["),
                        StyledText::info_fg(),
                        style::Print(&format!("top_level={top_level_only}")),
                        StyledText::reset(),
                        style::Print("]"),
                    )?;
                }
            },
            Code::LookupSymbols(params) => {
                queue!(output, style::Print("Looking up symbols: "), style::Print("["),)?;
                for (i, symbol) in params.symbols.iter().enumerate() {
                    if i > 0 {
                        queue!(output, style::Print(", "))?;
                    }
                    queue!(
                        output,
                        style::Print("\""),
                        StyledText::brand_fg(),
                        style::Print(symbol),
                        StyledText::reset(),
                        style::Print("\""),
                    )?;
                }
                queue!(output, style::Print("]"), StyledText::reset(),)?;

                // Show scope only if file-specific
                if let Some(file_path) = &params.file_path {
                    queue!(
                        output,
                        style::Print(" in "),
                        StyledText::secondary_fg(),
                        style::Print(file_path),
                        StyledText::reset(),
                    )?;
                }
            },
            Code::GetDiagnostics(params) => {
                queue!(
                    output,
                    style::Print("Getting diagnostics for: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                )?;
            },
            Code::GetHover(params) => {
                queue!(
                    output,
                    style::Print("Getting hover information at: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::secondary_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                )?;
            },
            Code::GetCompletions(params) => {
                queue!(
                    output,
                    style::Print("Getting completions at: "),
                    StyledText::brand_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::secondary_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                )?;

                if let Some(trigger) = &params.trigger_character {
                    queue!(
                        output,
                        style::Print(" [trigger: "),
                        StyledText::info_fg(),
                        style::Print(&format!("'{trigger}'")),
                        StyledText::reset(),
                        style::Print("]"),
                    )?;
                }
            },
            Code::InitializeWorkspace => {
                queue!(output, style::Print("Initializing workspace"),)?;
            },
            Code::PatternSearch(params) => {
                queue!(
                    output,
                    style::Print("Pattern search: "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.pattern)),
                    StyledText::reset(),
                )?;
                if let Some(ref path) = params.file_path {
                    queue!(
                        output,
                        style::Print(" in "),
                        StyledText::brand_fg(),
                        style::Print(path),
                        StyledText::reset(),
                    )?;
                }
                queue!(
                    output,
                    style::Print(" ("),
                    StyledText::info_fg(),
                    style::Print(&params.language),
                    StyledText::reset(),
                    style::Print(")"),
                )?;
            },
            Code::PatternRewrite(params) => {
                let mode = if params.dry_run { "[dry-run]" } else { "[apply]" };
                queue!(
                    output,
                    style::Print("Pattern rewrite "),
                    StyledText::info_fg(),
                    style::Print(mode),
                    StyledText::reset(),
                    style::Print(": "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.pattern)),
                    StyledText::reset(),
                    style::Print(" → "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.replacement)),
                    StyledText::reset(),
                )?;
                if let Some(ref path) = params.file_path {
                    queue!(
                        output,
                        style::Print(" in "),
                        StyledText::brand_fg(),
                        style::Print(path),
                        StyledText::reset(),
                    )?;
                }
                queue!(
                    output,
                    style::Print(" ("),
                    StyledText::info_fg(),
                    style::Print(&params.language),
                    StyledText::reset(),
                    style::Print(")"),
                )?;
            },
            Code::GenerateCodebaseOverview(params) => {
                queue!(output, style::Print("Generate codebase overview"),)?;
                if let Some(ref path) = params.path {
                    queue!(
                        output,
                        style::Print(" for "),
                        StyledText::brand_fg(),
                        style::Print(path),
                        StyledText::reset(),
                    )?;
                }
            },
            Code::SearchCodebaseMap(params) => {
                queue!(output, style::Print("Search codebase map"))?;
                if let Some(ref path) = params.file_path {
                    queue!(
                        output,
                        style::Print(" "),
                        StyledText::brand_fg(),
                        style::Print(path),
                        StyledText::reset(),
                    )?;
                }
            },
        }
        super::display_tool_use(tool, output)?;
        Ok(())
    }
}
