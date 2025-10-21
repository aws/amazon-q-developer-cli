use std::io::Write;

use eyre::Result;
use serde::Deserialize;

use super::{InvokeOutput, OutputKind};
use crate::cli::agent::{Agent, PermissionEvalResult};
use crate::cli::experiment::experiment_manager::{ExperimentManager, ExperimentName};
use crate::os::Os;

/// Code intelligence operations using LSP servers for symbol search, references, definitions, and workspace analysis.
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
    InitializeWorkspace,
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
    pub symbol_name: String,
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

fn default_tab_size() -> i32 { 4 }
fn default_insert_spaces() -> bool { true }

impl Code {
    /// Checks if the code intelligence feature is enabled in settings
    pub fn is_enabled(os: &Os) -> bool {
        ExperimentManager::is_enabled(os, ExperimentName::CodeIntelligence)
    }

    pub fn eval_perm(os: &Os, _agent: &Agent) -> PermissionEvalResult {
        if !ExperimentManager::is_enabled(os, ExperimentName::CodeIntelligence) {
            return PermissionEvalResult::Deny(vec!["Code intelligence is disabled. Enable it with: q settings chat.enableCodeIntelligence true".to_string()]);
        }
        PermissionEvalResult::Allow
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        if !ExperimentManager::is_enabled(os, ExperimentName::CodeIntelligence) {
            return Err(eyre::eyre!("Code intelligence is disabled. Enable it with: q settings chat.enableCodeIntelligence true"));
        }
        
        match self {
            Code::SearchSymbols(params) => {
                if params.symbol_name.trim().is_empty() {
                    eyre::bail!("Symbol name cannot be empty");
                }
                if let Some(file_path) = &params.file_path {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, file_path);
                    if !path.exists() {
                        eyre::bail!("File path '{}' does not exist", file_path);
                    }
                }
                Ok(())
            }
            Code::FindReferences(params) => {
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &params.file_path);
                if !path.exists() {
                    eyre::bail!("File path '{}' does not exist", params.file_path);
                }
                if params.row < 1 {
                    eyre::bail!("Row number must be >= 1 (got {})", params.row);
                }
                if params.column < 1 {
                    eyre::bail!("Column number must be >= 1 (got {})", params.column);
                }
                Ok(())
            }
            Code::GotoDefinition(params) => {
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &params.file_path);
                if !path.exists() {
                    eyre::bail!("File path '{}' does not exist", params.file_path);
                }
                if params.row < 1 {
                    eyre::bail!("Row number must be >= 1 (got {})", params.row);
                }
                if params.column < 1 {
                    eyre::bail!("Column number must be >= 1 (got {})", params.column);
                }
                Ok(())
            }
            Code::RenameSymbol(params) => {
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &params.file_path);
                if !path.exists() {
                    eyre::bail!("File path '{}' does not exist", params.file_path);
                }
                if params.row < 1 {
                    eyre::bail!("Row number must be >= 1 (got {})", params.row);
                }
                if params.column < 1 {
                    eyre::bail!("Column number must be >= 1 (got {})", params.column);
                }
                if params.new_name.trim().is_empty() {
                    eyre::bail!("New name cannot be empty");
                }
                Ok(())
            }
            Code::Format(params) => {
                if let Some(file_path) = &params.file_path {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, file_path);
                    if !path.exists() {
                        eyre::bail!("File path '{}' does not exist", file_path);
                    }
                }
                if params.tab_size < 1 {
                    eyre::bail!("Tab size must be >= 1 (got {})", params.tab_size);
                }
                Ok(())
            }
            Code::GetDocumentSymbols(params) => {
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &params.file_path);
                if !path.exists() {
                    eyre::bail!("File path '{}' does not exist", params.file_path);
                }
                Ok(())
            }
            Code::LookupSymbols(params) => {
                if params.symbols.is_empty() {
                    eyre::bail!("Symbols list cannot be empty");
                }
                if let Some(file_path) = &params.file_path {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, file_path);
                    if !path.exists() {
                        eyre::bail!("File path '{}' does not exist", file_path);
                    }
                }
                Ok(())
            }
            Code::InitializeWorkspace => Ok(()),
        }
    }

    pub async fn invoke(
        &self,
        _os: &Os,
        _stdout: &mut impl Write,
        code_intelligence_client: &mut Option<code_agent_sdk::sdk::client::CodeIntelligence>,
    ) -> Result<InvokeOutput> {
        use crossterm::{queue, style};
        use crate::theme::StyledText;
        
        #[allow(unused_assignments)]
        let mut result = String::new();
        
        if let Some(client) = code_intelligence_client {
            match self {
                Code::SearchSymbols(params) => {
                    let request = code_agent_sdk::model::types::FindSymbolsRequest {
                        symbol_name: params.symbol_name.clone(),
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        symbol_type: params.symbol_type.as_ref()
                            .and_then(|s| s.parse().ok()),
                        limit: params.limit.map(|l| l as u32),
                        exact_match: params.exact_match.unwrap_or(false),
                    };
                    
                    match client.find_symbols(request).await {
                        Ok(symbols) => {
                            if symbols.is_empty() {
                                queue!(
                                    _stdout,
                                    style::Print("\n🔍 No symbols found matching \""),
                                    StyledText::warning_fg(),
                                    style::Print(&params.symbol_name),
                                    StyledText::reset(),
                                    style::Print("\"\n"),
                                )?;
                            } else {
                                queue!(
                                    _stdout,
                                    style::Print("\n🔍 Found "),
                                    StyledText::success_fg(),
                                    style::Print(&symbols.len().to_string()),
                                    StyledText::reset(),
                                    style::Print(" symbol(s):\n"),
                                )?;
                                for (i, symbol) in symbols.iter().enumerate() {
                                    let symbol_type = symbol.symbol_type.as_deref().unwrap_or("symbol");
                                    queue!(
                                        _stdout,
                                        style::Print(&format!("  {}. ", i + 1)),
                                        StyledText::info_fg(),
                                        style::Print(symbol_type),
                                        StyledText::reset(),
                                        style::Print(" "),
                                        StyledText::success_fg(),
                                        style::Print(&symbol.name),
                                        StyledText::reset(),
                                        style::Print(&format!(" at {}:{}:{}\n", 
                                            symbol.file_path, 
                                            symbol.start_row, 
                                            symbol.start_column)),
                                    )?;
                                }
                            }
                            result = format!("{:?}", symbols);
                        }
                        Err(e) => {
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("❌ Search failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{}\n", e)),
                            )?;
                            result = format!("❌ Failed to search symbols: {}", e);
                        }
                    }
                }
                Code::FindReferences(params) => {
                    let request = code_agent_sdk::model::types::FindReferencesByLocationRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                    };
                    
                    match client.find_references_by_location(request).await {
                        Ok(references) => {
                            if references.is_empty() {
                                queue!(
                                    _stdout,
                                    style::Print("\n🔗 No references found for \""),
                                    StyledText::warning_fg(),
                                    style::Print(&params.symbol_name),
                                    StyledText::reset(),
                                    style::Print("\"\n"),
                                )?;
                            } else {
                                queue!(
                                    _stdout,
                                    style::Print("\n🔗 Found "),
                                    StyledText::success_fg(),
                                    style::Print(&references.len().to_string()),
                                    StyledText::reset(),
                                    style::Print(" reference(s) to \""),
                                    StyledText::success_fg(),
                                    style::Print(&params.symbol_name),
                                    StyledText::reset(),
                                    style::Print("\":\n"),
                                )?;
                                for (i, reference) in references.iter().enumerate() {
                                    queue!(
                                        _stdout,
                                        style::Print(&format!("  {}. ", i + 1)),
                                        StyledText::info_fg(),
                                        style::Print(&reference.file_path),
                                        StyledText::reset(),
                                        style::Print(&format!(":{}:{}\n", 
                                            reference.start_row, 
                                            reference.start_column)),
                                    )?;
                                }
                            }
                            result = format!("{:?}", references);
                        }
                        Err(e) => {
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("❌ Failed to find references: "),
                                StyledText::reset(),
                                style::Print(&format!("{}\n", e)),
                            )?;
                            result = format!("❌ Failed to find references: {}", e);
                        }
                    }
                }
                Code::GotoDefinition(params) => {
                    let request = code_agent_sdk::model::types::GotoDefinitionRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        show_source: params.show_source.unwrap_or(true),
                    };
                    
                    match client.goto_definition(request).await {
                        Ok(definition) => {
                            result = format!("{:?}", definition);
                        }
                        Err(e) => {
                            result = format!("❌ Failed to find definition: {}", e);
                        }
                    }
                }
                Code::RenameSymbol(params) => {
                    let request = code_agent_sdk::model::types::RenameSymbolRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                        row: params.row as u32,
                        column: params.column as u32,
                        new_name: params.new_name.clone(),
                        dry_run: params.dry_run.unwrap_or(false),
                    };
                    
                    match client.rename_symbol(request).await {
                        Ok(rename_result) => {
                            result = format!("{:?}", rename_result);
                        }
                        Err(e) => {
                            result = format!("❌ Failed to rename symbol: {}", e);
                        }
                    }
                }
                Code::Format(params) => {
                    let request = code_agent_sdk::model::types::FormatCodeRequest {
                        file_path: params.file_path.as_ref().map(std::path::PathBuf::from),
                        tab_size: params.tab_size as u32,
                        insert_spaces: params.insert_spaces,
                    };
                    
                    match client.format_code(request).await {
                        Ok(format_result) => {
                            result = format!("{:?}", format_result);
                        }
                        Err(e) => {
                            result = format!("❌ Failed to format code: {}", e);
                        }
                    }
                }
                Code::GetDocumentSymbols(params) => {
                    let request = code_agent_sdk::model::types::GetDocumentSymbolsRequest {
                        file_path: std::path::PathBuf::from(&params.file_path),
                    };
                    
                    match client.get_document_symbols(request).await {
                        Ok(symbols) => {
                            result = format!("{:?}", symbols);
                        }
                        Err(e) => {
                            result = format!("❌ Failed to get document symbols: {}", e);
                        }
                    }
                }
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
                            if symbols.is_empty() {
                                queue!(
                                    _stdout,
                                    StyledText::warning_fg(),
                                    style::Print("🔎 No symbols found\n"),
                                    StyledText::reset(),
                                )?;
                            } else {
                                queue!(
                                    _stdout,
                                    style::Print("\n🔎 Found "),
                                    StyledText::success_fg(),
                                    style::Print(&symbols.len().to_string()),
                                    StyledText::reset(),
                                    style::Print(" symbol(s):\n"),
                                )?;
                                for (i, symbol) in symbols.iter().enumerate() {
                                    let symbol_type = symbol.symbol_type.as_deref().unwrap_or("symbol");
                                    queue!(
                                        _stdout,
                                        style::Print(&format!("  {}. ", i + 1)),
                                        StyledText::info_fg(),
                                        style::Print(symbol_type),
                                        StyledText::reset(),
                                        style::Print(" "),
                                        StyledText::success_fg(),
                                        style::Print(&symbol.name),
                                        StyledText::reset(),
                                        style::Print(&format!(" at {}:{}:{}\n", 
                                            symbol.file_path, 
                                            symbol.start_row, 
                                            symbol.start_column)),
                                    )?;
                                }
                            }
                            result = format!("{:?}", symbols);
                        }
                        Err(e) => {
                            queue!(
                                _stdout,
                                StyledText::error_fg(),
                                style::Print("❌ Lookup failed: "),
                                StyledText::reset(),
                                style::Print(&format!("{}\n", e)),
                            )?;
                            result = format!("❌ Failed to lookup symbols: {}", e);
                        }
                    }
                }
                Code::InitializeWorkspace => {
                    match client.initialize().await {
                        Ok(init_response) => {
                            result = format!("{:?}", init_response);
                        }
                        Err(e) => {
                            result = format!("❌ Failed to initialize workspace: {}", e);
                        }
                    }
                }
            }
        } else {
            result = "⚠️  Code intelligence client not initialized\n   Enable with: q settings chat.enableCodeIntelligence true".to_string();
        }
        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        use crossterm::{queue, style};
        use crate::theme::StyledText;
        
        match self {
            Code::SearchSymbols(params) => {
                queue!(
                    output,
                    style::Print("Searching for symbols matching: "),
                    StyledText::success_fg(),
                    style::Print(&params.symbol_name),
                    StyledText::reset(),
                )?;
                if let Some(file_path) = &params.file_path {
                    queue!(
                        output,
                        style::Print(" in file: "),
                        StyledText::info_fg(),
                        style::Print(file_path),
                        StyledText::reset(),
                    )?;
                }
            }
            Code::FindReferences(params) => {
                queue!(
                    output,
                    style::Print("🔗 Finding references for "),
                    StyledText::success_fg(),
                    style::Print(&format!("\"{}\"", params.symbol_name)),
                    StyledText::reset(),
                    style::Print(" at "),
                    StyledText::info_fg(),
                    style::Print(&format!("{}:{}:{}", params.file_path, params.row, params.column)),
                    StyledText::reset(),
                )?;
            }
            Code::GotoDefinition(params) => {
                queue!(
                    output,
                    style::Print("🎯 Going to definition at: "),
                    StyledText::success_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::info_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                )?;
            }
            Code::RenameSymbol(params) => {
                queue!(
                    output,
                    style::Print("✏️  Renaming symbol at: "),
                    StyledText::success_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                    style::Print(":"),
                    StyledText::info_fg(),
                    style::Print(&format!("{}:{}", params.row, params.column)),
                    StyledText::reset(),
                    style::Print(" to: "),
                    StyledText::success_fg(),
                    style::Print(&params.new_name),
                    StyledText::reset(),
                )?;
            }
            Code::Format(params) => {
                if let Some(file_path) = &params.file_path {
                    queue!(
                        output,
                        style::Print("🎨 Formatting code in: "),
                        StyledText::success_fg(),
                        style::Print(file_path),
                        StyledText::reset(),
                    )?;
                } else {
                    queue!(
                        output,
                        style::Print("🎨 Formatting workspace code"),
                    )?;
                }
            }
            Code::GetDocumentSymbols(params) => {
                queue!(
                    output,
                    style::Print("📄 Getting symbols from: "),
                    StyledText::success_fg(),
                    style::Print(&params.file_path),
                    StyledText::reset(),
                )?;
            }
            Code::LookupSymbols(params) => {
                queue!(
                    output,
                    style::Print("🔎 Looking up symbols: "),
                    StyledText::info_fg(),
                    style::Print("["),
                )?;
                for (i, symbol) in params.symbols.iter().enumerate() {
                    if i > 0 {
                        queue!(output, style::Print(", "))?;
                    }
                    queue!(
                        output,
                        style::Print("\""),
                        StyledText::success_fg(),
                        style::Print(symbol),
                        StyledText::info_fg(),
                        style::Print("\""),
                    )?;
                }
                queue!(
                    output,
                    style::Print("]"),
                    StyledText::reset(),
                )?;
            }
            Code::InitializeWorkspace => {
                queue!(
                    output,
                    style::Print("🚀 Initializing workspace"),
                )?;
            }
        }
        Ok(())
    }
}
