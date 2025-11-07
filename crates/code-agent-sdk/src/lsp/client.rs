use crate::lsp::protocol::*;
use crate::model::entities::DiagnosticEvent;
use crate::types::LanguageServerConfig;
use anyhow::Result;
use lsp_types::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex, broadcast};
use tracing::{debug, error};
use url::Url;

type ResponseCallback = Box<dyn FnOnce(Result<Value>) + Send>;

/// Language Server Protocol client for communicating with language servers
///
/// Provides a high-level interface for LSP operations including:
/// - Symbol finding and navigation
/// - Code formatting and refactoring
/// - Document lifecycle management
/// - Diagnostic notifications (push model)
pub struct LspClient {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending_requests: Arc<Mutex<HashMap<u64, ResponseCallback>>>,
    diagnostic_sender: broadcast::Sender<DiagnosticEvent>,
    next_id: Arc<Mutex<u64>>,
    config: LanguageServerConfig,
    init_result: Arc<Mutex<Option<InitializeResult>>>,
}

impl LspClient {
    /// Creates a new LSP client and starts the language server process
    ///
    /// # Arguments
    /// * `config` - Language server configuration including command and args
    ///
    /// # Returns
    /// * `Result<Self>` - New LSP client instance or error if server fails to start
    pub async fn new(config: LanguageServerConfig) -> Result<Self> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start {}: {}", config.name, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stderr"))?;

        // Create broadcast channel for diagnostics (capacity of 100 events)
        let (diagnostic_sender, _) = broadcast::channel(100);

        let client = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            diagnostic_sender,
            next_id: Arc::new(Mutex::new(1)),
            config: config.clone(),
            init_result: Arc::new(Mutex::new(None)),
        };

        // Start stderr monitoring
        let server_name = config.name.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }
                tracing::error!("LSP {} stderr: {}", server_name, line.trim());
                line.clear();
            }
        });

        client.start_message_handler(stdout).await;
        Ok(client)
    }

    /// Initializes the language server with workspace configuration
    ///
    /// # Arguments
    /// * `root_uri` - Root URI of the workspace
    ///
    /// # Returns
    /// * `Result<InitializeResult>` - Server capabilities or initialization error
    pub async fn initialize(&self, root_uri: Url) -> Result<InitializeResult> {
        tracing::debug!("Initializing LSP client for workspace: {}", root_uri);
        
        let (tx, rx) = oneshot::channel();

        let init_params = crate::lsp::LspConfig::build_initialize_params(
            root_uri.clone(),
            self.config.initialization_options.clone(),
        );

        tracing::debug!("Sending initialize request to LSP server: {}", self.config.name);
        
        self.send_request("initialize", json!(init_params), move |result| {
            let _ = tx.send(result);
        })
        .await?;

        let result = rx.await??;
        let init_result: InitializeResult = serde_json::from_value(result)?;

        // Store the initialization result
        *self.init_result.lock().await = Some(init_result.clone());

        tracing::debug!("Sending initialized notification to LSP server: {}", self.config.name);
        self.send_notification("initialized", json!({})).await?;
        
        tracing::debug!("LSP client initialization completed for: {}", self.config.name);
        Ok(init_result)
    }

    /// Subscribe to diagnostic notifications from the language server
    ///
    /// # Returns
    /// * `broadcast::Receiver<DiagnosticEvent>` - Receiver for diagnostic events
    pub fn subscribe_diagnostics(&self) -> broadcast::Receiver<DiagnosticEvent> {
        self.diagnostic_sender.subscribe()
    }

    /// Get the server capabilities from initialization
    pub async fn get_server_capabilities(&self) -> Option<ServerCapabilities> {
        self.init_result.lock().await.as_ref().map(|result| result.capabilities.clone())
    }

    /// Navigate to symbol definition
    ///
    /// # Arguments
    /// * `params` - Position and document information
    ///
    /// # Returns
    /// * `Result<Option<GotoDefinitionResponse>>` - Definition location or None
    pub async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.send_lsp_request("textDocument/definition", params)
            .await
    }

    /// Find all references to a symbol
    ///
    /// # Arguments
    /// * `params` - Symbol position and context
    ///
    /// # Returns
    /// * `Result<Option<Vec<Location>>>` - Reference locations or None
    pub async fn find_references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.send_lsp_request("textDocument/references", params)
            .await
    }

    /// Search for symbols in the workspace
    ///
    /// # Arguments
    /// * `params` - Search query and filters
    ///
    /// # Returns
    /// * `Result<Option<Vec<WorkspaceSymbol>>>` - Matching symbols or None
    pub async fn workspace_symbols(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<WorkspaceSymbol>>> {
        self.send_lsp_request("workspace/symbol", params).await
    }

    /// Get symbols in a specific document
    ///
    /// # Arguments
    /// * `params` - Document identifier
    ///
    /// # Returns
    /// * `Result<Option<DocumentSymbolResponse>>` - Document symbols or None
    pub async fn document_symbols(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.send_lsp_request("textDocument/documentSymbol", params)
            .await
    }

    /// Rename a symbol across the workspace
    ///
    /// # Arguments
    /// * `params` - Symbol position and new name
    ///
    /// # Returns
    /// * `Result<Option<WorkspaceEdit>>` - Workspace changes or None
    pub async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        tracing::trace!("LSP rename request: method=textDocument/rename, params={:?}", params);
        let result = self.send_lsp_request("textDocument/rename", params).await;
        tracing::trace!("LSP rename response: {:?}", result);
        result
    }

    /// Format a document
    ///
    /// # Arguments
    /// * `params` - Document and formatting options
    ///
    /// # Returns
    /// * `Result<Option<Vec<TextEdit>>>` - Formatting changes or None
    pub async fn format_document(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        self.send_lsp_request("textDocument/formatting", params)
            .await
    }

    /// Notify server that a document was opened
    ///
    /// # Arguments
    /// * `params` - Document URI, language ID, version, and content
    pub async fn did_open(&self, params: DidOpenTextDocumentParams) -> Result<()> {
        self.send_notification("textDocument/didOpen", json!(params))
            .await
    }

    /// Notify server that a document was closed
    ///
    /// # Arguments
    /// * `params` - Document identifier
    pub async fn did_close(&self, params: DidCloseTextDocumentParams) -> Result<()> {
        self.send_notification("textDocument/didClose", json!(params))
            .await
    }

    /// Notify server about file system changes
    ///
    /// # Arguments
    /// * `params` - File change events
    pub async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) -> Result<()> {
        self.send_notification("workspace/didChangeWatchedFiles", json!(params))
            .await
    }

    /// Notify server about created files (LSP 3.16+)
    ///
    /// # Arguments
    /// * `params` - Created file parameters
    pub async fn did_create_files(&self, params: CreateFilesParams) -> Result<()> {
        self.send_notification("workspace/didCreateFiles", json!(params))
            .await
    }

    /// Notify server about document content changes
    ///
    /// # Arguments
    /// * `params` - Document change parameters
    pub async fn did_change(&self, params: DidChangeTextDocumentParams) -> Result<()> {
        self.send_notification("textDocument/didChange", json!(params))
            .await
    }

    /// Request diagnostics for a document (pull model)
    ///
    /// # Arguments
    /// * `params` - Document diagnostic parameters
    ///
    /// # Returns
    /// * `Result<Option<DocumentDiagnosticReport>>` - Diagnostic report or None
    pub async fn document_diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<Option<DocumentDiagnosticReport>> {
        self.send_lsp_request("textDocument/diagnostic", params)
            .await
    }

    /// Generic LSP request handler with automatic response parsing
    async fn send_lsp_request<T, R>(&self, method: &str, params: T) -> Result<Option<R>>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        tracing::trace!("Sending LSP request: method={}, params={:?}", method, serde_json::to_value(&params)?);
        
        let (tx, rx) = oneshot::channel();

        self.send_request(method, json!(params), move |result| {
            tracing::trace!("LSP request callback received result: {:?}", result);
            let _ = tx.send(result);
        })
        .await?;

        tracing::trace!("Waiting for LSP response...");
        let result = rx.await??;
        tracing::trace!("Raw LSP response: {:?}", result);
        
        if result.is_null() {
            tracing::trace!("LSP response is null, returning None");
            Ok(None)
        } else {
            let parsed: R = serde_json::from_value(result)?;
            tracing::trace!("Successfully parsed LSP response");
            Ok(Some(parsed))
        }
    }

    /// Start background task to handle LSP messages from server
    async fn start_message_handler(&self, stdout: tokio::process::ChildStdout) {
        let pending_requests = self.pending_requests.clone();
        let diagnostic_sender = self.diagnostic_sender.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            while let Ok(content) = read_lsp_message(&mut reader).await {
                if let Err(e) = Self::process_message(&content, &pending_requests, &diagnostic_sender).await {
                    error!("Failed to process LSP message: {}", e);
                }
            }
            debug!("LSP connection closed");
        });
    }

    /// Process a single LSP message and handle response callbacks
    async fn process_message(
        content: &str,
        pending_requests: &Arc<Mutex<HashMap<u64, ResponseCallback>>>,
        diagnostic_sender: &broadcast::Sender<DiagnosticEvent>,
    ) -> Result<()> {
        let message = parse_lsp_message(content)?;
        
        // Debug: Log all incoming messages
        debug!("LSP message received: method={}, has_id={}", message.method, message.id.is_some());

        // Handle notifications (no ID)
        if message.id.is_none() {
            match message.method.as_str() {
                "textDocument/publishDiagnostics" => {
                    debug!("Processing publishDiagnostics notification");
                    if let Some(params) = message.params {
                        match serde_json::from_value::<PublishDiagnosticsParams>(params) {
                            Ok(diagnostic_params) => {
                                let event = DiagnosticEvent {
                                    uri: diagnostic_params.uri.to_string(),
                                    diagnostics: diagnostic_params.diagnostics,
                                };
                                
                                debug!("Sending diagnostic event: uri={}, count={}", event.uri, event.diagnostics.len());
                                
                                // Send to broadcast channel (ignore if no receivers)
                                match diagnostic_sender.send(event) {
                                    Ok(_) => debug!("Diagnostic event sent successfully"),
                                    Err(e) => error!("Failed to send diagnostic event: {}", e),
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse publishDiagnostics params: {}", e);
                            }
                        }
                    }
                }
                _ => {
                    // Other notifications - just log for now
                    debug!("Received LSP notification: {}", message.method);
                }
            }
            return Ok(());
        }

        // Handle responses (with ID)
        let Some(id) = message.id.and_then(|id| id.as_u64()) else {
            return Ok(()); // Invalid ID
        };

        let Some(callback) = pending_requests.lock().await.remove(&id) else {
            return Ok(()); // No pending request for this ID
        };

        let result = match message.error {
            Some(error) => Err(anyhow::anyhow!("LSP Error: {}", error)),
            None => Ok(message.result.unwrap_or(Value::Null)),
        };

        callback(result);
        Ok(())
    }

    /// Send LSP request with callback for response handling
    async fn send_request<F>(&self, method: &str, params: Value, callback: F) -> Result<()>
    where
        F: FnOnce(Result<Value>) + Send + 'static,
    {
        let id = {
            let mut next_id = self.next_id.lock().await;
            let current_id = *next_id;
            *next_id += 1;
            current_id
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, Box::new(callback));
        }

        let content = serde_json::to_string(&request)?;
        let mut stdin = self.stdin.lock().await;
        write_lsp_message(&mut *stdin, &content).await?;

        Ok(())
    }

    /// Apply workspace edits using LSP-compliant batch operations
    ///
    /// # Arguments
    /// * `workspace_edit` - The workspace edit to apply
    ///
    /// # Returns
    /// * `Result<bool>` - True if all edits were applied successfully
    pub async fn apply_workspace_edit(&self, workspace_edit: &WorkspaceEdit) -> Result<bool> {
        // Validate workspace edit has changes
        if workspace_edit.changes.is_none() || workspace_edit.changes.as_ref().unwrap().is_empty() {
            return Ok(false); // No changes to apply
        }

        // Apply edits with validation
        match crate::utils::apply_workspace_edit(workspace_edit) {
            Ok(()) => Ok(true),
            Err(e) => Err(anyhow::anyhow!("Workspace edit failed: {}", e)),
        }
    }

    /// Send LSP notification (no response expected)
    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&notification)?;
        let mut stdin = self.stdin.lock().await;
        write_lsp_message(&mut *stdin, &content).await?;

        Ok(())
    }
}
