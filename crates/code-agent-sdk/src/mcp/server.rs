use anyhow::Result;
use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, ErrorCode, ErrorData, ListToolsResult,
        PaginatedRequestParam, ServerCapabilities, ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
    ServerHandler,
};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::model::types::*;
use crate::sdk::client::CodeIntelligence;

pub struct CodeIntelligenceServer {
    client: Arc<Mutex<Option<CodeIntelligence>>>,
}

impl CodeIntelligenceServer {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_client(&self, workspace_root: Option<PathBuf>) -> Result<(), ErrorData> {
        let mut client_guard = self.client.lock().await;

        if client_guard.is_none() {
            let workspace =
                workspace_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let mut client = CodeIntelligence::builder()
                .workspace_root(workspace)
                .auto_detect_languages()
                .build()
                .map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to create client: {}", e),
                        None,
                    )
                })?;

            client.initialize().await.map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to initialize: {}", e),
                    None,
                )
            })?;

            *client_guard = Some(client);
        }

        Ok(())
    }
}

impl Default for CodeIntelligenceServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerHandler for CodeIntelligenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some("Code Intelligence MCP server using LSP integration".to_string()),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = vec![
            Tool {
                name: "workspace_status".into(),
                description: Some("Get workspace languages and available language servers. Example: workspace_status() returns detected languages like ['rust', 'typescript'] and available LSPs.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {}
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "initialize_workspace".into(),
                description: Some("Initialize language servers for workspace (optional - auto-called when needed). Example: initialize_workspace() starts all detected language servers.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {}
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "search_symbols".into(),
                description: Some("Search for symbols using fuzzy matching. Examples: search_symbols({\"symbol_name\": \"calculateSum\"}) finds functions like 'calc_sum'. Use file_path to limit search to specific file.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "symbol_name": {
                            "type": "string",
                            "description": "Name of symbol to search for"
                        },
                        "file_path": {
                            "type": "string",
                            "description": "Optional file path to search within"
                        },
                        "symbol_type": {
                            "type": "string",
                            "description": "Optional symbol type filter (function, class, struct, enum, interface, constant, variable, module, import)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results to return",
                            "default": 10
                        },
                        "exact_match": {
                            "type": "boolean",
                            "description": "Whether to use exact matching",
                            "default": false
                        }
                    },
                    "required": ["symbol_name"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "lookup_symbols".into(),
                description: Some("Get symbols by exact names for existence checking. Example: lookup_symbols({\"symbols\": [\"main\", \"init\"]}) returns details for those specific symbols.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "symbols": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of symbol names to retrieve"
                        },
                        "file_path": {
                            "type": "string",
                            "description": "Optional file path to search within"
                        }
                    },
                    "required": ["symbols"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "get_document_symbols".into(),
                description: Some("Get all symbols from a document/file. Example: get_document_symbols({\"file_path\": \"src/main.rs\"}) returns all functions, classes, etc. in that file.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to analyze"
                        }
                    },
                    "required": ["file_path"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "goto_definition".into(),
                description: Some("Navigate to symbol definition. Example: goto_definition({\"file_path\": \"src/main.rs\", \"line\": 10, \"character\": 5}) finds where the symbol at line 10, column 5 is defined.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "File path containing the symbol"
                        },
                        "row": {
                            "type": "integer",
                            "description": "Line number (1-based) where the symbol is located"
                        },
                        "column": {
                            "type": "integer",
                            "description": "Column number (1-based) where the symbol is located"
                        },
                        "show_source": {
                            "type": "boolean",
                            "description": "Whether to include source code in the response",
                            "default": true
                        }
                    },
                    "required": ["file_path", "row", "column"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "find_references".into(),
                description: Some("Find all references to a symbol at a specific location. Example: find_references({\"file_path\": \"src/main.rs\", \"line\": 5, \"column\": 10}) finds all uses of the symbol at that position.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "File path"
                        },
                        "row": {
                            "type": "integer",
                            "description": "Line number (1-based)"
                        },
                        "column": {
                            "type": "integer",
                            "description": "Column number (1-based)"
                        }
                    },
                    "required": ["file_path", "row", "column"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "search_references".into(),
                description: Some("Find all references to a symbol by name. Example: search_references({\"symbol_name\": \"myFunction\"}) finds all places where 'myFunction' is used.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "symbol_name": {
                            "type": "string",
                            "description": "Name of the symbol to find references for"
                        }
                    },
                    "required": ["symbol_name"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "rename_symbol".into(),
                description: Some("Rename a symbol with workspace-wide updates. Example: rename_symbol({\"file_path\": \"src/main.rs\", \"start_row\": 5, \"start_column\": 10, \"new_name\": \"newName\", \"dry_run\": true}) previews renaming the symbol.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "File path containing the symbol"
                        },
                        "row": {
                            "type": "integer",
                            "description": "Start row (1-based)"
                        },
                        "column": {
                            "type": "integer",
                            "description": "Start column (1-based)"
                        },
                        "new_name": {
                            "type": "string",
                            "description": "New name for the symbol"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Preview changes without applying",
                            "default": false
                        }
                    },
                    "required": ["file_path", "row", "column", "new_name"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "format_code".into(),
                description: Some("Format code in a file using the appropriate language server. Example: format_code({\"file_path\": \"src/main.rs\", \"tab_size\": 2}) formats the file with 2-space indentation.".into()),
                input_schema: Arc::new(serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to format"
                        },
                        "tab_size": {
                            "type": "integer",
                            "description": "Tab size for formatting",
                            "default": 4
                        },
                        "insert_spaces": {
                            "type": "boolean",
                            "description": "Whether to insert spaces instead of tabs",
                            "default": true
                        }
                    },
                    "required": ["file_path"]
                })).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
        ];

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        match request.name.as_ref() {
            "workspace_status" => self.detect_workspace_tool(request.arguments).await,
            "initialize_workspace" => self.initialize_tool(request.arguments).await,
            "search_symbols" => self.find_symbols_tool(request.arguments).await,
            "lookup_symbols" => self.get_symbols_tool(request.arguments).await,
            "get_document_symbols" => self.get_document_symbols_tool(request.arguments).await,
            "goto_definition" => self.goto_definition_tool(request.arguments).await,
            "find_references" => {
                self.find_references_by_location_tool(request.arguments)
                    .await
            }
            "search_references" => self.find_references_by_name_tool(request.arguments).await,
            "rename_symbol" => self.rename_symbol_tool(request.arguments).await,
            "format_code" => self.format_code_tool(request.arguments).await,
            _ => Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "Method not found",
                None,
            )),
        }
    }
}

