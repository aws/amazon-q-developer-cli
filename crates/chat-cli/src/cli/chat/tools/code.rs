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
    pub exact_match: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FindReferencesParams {
    pub file_path: String,
    pub row: i32,
    pub column: i32,
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
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub previous_result_id: Option<String>,
}

fn default_tab_size() -> i32 {
    4
}
fn default_insert_spaces() -> bool {
    true
}

impl Code {
    /// Checks if the code intelligence feature is enabled
    #[allow(dead_code)]
    pub fn is_enabled(_os: &Os) -> bool {
        crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED
    }

    pub fn eval_perm(_os: &Os, _agent: &Agent) -> PermissionEvalResult {
        if !crate::feature_flags::FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            return PermissionEvalResult::Deny(vec!["Code intelligence feature is not available".to_string()]);
        }
        PermissionEvalResult::Allow
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
        code_intelligence_client: &mut Option<code_agent_sdk::sdk::client::CodeIntelligence>,
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

        if let Some(client) = code_intelligence_client {
            // Check if workspace is initialized (except for InitializeWorkspace operation)
            if !matches!(self, Code::InitializeWorkspace) && !client.is_initialized() {
                return Err(eyre::eyre!(
                    "Workspace is not initialized. Run '/code detect' to initialize the workspace."
                ));
            }

            queue!(_stdout, style::Print("\n"))?;
            let mut spinner = Some(Spinner::new(Spinners::Dots, "Loading...".to_string()));
            match self {
                Code::SearchSymbols(params) => {
                    let request = code_agent_sdk::model::types::FindSymbolsRequest {
                        symbol_name: params.symbol_name.clone(),
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        symbol_type: params.symbol_type.as_ref().and_then(|s| s.parse().ok()),
                        limit: params.limit.map(|l| l as u32),
                        exact_match: params.exact_match.unwrap_or(false),
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
                    };

                    match client.find_references_by_location(request).await {
                        Ok(references) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            if references.is_empty() {
                                queue!(_stdout, style::Print("\nNo references found\n"),)?;
                                result = "No references found".to_string();
                            } else {
                                queue!(_stdout, style::Print("\n"))?;
                                Self::render_references(&references, _stdout)?;
                                result = format!("{references:?}");
                            }
                        },
                        Err(e) => {
                            Self::stop_spinner(&mut spinner, _stdout)?;
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("Failed to find references: "),
                                StyledText::reset(),
                                style::Print(&format!("{e}\n")),
                            )?;
                            result = format!("Failed to find references: {e}");
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
                                style::Print(&context),
                                style::Print("\n"),
                            )?;

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

                            queue!(_stdout, style::Print("\n"),)?;

                            if is_dry_run {
                                queue!(
                                    _stdout,
                                    StyledText::warning_fg(),
                                    style::Print("DRY RUN: "),
                                    StyledText::reset(),
                                    style::Print(&format!(
                                        "Would rename {} occurrences in {} files\n",
                                        rename_result.edit_count, rename_result.file_count
                                    )),
                                )?;
                                result = format!("{rename_result:?}");
                            } else {
                                queue!(
                                    _stdout,
                                    StyledText::success_fg(),
                                    style::Print(&format!(
                                        "Renamed {} occurrences in {} files\n",
                                        rename_result.edit_count, rename_result.file_count
                                    )),
                                    StyledText::reset(),
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
                        identifier: params.identifier.clone(),
                        previous_result_id: params.previous_result_id.clone(),
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

        for (i, symbol) in symbols.iter().enumerate() {
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

        for (i, reference) in references.iter().enumerate() {
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
                    queue!(
                        stdout,
                        style::Print(" - "),
                        StyledText::secondary_fg(),
                        style::Print(trimmed),
                        StyledText::reset(),
                    )?;
                }
            }

            queue!(stdout, style::Print("\n"))?;
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

        for (i, diagnostic) in diagnostics.iter().enumerate() {
            // Determine severity icon and color
            let (severity_icon, severity_text) = match diagnostic.severity {
                ApiDiagnosticSeverity::Error => ("ERROR", "Error"),
                ApiDiagnosticSeverity::Warning => ("WARN", "Warning"),
                ApiDiagnosticSeverity::Information => ("INFO", "Info"),
                ApiDiagnosticSeverity::Hint => ("HINT", "Hint"),
            };

            queue!(stdout, style::Print(&format!("  {}. {} ", i + 1, severity_icon)),)?;

            // Color based on severity
            match diagnostic.severity {
                ApiDiagnosticSeverity::Error => {
                    queue!(stdout, StyledText::error_fg())?;
                },
                ApiDiagnosticSeverity::Warning => {
                    queue!(stdout, StyledText::warning_fg())?;
                },
                _ => {
                    queue!(stdout, StyledText::info_fg())?;
                },
            }

            queue!(
                stdout,
                style::Print(severity_text),
                StyledText::reset(),
                style::Print(&format!(
                    " at line {}:{}",
                    diagnostic.start_row, diagnostic.start_column
                )),
            )?;

            // Show diagnostic source if available
            if let Some(source) = &diagnostic.source {
                queue!(
                    stdout,
                    style::Print(" ["),
                    StyledText::info_fg(),
                    style::Print(source),
                    StyledText::reset(),
                    style::Print("]"),
                )?;
            }

            // Show diagnostic code if available
            if let Some(code) = &diagnostic.code {
                queue!(stdout, style::Print(" ("), style::Print(code), style::Print(")"),)?;
            }

            queue!(stdout, style::Print("\n"))?;

            // Show the diagnostic message (indented)
            queue!(
                stdout,
                style::Print("     "),
                style::Print(&diagnostic.message),
                style::Print("\n"),
            )?;

            // Show related information if available
            if !diagnostic.related_information.is_empty() {
                queue!(stdout, style::Print("     Related:\n"),)?;
                for info in &diagnostic.related_information {
                    queue!(
                        stdout,
                        style::Print("       • "),
                        StyledText::info_fg(),
                        style::Print(&info.file_path),
                        StyledText::reset(),
                        style::Print(&format!(
                            ":{}:{} - {}\n",
                            info.start_row, info.start_column, info.message
                        )),
                    )?;
                }
            }
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
                let limit = params.limit.unwrap_or(10);
                let is_exact = params.exact_match.unwrap_or(false);

                queue!(
                    output,
                    style::Print("Searching for symbols matching: "),
                    StyledText::brand_fg(),
                    style::Print(&format!("\"{}\"", params.symbol_name)),
                    StyledText::reset(),
                    style::Print(&format!(" with limit {limit}")),
                )?;

                if is_exact {
                    queue!(output, style::Print(" and exact match"))?;
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
                        style::Print("DRY RUN"),
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
            Code::InitializeWorkspace => {
                queue!(output, style::Print("Initializing workspace"),)?;
            },
        }
        super::display_tool_use(tool, output)?;
        Ok(())
    }
}
