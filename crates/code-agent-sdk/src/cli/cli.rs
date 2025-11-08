use clap::{Parser, Subcommand};
use code_agent_sdk::{
    CodeIntelligence, FindReferencesByLocationRequest, FindReferencesByNameRequest,
    FindSymbolsRequest, GetDocumentSymbolsRequest, GetDocumentDiagnosticsRequest, GotoDefinitionRequest,
    RenameSymbolRequest, FormatCodeRequest, OpenFileRequest, ApiDiagnosticSeverity,
};
use code_agent_sdk::model::types::ApiSymbolKind;
use code_agent_sdk::utils::logging;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "code-agent-cli")]
#[command(about = "Language-agnostic code intelligence for LLM tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Find symbols by name (fuzzy search)
    FindSymbol {
        /// Symbol name to search for
        name: String,
        /// Optional file to search within
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Optional symbol type filter
        #[arg(short, long)]
        symbol_type: Option<String>,
    },
    /// Find references to a symbol (by name or position)
    FindReferences {
        /// Symbol name to find references for
        #[arg(short, long, conflicts_with_all = ["file", "line", "column"])]
        name: Option<String>,
        /// File containing the symbol (for position-based search)
        #[arg(short, long, requires_ifs = [("line", "column"), ("column", "line")])]
        file: Option<PathBuf>,
        /// Row number (1-based)
        #[arg(short, long)]
        row: Option<u32>,
        /// Column number (0-based)
        #[arg(short, long)]
        column: Option<u32>,
    },
    /// Go to definition of a symbol
    GotoDefinition {
        /// File containing the symbol
        file: PathBuf,
        /// Row number (1-based)
        row: u32,
        /// Column number (1-based)
        column: u32,
        /// Show full source code (multi-line) instead of just declaration line
        #[arg(long)]
        show_source: bool,
    },
    /// Rename a symbol with optional dry-run
    RenameSymbol {
        /// File containing the symbol
        file: PathBuf,
        /// Row number (1-based)
        row: u32,
        /// Column number (1-based)
        column: u32,
        /// New name for the symbol
        new_name: String,
        /// Preview changes without applying (dry-run)
        #[arg(long)]
        dry_run: bool,
    },
    /// Format code in a file or workspace
    FormatCode {
        /// File to format (if not specified, formats workspace)
        file: Option<PathBuf>,
        /// Tab size for formatting
        #[arg(long, default_value = "4")]
        tab_size: u32,
        /// Use spaces instead of tabs
        #[arg(long)]
        insert_spaces: bool,
    },
    /// Detect workspace languages and available LSPs
    DetectWorkspace,
    /// Get all symbols from a document/file
    GetDocumentSymbols {
        /// Path to the file
        file: PathBuf,
    },
    /// Get diagnostics for a document (pull model)
    GetDiagnostics {
        /// Path to the file
        file: PathBuf,
        /// Optional identifier for the diagnostic request
        #[arg(long)]
        identifier: Option<String>,
    },
    /// Inspect server capabilities
    InspectCapabilities {
        /// Path to a file to determine which server to inspect
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize file logging for debugging
    if let Err(e) = logging::init_file_logging() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    } else {
        println!("📝 Logging enabled to code_intelligence.log");
    }

    let cli = Cli::parse();

    // Auto-detect workspace and initialize
    let workspace_root = std::env::current_dir()?;
    let mut code_intel = CodeIntelligence::builder()
        .auto_detect_languages()
        .workspace_root(workspace_root)
        .build()
        .expect("Failed to initialize CodeIntelligence");
    code_intel.initialize().await?;

    match cli.command {
        Commands::FindSymbol {
            name,
            file,
            symbol_type,
        } => {
            let request = FindSymbolsRequest {
                symbol_name: name,
                file_path: file,
                symbol_type: symbol_type.and_then(|s| ApiSymbolKind::from_str(&s).ok()),
                limit: None,        // Use default 20
                exact_match: false, // Enable fuzzy matching
            };

            let symbols = code_intel.find_symbols(request).await?;

            if symbols.is_empty() {
                println!("No symbols found");
            } else {
                for symbol in symbols {
                    print!(
                        "{} {} {} ({}:{} to {}:{})",
                        symbol.name,
                        symbol.symbol_type.unwrap_or_default(),
                        symbol.file_path,
                        symbol.start_row,
                        symbol.start_column,
                        symbol.end_row,
                        symbol.end_column
                    );
                    if let Some(container) = &symbol.container_name {
                        print!(" (in {})", container);
                    }
                    println!();
                    if let Some(detail) = &symbol.detail {
                        println!("  {}", detail);
                    } else if let Some(source) = &symbol.source_line {
                        println!("  {}", source);
                    }
                }
            }
        }

        Commands::FindReferences {
            name,
            file,
            row,
            column,
        } => {
            if let Some(symbol_name) = name {
                // Name-based reference search
                let request = FindReferencesByNameRequest { symbol_name };
                let references = code_intel.find_references_by_name(request).await?;
                if references.is_empty() {
                    println!("No references found");
                } else {
                    for reference in references {
                        println!(
                            "{} ({}:{} to {}:{})",
                            reference.file_path,
                            reference.start_row,
                            reference.start_column,
                            reference.end_row,
                            reference.end_column
                        );
                        if let Some(source) = &reference.source_line {
                            println!("  {}", source);
                        }
                    }
                }
            } else if let (Some(file), Some(row), Some(column)) = (file, row, column) {
                // Position-based reference search
                let request = FindReferencesByLocationRequest {
                    file_path: file,
                    row,
                    column,
                };
                let references = code_intel.find_references_by_location(request).await?;
                for reference in references {
                    println!(
                        "{} ({}:{} to {}:{})",
                        reference.file_path,
                        reference.start_row,
                        reference.start_column,
                        reference.end_row,
                        reference.end_column
                    );
                    if let Some(source) = &reference.source_line {
                        println!("  {}", source);
                    }
                }
            } else {
                println!("Either --name or all of --file, --line, --column must be provided");
            }
        }

        Commands::GotoDefinition {
            file,
            row,
            column,
            show_source,
        } => {
            match code_intel
                .goto_definition(GotoDefinitionRequest {
                    file_path: file.clone(),
                    row,
                    column,
                    show_source,
                })
                .await?
            {
                Some(definition) => {
                    println!(
                        "{} ({}:{} to {}:{})",
                        definition.file_path,
                        definition.start_row,
                        definition.start_column,
                        definition.end_row,
                        definition.end_column
                    );
                    if let Some(source) = &definition.source_line {
                        println!("  {}", source);
                    }
                }
                None => {
                    println!(
                        "No definition found at {}:{}:{}",
                        file.display(),
                        row,
                        column
                    );
                }
            }
        }

        Commands::RenameSymbol {
            file,
            row,
            column,
            new_name,
            dry_run,
        } => {
            // Open the file first to ensure LSP server processes it
            let content = std::fs::read_to_string(&file)?;
            code_intel.open_file(OpenFileRequest {
                file_path: file.clone(),
                content,
            }).await?;
            
            // Wait for LSP server to process the file
            println!("⏳ Waiting for LSP server to process file...");
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            
            let request = RenameSymbolRequest {
                file_path: file.clone(),
                row,
                column,
                new_name: new_name.clone(),
                dry_run,
            };

            match code_intel.rename_symbol(request).await? {
                Some(rename_result) => {
                    if dry_run {
                        println!(
                            "Dry-run: Would rename symbol to '{}' affecting {} files with {} edits",
                            new_name, rename_result.file_count, rename_result.edit_count
                        );
                    } else {
                        println!(
                            "Successfully renamed symbol to '{}' in {} files with {} edits",
                            new_name, rename_result.file_count, rename_result.edit_count
                        );
                    }
                }
                None => {
                    println!(
                        "Cannot rename symbol at {}:{}:{}",
                        file.display(),
                        row,
                        column
                    );
                }
            }
        }

        Commands::FormatCode {
            file,
            tab_size,
            insert_spaces,
        } => {
            let request = FormatCodeRequest {
                file_path: file.clone(),
                tab_size,
                insert_spaces,
            };

            let edit_count = code_intel.format_code(request).await?;

            if edit_count == 0 {
                println!("No formatting changes needed");
            } else {
                // Count unique lines affected by calculating from edit count
                println!("Applied formatting to {} lines", edit_count);
                println!("✅ Formatting applied successfully");
            }
        }

        Commands::DetectWorkspace => {
            let workspace_info = code_intel.detect_workspace()?;

            println!("📁 Workspace: {}", workspace_info.root_path.display());
            println!(
                "🌐 Detected Languages: {:?}",
                workspace_info.detected_languages
            );

            println!("\n🔧 Available LSPs:");
            for lsp in &workspace_info.available_lsps {
                let status = if lsp.is_available { "✅" } else { "❌" };
                println!("  {} {} ({})", status, lsp.name, lsp.languages.join(", "));
            }
        }

        Commands::GetDocumentSymbols { file } => {
            let symbols = code_intel
                .get_document_symbols(GetDocumentSymbolsRequest {
                    file_path: file.clone(),
                })
                .await?;

            if symbols.is_empty() {
                println!("No symbols found in {}", file.display());
            } else {
                println!("📄 Symbols in {}:", file.display());
                for symbol in symbols {
                    let symbol_type = symbol.symbol_type.as_deref().unwrap_or("Unknown");
                    print!(
                        "  {} {} ({}:{} to {}:{})",
                        symbol_type,
                        symbol.name,
                        symbol.start_row,
                        symbol.start_column,
                        symbol.end_row,
                        symbol.end_column
                    );
                    if let Some(container) = &symbol.container_name {
                        print!(" (in {})", container);
                    }
                    println!();
                    if let Some(detail) = &symbol.detail {
                        println!("    {}", detail);
                    } else if let Some(source) = &symbol.source_line {
                        println!("    {}", source);
                    }
                }
            }
        }

        Commands::GetDiagnostics { file, identifier } => {
            // Open the file first
            let content = std::fs::read_to_string(&file)?;
            code_intel.open_file(OpenFileRequest {
                file_path: file.clone(),
                content,
            }).await?;

            // Wait for language server to analyze the file
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let diagnostics = code_intel
                .get_document_diagnostics(GetDocumentDiagnosticsRequest {
                    file_path: file.clone(),
                    identifier,
                    previous_result_id: None,
                })
                .await?;

            if diagnostics.is_empty() {
                println!("✅ No diagnostics found in {}", file.display());
            } else {
                println!("🔍 Diagnostics in {}:", file.display());
                for diagnostic in diagnostics {
                    let severity = match diagnostic.severity {
                        ApiDiagnosticSeverity::Error => "❌ Error",
                        ApiDiagnosticSeverity::Warning => "⚠️  Warning",
                        ApiDiagnosticSeverity::Information => "ℹ️  Info",
                        ApiDiagnosticSeverity::Hint => "💡 Hint",
                    };
                    
                    println!(
                        "  {} at {}:{} - {}",
                        severity,
                        diagnostic.start_row,
                        diagnostic.start_column,
                        diagnostic.message
                    );
                    
                    if let Some(source) = &diagnostic.source {
                        println!("    Source: {}", source);
                    }
                    
                    if let Some(code) = &diagnostic.code {
                        println!("    Code: {}", code);
                    }
                }
            }
        }

        Commands::InspectCapabilities { file } => {
            let workspace_root = std::env::current_dir()?;
            let mut workspace_manager = code_intel.workspace_manager;
            
            let client = workspace_manager.get_client_for_file(&file).await?
                .ok_or_else(|| anyhow::anyhow!("No LSP client available for file: {:?}", file))?;
            
            println!("🔍 Inspecting server capabilities for: {}", file.display());
            
            if let Some(capabilities) = client.get_server_capabilities().await {
                println!("📋 Server Capabilities:");
                
                // Check diagnostic capabilities
                match &capabilities.diagnostic_provider {
                    Some(lsp_types::DiagnosticServerCapabilities::Options(opts)) => {
                        println!("  ✅ Diagnostic Provider: Supported");
                        println!("    - Inter-file dependencies: {:?}", opts.inter_file_dependencies);
                        println!("    - Workspace diagnostics: {:?}", opts.workspace_diagnostics);
                        if let Some(id) = &opts.identifier {
                            println!("    - Identifier: {}", id);
                        }
                    }
                    Some(lsp_types::DiagnosticServerCapabilities::RegistrationOptions(_)) => {
                        println!("  ✅ Diagnostic Provider: Registration Options");
                    }
                    None => {
                        println!("  ❌ Diagnostic Provider: Not supported");
                    }
                }
                
                // Check other relevant capabilities
                println!("  📄 Text Document Sync: {:?}", capabilities.text_document_sync);
                println!("  🔍 Definition Provider: {:?}", capabilities.definition_provider.is_some());
                println!("  📚 References Provider: {:?}", capabilities.references_provider.is_some());
            } else {
                println!("❌ No server capabilities available");
            }
        }
    }

    Ok(())
}