impl CodeIntelligenceServer {
    async fn detect_workspace_tool(
        &self,
        _arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let workspace_info = client.detect_workspace().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to detect workspace: {}", e),
                None,
            )
        })?;

        let response = json!({
            "detected_languages": workspace_info.detected_languages,
            "available_lsps": workspace_info.available_lsps.iter().map(|lsp| json!({
                "name": lsp.name,
                "languages": lsp.languages,
                "is_available": lsp.is_available
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn initialize_tool(
        &self,
        _arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let response = json!({
            "status": "initialized",
            "message": "Language servers initialized successfully"
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn find_symbols_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let symbol_name = args
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing symbol_name", None)
            })?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let symbol_type = args
            .get("symbol_type")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as u32);
        let exact_match = args
            .get("exact_match")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let request = FindSymbolsRequest {
            symbol_name: symbol_name.to_string(),
            file_path,
            symbol_type,
            limit,
            exact_match,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let symbols = client.find_symbols(request.clone()).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Find symbols failed: {}", e),
                None,
            )
        })?;

        let response = json!({
            "symbols": symbols.iter().map(|s| json!({
                "name": s.name,
                "symbol_type": s.symbol_type,
                "file_path": s.file_path,
                "start_row": s.start_row,
                "start_column": s.start_column,
                "end_row": s.end_row,
                "end_column": s.end_column,
                "detail": s.detail
            })).collect::<Vec<_>>(),
            "search_context": {
                "symbol_name": request.symbol_name,
                "total_found": symbols.len(),
                "limit_applied": request.limit,
                "scope": if request.file_path.is_some() { 
                    format!("file: {}", request.file_path.as_ref().unwrap().display()) 
                } else { 
                    "workspace".to_string() 
                }
            }
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn get_symbols_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let symbols = args
            .get("symbols")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing symbols array", None)
            })?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);

        let request = GetSymbolsRequest {
            symbols,
            include_source: false,
            file_path,
            start_row: None,
            start_column: None,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let symbols = client.get_symbols(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Get symbols failed: {}", e),
                None,
            )
        })?;

        let response = json!({
            "symbols": symbols.iter().map(|s| json!({
                "name": s.name,
                "symbol_type": s.symbol_type,
                "file_path": s.file_path,
                "start_row": s.start_row,
                "start_column": s.start_column,
                "end_row": s.end_row,
                "end_column": s.end_column,
                "detail": s.detail
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn get_document_symbols_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing file_path", None))?;

        let request = GetDocumentSymbolsRequest {
            file_path: PathBuf::from(file_path),
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let symbols = client.get_document_symbols(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Get document symbols failed: {}", e),
                None,
            )
        })?;

        let response = json!({
            "symbols": symbols.iter().map(|s| json!({
                "name": s.name,
                "symbol_type": s.symbol_type,
                "file_path": s.file_path,
                "start_row": s.start_row,
                "start_column": s.start_column,
                "end_row": s.end_row,
                "end_column": s.end_column,
                "detail": s.detail
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn goto_definition_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing file_path", None))?;
        let row = args
            .get("row")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing row", None))?
            as u32;
        let column = args
            .get("column")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing column", None))?
            as u32;
        let show_source = args
            .get("show_source")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let request = GotoDefinitionRequest {
            file_path: PathBuf::from(file_path),
            row,
            column,
            show_source,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let definition = client.goto_definition(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Goto definition failed: {}", e),
                None,
            )
        })?;

        let response = if let Some(def) = definition {
            json!({
                "found": true,
                "file_path": def.file_path,
                "start_row": def.start_row,
                "start_column": def.start_column,
                "end_row": def.end_row,
                "end_column": def.end_column,
                "source_line": def.source_line
            })
        } else {
            json!({
                "found": false
            })
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn find_references_by_location_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing file_path", None))?;
        let row = args
            .get("row")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing row", None))?
            as u32;
        let column = args
            .get("column")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing column", None))?
            as u32;

        let request = FindReferencesByLocationRequest {
            file_path: PathBuf::from(file_path),
            row,
            column,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let references = client
            .find_references_by_location(request)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Find references failed: {}", e),
                    None,
                )
            })?;

        let response = json!({
            "references": references.iter().map(|r| json!({
                "file_path": r.file_path,
                "start_row": r.start_row,
                "start_column": r.start_column,
                "end_row": r.end_row,
                "end_column": r.end_column
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn find_references_by_name_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let symbol_name = args
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing symbol_name", None)
            })?;

        let request = FindReferencesByNameRequest {
            symbol_name: symbol_name.to_string(),
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let references = client.find_references_by_name(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Find references by name failed: {}", e),
                None,
            )
        })?;

        let response = json!({
            "references": references.iter().map(|r| json!({
                "file_path": r.file_path,
                "start_row": r.start_row,
                "start_column": r.start_column,
                "end_row": r.end_row,
                "end_column": r.end_column
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn rename_symbol_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing file_path", None))?;
        let row = args
            .get("row")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing row", None))?
            as u32;
        let column = args
            .get("column")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing column", None)
            })? as u32;
        let new_name = args
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing new_name", None))?;
        let dry_run = args
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let request = RenameSymbolRequest {
            file_path: PathBuf::from(file_path),
            row,
            column,
            new_name: new_name.to_string(),
            dry_run,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let workspace_edit = client.rename_symbol(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Rename symbol failed: {}", e),
                None,
            )
        })?;

        let response = if let Some(result) = workspace_edit {
            let message = if dry_run {
                format!("Dry-run: Would rename to '{}' ({} edits in {} files)", 
                    new_name, result.edit_count, result.file_count)
            } else {
                format!("Renamed to '{}' ({} edits in {} files)", 
                    new_name, result.edit_count, result.file_count)
            };
            
            json!({
                "success": true,
                "message": message,
                "file_count": result.file_count,
                "edit_count": result.edit_count
            })
        } else {
            json!({
                "success": false,
                "message": "Rename not possible"
            })
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }

    async fn format_code_tool(
        &self,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_client(None).await?;

        let args = arguments
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing arguments", None))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::new(ErrorCode::INVALID_PARAMS, "Missing file_path", None))?;
        let tab_size = args.get("tab_size").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
        let insert_spaces = args
            .get("insert_spaces")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let request = FormatCodeRequest {
            file_path: Some(PathBuf::from(file_path)),
            tab_size,
            insert_spaces,
        };

        let mut client_guard = self.client.lock().await;
        let client = client_guard.as_mut().unwrap();

        let edits = client.format_code(request).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Format code failed: {}", e),
                None,
            )
        })?;

        let message = if edits > 0 {
            format!("Code formatted successfully ({} edits applied)", edits)
        } else {
            "No formatting changes needed".to_string()
        };

        let response = json!({
            "success": true,
            "message": message,
            "formatted": edits
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap(),
        )]))
    }
}
