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
    ToolInfo,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;
use crate::util::tool_permission_checker::is_tool_in_allowlist;

/// Maximum number of results to display before showing "(x more items found)"
const MAX_VISIBLE_RESULTS: usize = 20;

/// Default limit for find_references results
const DEFAULT_REFERENCES_LIMIT: usize = 500;

/// Maximum limit for find_references results
const MAX_REFERENCES_LIMIT: usize = 1000;

/// Code intelligence operations using LSP servers for symbol search, references, definitions, and
/// workspace analysis.
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
}

impl Code {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "code",
        preferred_alias: "code",
        aliases: &["code"],
    };
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchSymbolsParams {
    pub symbol_name: String,
    #[serde(default)]
    pub file_path: Option<String>,
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

fn default_tab_size() -> i32 {
    4
}

fn default_completion_limit() -> usize {
    50
}

fn default_insert_spaces() -> bool {
    true
}

impl Code {
    /// Checks if the code intelligence feature is enabled and configured
    /// Returns true only if feature flag is on AND lsp.json exists
    #[allow(dead_code)]
    pub fn is_enabled(_os: &Os) -> bool {
        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return false;
        }
        // Check if lsp.json exists (user has run /code init)
        std::env::current_dir()
            .map(|cwd| code_agent_sdk::ConfigManager::lsp_config_exists(&cwd))
            .unwrap_or(false)
    }

    pub fn eval_perm(_os: &Os, agent: &Agent) -> PermissionEvalResult {
        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return PermissionEvalResult::Deny(vec!["Code intelligence feature is not available".to_string()]);
        }

        if Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None))
        {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return Err(eyre::eyre!("Code intelligence feature is not available"));
        }

        match self {
            Code::SearchSymbols(params) => {
                if params.symbol_name.trim().is_empty() {
                    eyre::bail!("Symbol name cannot be empty");
                }
                if let Some(file_path) = &params.file_path {
                    Self::validate_file_exists(os, file_path)?;
                }
                Ok(())
            },
            Code::FindReferences(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Self::validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::GotoDefinition(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Self::validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::RenameSymbol(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Self::validate_position(params.row, params.column)?;
                if params.new_name.trim().is_empty() {
                    eyre::bail!("New name cannot be empty");
                }
                Ok(())
            },
            Code::Format(params) => {
                if let Some(file_path) = &params.file_path {
                    Self::validate_file_exists(os, file_path)?;
                }
                if params.tab_size < 1 {
                    eyre::bail!("Tab size must be >= 1 (got {})", params.tab_size);
                }
                Ok(())
            },
            Code::GetDocumentSymbols(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Ok(())
            },
            Code::LookupSymbols(params) => {
                if params.symbols.is_empty() {
                    eyre::bail!("Symbols list cannot be empty");
                }
                if let Some(file_path) = &params.file_path {
                    Self::validate_file_exists(os, file_path)?;
                }
                Ok(())
            },
            Code::GetDiagnostics(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Ok(())
            },
            Code::GetHover(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Self::validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::GetCompletions(params) => {
                Self::validate_file_exists(os, &params.file_path)?;
                Self::validate_position(params.row, params.column)?;
                Ok(())
            },
            Code::InitializeWorkspace => Ok(()),
        }
    }

    fn validate_file_exists(os: &Os, file_path: &str) -> Result<()> {
        let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, file_path);
        if !path.exists() {
            eyre::bail!("File path '{}' does not exist", file_path);
        }
        Ok(())
    }

    fn validate_position(row: i32, column: i32) -> Result<()> {
        if row < 1 {
            eyre::bail!("Row number must be >= 1 (got {})", row);
        }
        if column < 1 {
            eyre::bail!("Column number must be >= 1 (got {})", column);
        }
        Ok(())
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

            // For all operations except InitializeWorkspace, require lsp.json to exist
            if !matches!(self, Code::InitializeWorkspace) && !is_configured {
                return Ok(InvokeOutput {
                    output: OutputKind::Text(
                        "Code intelligence not configured. Only manual '/code init' command can initialize code intelligence. Please run '/code init' first.".to_string()
                    ),
                });
            }

            // Auto-initialize if json exists but workspace not initialized yet
            if is_configured
                && client.workspace_status() == code_agent_sdk::sdk::WorkspaceStatus::NotInitialized
                && let Err(e) = client.initialize().await
            {
                return Err(eyre::eyre!("Failed to initialize code intelligence: {e}"));
            }

            // Check if workspace is initialized (except for InitializeWorkspace operation)
            if !matches!(self, Code::InitializeWorkspace) {
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

                    let limit = params.limit.map(|l| (l as u32).min(MAX_RESULTS)).or(Some(MAX_RESULTS));

                    let request = code_agent_sdk::model::types::FindSymbolsRequest {
                        symbol_name: params.symbol_name.clone(),
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        symbol_type: params.symbol_type.as_ref().and_then(|s| s.parse().ok()),
                        limit,
                        language: params.language.clone(),
                        exact_match: params.exact_match.unwrap_or(false),
                        timeout_secs: None,
                    };

                    match client.find_symbols(request).await {
                        Ok(symbols) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if symbols.is_empty() {
                                queue!(_stdout, style::Print("\nNo symbols found\n"),)?;
                                result = "No symbols found".to_string();
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_symbols(&symbols, _stdout)?;
                                result = format!("{symbols:?}");
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
                                result = format!("{:?}", refs_result.references);
                                if truncated > 0 {
                                    result.push_str(&format!("\n\nNote: {truncated} more references available. Increase limit parameter to see more (max: {MAX_REFERENCES_LIMIT})."));
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

                            result = format!("{definition:?}");
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
                        dry_run: params.dry_run.unwrap_or(false),
                    };

                    match client.rename_symbol(request).await {
                        Ok(Some(rename_result)) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let is_dry_run = params.dry_run.unwrap_or(false);

                            queue!(_stdout, style::Print("\n"))?;

                            if is_dry_run {
                                queue!(
                                    _stdout,
                                    StyledText::warning_fg(),
                                    style::Print("Dry run: "),
                                    StyledText::reset(),
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
                            result = format!("{symbols:?}");
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
                        include_source: false,
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        start_row: None,
                        start_column: None,
                    };

                    match client.get_symbols(request).await {
                        Ok(symbols) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            let requested_count = params.symbols.len();
                            let found_count = symbols.len();

                            if symbols.is_empty() {
                                queue!(
                                    _stdout,
                                    StyledText::warning_fg(),
                                    style::Print(&format!("\nNo symbols found (0 of {requested_count} requested)\n")),
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
                            result = format!("{symbols:?}");
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
            }
        } else {
            result = " Code intelligence client not initialized\n   Enable with: q settings chat.enableCodeIntelligence true".to_string();
        }
        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
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
                let is_dry_run = params.dry_run.unwrap_or(false);
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
        }
        super::display_tool_use(tool, output)?;
        Ok(())
    }
}
