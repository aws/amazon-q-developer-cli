use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Result;
use lsp_types::*;
use serde_json::{
    Value,
    json,
};
use tokio::io::{
    AsyncBufReadExt,
    BufReader,
};
use tokio::process::Command;
use tokio::sync::{
    Mutex,
    broadcast,
    oneshot,
};
use tracing::{
    debug,
    error,
};
use url::Url;

use crate::lsp::protocol::*;
use crate::model::entities::DiagnosticEvent;
use crate::types::LanguageServerConfig;

type ResponseCallback = Box<dyn FnOnce(Result<Value>) + Send>;

/// Status of an LSP server
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspStatus {
    /// Server process started, awaiting initialization
    Registered,
    /// Initialize request sent, waiting for response
    Initializing,
    /// Successfully initialized and ready
    Initialized,
    /// Initialization failed with error message
    Failed(String),
}

/// Language Server Protocol client for communicating with language servers
///
/// Provides a high-level interface for LSP operations including:
/// - Symbol finding and navigation
/// - Code formatting and refactoring
/// - Document lifecycle management
/// - Diagnostic notifications (push model)
pub struct LspClient {
    pub(crate) stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pub(crate) pending_requests: Arc<Mutex<HashMap<String, ResponseCallback>>>,
    diagnostic_sender: broadcast::Sender<DiagnosticEvent>,
    pub(crate) next_id: Arc<Mutex<u64>>,
    pub(crate) config: LanguageServerConfig,
    pub(crate) status: Arc<Mutex<LspStatus>>,
    pub(crate) init_result: Arc<Mutex<Option<InitializeResult>>>,
    pub(crate) child: Arc<Mutex<Option<tokio::process::Child>>>,
    /// Captures the last error from stderr for better error messages
    pub(crate) last_stderr_error: Arc<Mutex<Option<String>>>,
    /// Timestamp when client was created
    pub(crate) init_start: std::time::Instant,
    /// Duration of initialization (set when completed)
    pub(crate) init_duration: Arc<Mutex<Option<std::time::Duration>>>,
    /// Whether server supports pull diagnostics (textDocument/diagnostic)
    pub(crate) supports_pull_diagnostics: Arc<Mutex<bool>>,
}

impl LspClient {
    /// Creates a new LSP client and starts the language server process
    ///
    /// # Arguments
    /// * `config` - Language server configuration including command and args
    /// * `workspace_root` - Root directory for the workspace (sets CWD for LSP process)
    ///
    /// # Returns
    /// * `Result<Self>` - New LSP client instance or error if server fails to start
    pub async fn new(config: LanguageServerConfig, workspace_root: &std::path::Path) -> Result<Self> {
        #[cfg(unix)]
        #[allow(unused_imports)]
        use std::os::unix::process::CommandExt;

        tracing::info!(
            "Spawning LSP server '{}' with CWD: {}",
            config.name,
            workspace_root.display()
        );

        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set process group to prevent LSP from receiving SIGINT when parent is interrupted
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command
            .spawn()
            .map_err(|e| crate::error::CodeIntelligenceError::init_failed(&config.name, &e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| crate::error::CodeIntelligenceError::init_failed(&config.name, "Failed to capture stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            crate::error::CodeIntelligenceError::init_failed(&config.name, "Failed to capture stdout")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            crate::error::CodeIntelligenceError::init_failed(&config.name, "Failed to capture stderr")
        })?;

        // Create broadcast channel for diagnostics (capacity of 100 events)
        let (diagnostic_sender, _) = broadcast::channel(100);

        let last_stderr_error = Arc::new(Mutex::new(None));

        let client = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            diagnostic_sender,
            next_id: Arc::new(Mutex::new(1_000_000)),
            config: config.clone(),
            status: Arc::new(Mutex::new(LspStatus::Registered)),
            init_result: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(Some(child))),
            last_stderr_error: last_stderr_error.clone(),
            init_start: std::time::Instant::now(),
            init_duration: Arc::new(Mutex::new(None)),
            supports_pull_diagnostics: Arc::new(Mutex::new(false)),
        };

        // Start stderr monitoring - LSPs often write info/debug to stderr
        let server_name = config.name.clone();
        let stderr_capture = last_stderr_error;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                // Always log stderr output since LSPs write important info there
                tracing::error!("LSP {server_name} stderr: {trimmed}");
                // Capture error messages for better error reporting
                if trimmed.contains("error:") || trimmed.contains("Error:") {
                    *stderr_capture.lock().await = Some(trimmed.to_string());
                }
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
        // Set status to Initializing
        *self.status.lock().await = LspStatus::Initializing;

        tracing::info!("Initializing LSP client for workspace: {}", root_uri);

        // Log the LSP server configuration
        tracing::info!("LSP Server Configuration:");
        tracing::info!("  Name: {}", self.config.name);
        tracing::info!("  Command: {}", self.config.command);
        tracing::info!("  Args: {:?}", self.config.args);
        tracing::info!("  File extensions: {:?}", self.config.file_extensions);
        if let Some(ref init_opts) = self.config.initialization_options {
            tracing::info!(
                "  Initialization Options: {}",
                serde_json::to_string_pretty(init_opts).unwrap_or_else(|_| "Failed to serialize".to_string())
            );
        } else {
            tracing::info!("  Initialization Options: None");
        }

        let (tx, rx) = oneshot::channel();

        let init_params = crate::lsp::LspConfig::build_initialize_params(
            root_uri.clone(),
            Some(&self.config),
            self.config.initialization_options.clone(),
        );

        // Log the initialization parameters being sent
        tracing::info!(
            "Initialize params workspace_folders: {:?}",
            init_params.workspace_folders
        );
        tracing::info!(
            "Initialize params initialization_options: {:?}",
            init_params.initialization_options
        );
        tracing::info!(
            "Initialize params (full): {}",
            serde_json::to_string_pretty(&init_params).unwrap_or_else(|_| "Failed to serialize".to_string())
        );

        tracing::info!("Sending initialize request to LSP server: {}", self.config.name);

        if let Err(e) = self
            .send_request("initialize", json!(init_params), move |result| {
                let _ = tx.send(result);
            })
            .await
        {
            let err_msg = e.to_string();
            *self.status.lock().await = LspStatus::Failed(err_msg.clone());
            return Err(e);
        }

        let result = match rx.await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                let err_str = e.to_string();
                let err = if err_str.contains("Broken pipe") || err_str.contains("os error 32") {
                    crate::error::CodeIntelligenceError::connection_closed(
                        &self.config.name,
                        "Server process terminated during initialization",
                    )
                } else {
                    crate::error::CodeIntelligenceError::init_failed(&self.config.name, &err_str)
                };
                *self.status.lock().await = LspStatus::Failed(err.to_string());
                return Err(err.into());
            },
            Err(_) => {
                let err = crate::error::CodeIntelligenceError::connection_closed(
                    &self.config.name,
                    "Response channel closed during initialization",
                );
                *self.status.lock().await = LspStatus::Failed(err.to_string());
                return Err(err.into());
            },
        };

        let init_result: InitializeResult = match serde_json::from_value(result) {
            Ok(r) => r,
            Err(e) => {
                let err_msg = format!("Failed to parse initialize response: {e}");
                *self.status.lock().await = LspStatus::Failed(err_msg.clone());
                return Err(anyhow::anyhow!(err_msg));
            },
        };

        // Log the server capabilities returned
        tracing::info!("LSP Server Capabilities for {}:", self.config.name);
        if let Some(ref server_info) = init_result.server_info {
            tracing::info!(
                "  Server Info: {} {}",
                server_info.name,
                server_info.version.as_ref().unwrap_or(&"unknown".to_string())
            );
        }
        tracing::info!(
            "  Capabilities: {}",
            serde_json::to_string_pretty(&init_result.capabilities)
                .unwrap_or_else(|_| "Failed to serialize".to_string())
        );

        // Store the initialization result and update status
        *self.init_result.lock().await = Some(init_result.clone());
        *self.status.lock().await = LspStatus::Initialized;
        *self.init_duration.lock().await = Some(self.init_start.elapsed());

        tracing::info!("Sending initialized notification to LSP server: {}", self.config.name);
        self.send_notification("initialized", json!({})).await?;

        // Send didChangeConfiguration to kick Pyright (fixes 1.1.407 hang)
        tracing::debug!(
            "Sending workspace/didChangeConfiguration to LSP server: {}",
            self.config.name
        );
        self.send_notification("workspace/didChangeConfiguration", json!({"settings": {}}))
            .await?;

        tracing::info!("LSP client initialization completed for: {}", self.config.name);
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
        self.init_result
            .lock()
            .await
            .as_ref()
            .map(|result| result.capabilities.clone())
    }

    /// Check if the LSP server has been successfully initialized
    pub fn is_initialized(&self) -> bool {
        self.status
            .try_lock()
            .map(|g| *g == LspStatus::Initialized)
            .unwrap_or(false)
    }

    /// Check if server supports pull diagnostics (textDocument/diagnostic)
    pub fn supports_pull_diagnostics(&self) -> bool {
        self.supports_pull_diagnostics.try_lock().map(|g| *g).unwrap_or(false)
    }

    /// Get the current status of the LSP server
    pub fn status(&self) -> LspStatus {
        self.status
            .try_lock()
            .map(|g| g.clone())
            .unwrap_or(LspStatus::Registered)
    }

    /// Navigate to symbol definition
    ///
    /// # Arguments
    /// * `params` - Position and document information
    ///
    /// # Returns
    /// * `Result<Option<GotoDefinitionResponse>>` - Definition location or None
    pub async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        self.send_lsp_request("textDocument/definition", params).await
    }

    /// Find all references to a symbol
    ///
    /// # Arguments
    /// * `params` - Symbol position and context
    ///
    /// # Returns
    /// * `Result<Option<Vec<Location>>>` - Reference locations or None
    pub async fn find_references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.send_lsp_request("textDocument/references", params).await
    }

    /// Search for symbols in the workspace
    ///
    /// # Arguments
    /// * `params` - Search query and filters
    ///
    /// # Returns
    /// * `Result<Option<Vec<WorkspaceSymbol>>>` - Matching symbols or None
    pub async fn workspace_symbols(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<WorkspaceSymbol>>> {
        self.send_lsp_request("workspace/symbol", params).await
    }

    /// Get symbols in a specific document
    ///
    /// # Arguments
    /// * `params` - Document identifier
    ///
    /// # Returns
    /// * `Result<Option<DocumentSymbolResponse>>` - Document symbols or None
    pub async fn document_symbols(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        self.send_lsp_request("textDocument/documentSymbol", params).await
    }

    /// Pull diagnostics for a document (LSP 3.17+)
    ///
    /// # Arguments
    /// * `params` - Document identifier and optional previous result ID
    ///
    /// # Returns
    /// * `Result<Option<DocumentDiagnosticReport>>` - Diagnostic report or None
    pub async fn document_diagnostics(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<Option<DocumentDiagnosticReport>> {
        self.send_lsp_request("textDocument/diagnostic", params).await
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
    pub async fn format_document(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        self.send_lsp_request("textDocument/formatting", params).await
    }

    /// Notify server that a document was opened
    ///
    /// # Arguments
    /// * `params` - Document URI, language ID, version, and content
    pub async fn did_open(&self, params: DidOpenTextDocumentParams) -> Result<()> {
        tracing::debug!("Sending didOpen for: {}", params.text_document.uri);
        self.send_notification("textDocument/didOpen", json!(params)).await
    }

    /// Notify server that a document was closed
    ///
    /// # Arguments
    /// * `params` - Document identifier
    pub async fn did_close(&self, params: DidCloseTextDocumentParams) -> Result<()> {
        self.send_notification("textDocument/didClose", json!(params)).await
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
        self.send_notification("workspace/didCreateFiles", json!(params)).await
    }

    /// Notify server about document content changes
    ///
    /// # Arguments
    /// * `params` - Document change parameters
    pub async fn did_change(&self, params: DidChangeTextDocumentParams) -> Result<()> {
        self.send_notification("textDocument/didChange", json!(params)).await
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
        self.send_lsp_request("textDocument/diagnostic", params).await
    }

    /// Generic LSP request handler with automatic response parsing
    async fn send_lsp_request<T, R>(&self, method: &str, params: T) -> Result<Option<R>>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        tracing::trace!(
            "Sending LSP request: method={}, params={:?}",
            method,
            serde_json::to_value(&params)?
        );

        let (tx, rx) = oneshot::channel();

        self.send_request(method, json!(params), move |result| {
            tracing::trace!("LSP request callback received result: {:?}", result);
            let _ = tx.send(result);
        })
        .await?;

        tracing::trace!("Waiting for LSP response...");
        let timeout_duration = std::time::Duration::from_secs(self.config.request_timeout_secs);
        let result = tokio::time::timeout(timeout_duration, rx)
            .await
            .map_err(|_| {
                tracing::error!(
                    "LSP {} request timed out after {}s for method: {}",
                    self.config.name,
                    self.config.request_timeout_secs,
                    method
                );
                crate::error::CodeIntelligenceError::LspError {
                    server_name: self.config.name.clone(),
                    code: None,
                    message: format!(
                        "Request timed out after {}s: {}",
                        self.config.request_timeout_secs, method
                    ),
                }
            })?
            .map_err(|_| {
                tracing::error!("LSP {} response channel closed unexpectedly", self.config.name);
                crate::error::CodeIntelligenceError::connection_closed(&self.config.name, "Response channel closed")
            })?
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("Broken pipe") || err_str.contains("os error 32") {
                    let stderr_err = self.last_stderr_error.try_lock().ok().and_then(|g| g.clone());
                    let reason = stderr_err.unwrap_or_else(|| "Server process terminated unexpectedly".to_string());
                    tracing::error!("LSP {} connection lost: {}", self.config.name, reason);
                    crate::error::CodeIntelligenceError::connection_closed(&self.config.name, &reason)
                } else {
                    tracing::error!("LSP {} error: {}", self.config.name, err_str);
                    crate::error::CodeIntelligenceError::LspError {
                        server_name: self.config.name.clone(),
                        code: None,
                        message: err_str,
                    }
                }
            })?;
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
        let stdin = self.stdin.clone();
        let supports_pull_diagnostics = self.supports_pull_diagnostics.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            while let Ok(content) = read_lsp_message(&mut reader).await {
                if let Err(e) = Self::process_message(
                    &content,
                    &pending_requests,
                    &diagnostic_sender,
                    &stdin,
                    &supports_pull_diagnostics,
                )
                .await
                {
                    error!("Failed to process LSP message: {}", e);
                }
            }
            debug!("LSP connection closed");
        });
    }

    /// Convert JSON-RPC ID to string key (supports both string and number IDs)
    fn id_to_key(id: &serde_json::Value) -> Option<String> {
        if let Some(s) = id.as_str() {
            Some(s.to_string())
        } else {
            id.as_u64()
                .or_else(|| id.as_i64().map(|n| n as u64))
                .map(|n| n.to_string())
        }
    }

    /// Send JSON-RPC response to server
    async fn send_response(
        stdin: &Arc<Mutex<tokio::process::ChildStdin>>,
        id: &serde_json::Value,
        result: serde_json::Value,
    ) -> Result<()> {
        let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let content = serde_json::to_string(&resp)?;
        let mut g = stdin.lock().await;
        write_lsp_message(&mut *g, &content).await
    }

    /// Send JSON-RPC error response to server
    async fn send_error_response(
        stdin: &Arc<Mutex<tokio::process::ChildStdin>>,
        id: &serde_json::Value,
        code: i32,
        message: String,
    ) -> Result<()> {
        let resp = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        });
        let content = serde_json::to_string(&resp)?;
        let mut g = stdin.lock().await;
        write_lsp_message(&mut *g, &content).await
    }

    /// Process a single LSP message and handle response callbacks
    async fn process_message(
        content: &str,
        pending_requests: &Arc<Mutex<HashMap<String, ResponseCallback>>>,
        diagnostic_sender: &broadcast::Sender<DiagnosticEvent>,
        stdin: &Arc<Mutex<tokio::process::ChildStdin>>,
        supports_pull_diagnostics: &Arc<Mutex<bool>>,
    ) -> Result<()> {
        let message = parse_lsp_message(content)?;

        // Debug: Log all incoming messages
        debug!(
            "LSP message received: method={}, has_id={}",
            message.method,
            message.id.is_some()
        );

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

                                debug!(
                                    "Sending diagnostic event: uri={}, count={}",
                                    event.uri,
                                    event.diagnostics.len()
                                );

                                // Send to broadcast channel (ignore if no receivers)
                                match diagnostic_sender.send(event) {
                                    Ok(_) => debug!("Diagnostic event sent successfully"),
                                    Err(e) => error!("Failed to send diagnostic event: {}", e),
                                }
                            },
                            Err(e) => {
                                error!("Failed to parse publishDiagnostics params: {}", e);
                            },
                        }
                    }
                },
                "window/logMessage" => {
                    if let Some(params) = message.params {
                        match serde_json::from_value::<LogMessageParams>(params) {
                            Ok(p) => {
                                tracing::info!("LSP logMessage [{:?}]: {}", p.typ, p.message);
                            },
                            Err(e) => {
                                tracing::warn!("Failed to parse window/logMessage params: {}", e);
                            },
                        }
                    }
                },
                "window/showMessage" => {
                    if let Some(params) = message.params {
                        match serde_json::from_value::<ShowMessageParams>(params) {
                            Ok(p) => {
                                tracing::info!("LSP showMessage [{:?}]: {}", p.typ, p.message);
                            },
                            Err(e) => {
                                tracing::warn!("Failed to parse window/showMessage params: {}", e);
                            },
                        }
                    }
                },
                _ => {
                    // Other notifications - just log for now
                    debug!("Received LSP notification: {}", message.method);
                },
            }
            return Ok(());
        }

        // Handle messages with ID (requests from server or responses to our requests)
        let Some(id_value) = message.id.clone() else {
            return Ok(());
        };

        let Some(id_key) = Self::id_to_key(&id_value) else {
            tracing::warn!("Invalid ID type in message: {:?}", id_value);
            return Ok(());
        };

        // Determine if this is a server→client request or a response to our request
        // If method is present and non-empty, it's a request from server
        if !message.method.is_empty() {
            // Server→client request - must respond
            match message.method.as_str() {
                "client/registerCapability" => {
                    tracing::info!(
                        "LSP server registering capabilities: {}",
                        serde_json::to_string_pretty(&message.params)
                            .unwrap_or_else(|_| "Failed to serialize".to_string())
                    );
                    Self::send_response(stdin, &id_value, json!(null)).await?;
                    tracing::info!("Capability registration acknowledged");
                },
                "client/unregisterCapability" => {
                    tracing::info!(
                        "LSP server unregistering capabilities: {}",
                        serde_json::to_string_pretty(&message.params)
                            .unwrap_or_else(|_| "Failed to serialize".to_string())
                    );
                    Self::send_response(stdin, &id_value, json!(null)).await?;
                    tracing::info!("Capability unregistration acknowledged");
                },
                "workspace/configuration" => {
                    tracing::info!(
                        "LSP server requesting workspace configuration: {}",
                        serde_json::to_string_pretty(&message.params)
                            .unwrap_or_else(|_| "Failed to serialize".to_string())
                    );
                    // Parse params to get number of items requested
                    let num_items = message
                        .params
                        .as_ref()
                        .and_then(|p| p.get("items"))
                        .and_then(|items| items.as_array())
                        .map(|arr| arr.len())
                        .unwrap_or(0);
                    // Return null for each item (no custom settings)
                    let result: Vec<serde_json::Value> = vec![json!(null); num_items];
                    Self::send_response(stdin, &id_value, json!(result)).await?;
                    tracing::info!("Workspace configuration request acknowledged");
                },
                "window/workDoneProgress/create" => {
                    tracing::debug!("LSP server creating work done progress");
                    Self::send_response(stdin, &id_value, json!(null)).await?;
                },
                "workspace/workspaceFolders" => {
                    tracing::debug!("LSP server requesting workspace folders");
                    // Return null or empty array - workspace folders are set during initialization
                    Self::send_response(stdin, &id_value, json!(null)).await?;
                },
                "workspace/diagnostic/refresh" => {
                    tracing::debug!("LSP server requesting diagnostic refresh - enabling pull diagnostics");
                    // Set flag to indicate server supports pull diagnostics
                    *supports_pull_diagnostics.lock().await = true;
                    // Acknowledge immediately
                    Self::send_response(stdin, &id_value, json!(null)).await?;
                },
                _ => {
                    tracing::debug!(
                        "Unhandled server request: {} - responding with method not found",
                        message.method
                    );
                    Self::send_error_response(
                        stdin,
                        &id_value,
                        -32601,
                        format!("Method not found: {}", message.method),
                    )
                    .await?;
                },
            }
            return Ok(());
        }

        // Handle responses to our requests (no method field)
        let Some(callback) = pending_requests.lock().await.remove(&id_key) else {
            tracing::debug!("Received response for unknown request ID: {}", id_key);
            return Ok(());
        };

        let result = match message.error {
            Some(error) => Err(anyhow::anyhow!("LSP server error: {error}")),
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
            pending.insert(id.to_string(), Box::new(callback));
        }

        let content = serde_json::to_string(&request)?;
        let mut stdin = self.stdin.lock().await;
        write_lsp_message(&mut *stdin, &content).await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("Broken pipe") || err_str.contains("os error 32") {
                let stderr_err = self.last_stderr_error.try_lock().ok().and_then(|g| g.clone());
                let reason = stderr_err.unwrap_or_else(|| "Server process terminated unexpectedly".to_string());
                tracing::error!(
                    "LSP {} connection lost while sending request: {}",
                    self.config.name,
                    reason
                );
                crate::error::CodeIntelligenceError::connection_closed(&self.config.name, &reason)
            } else {
                tracing::error!("LSP {} write error: {}", self.config.name, err_str);
                crate::error::CodeIntelligenceError::LspError {
                    server_name: self.config.name.clone(),
                    code: None,
                    message: err_str,
                }
            }
        })?;

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
            Err(e) => Err(anyhow::anyhow!("Failed to apply workspace edit: {e}")),
        }
    }

    /// Shutdown the language server gracefully
    ///
    /// Sends shutdown request followed by exit notification per LSP spec
    pub async fn shutdown(&self) -> Result<()> {
        // Send shutdown request
        let _: Option<()> = self.send_lsp_request("shutdown", json!(null)).await?;

        // Send exit notification
        self.send_notification("exit", json!(null)).await?;

        Ok(())
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
        write_lsp_message(&mut *stdin, &content).await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("Broken pipe") || err_str.contains("os error 32") {
                let stderr_err = self.last_stderr_error.try_lock().ok().and_then(|g| g.clone());
                let reason = stderr_err.unwrap_or_else(|| "Server process terminated unexpectedly".to_string());
                tracing::error!(
                    "LSP {} connection lost while sending notification: {}",
                    self.config.name,
                    reason
                );
                crate::error::CodeIntelligenceError::connection_closed(&self.config.name, &reason)
            } else {
                tracing::error!("LSP {} notification error: {}", self.config.name, err_str);
                crate::error::CodeIntelligenceError::LspError {
                    server_name: self.config.name.clone(),
                    code: None,
                    message: err_str,
                }
            }
        })?;

        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Try to kill the child process if it's still running
        // Use try_lock to avoid blocking in async context
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}
