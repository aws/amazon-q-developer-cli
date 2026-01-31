use std::collections::{
    HashMap,
    HashSet,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use anyhow::Result;
use tokio::sync::{
    RwLock,
    mpsc,
};
use url::Url;

use crate::config::ConfigManager;
use crate::config::json_config::LanguagesConfig;
use crate::lsp::LspRegistry;
use crate::model::FsEvent;
use crate::model::types::{
    LspInfo,
    WorkspaceInfo,
};
use crate::sdk::code_store::CodeStore;
use crate::sdk::file_watcher::{
    FileWatcher,
    FileWatcherConfig,
};

/// Status of workspace initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStatus {
    /// Workspace has not been initialized
    NotInitialized,
    /// LSP servers are being initialized
    Initializing,
    /// All LSP servers have completed initialization (success or failure)
    Initialized,
}

/// Tracks file state in LSP servers
#[derive(Debug, Clone)]
pub struct FileState {
    pub version: i32,
    pub is_open: bool,
}

/// Manages workspace detection, LSP client lifecycle, and file watching.
///
/// The WorkspaceManager is responsible for:
/// - Detecting project languages by scanning file extensions
/// - Starting and managing LSP servers for detected languages
/// - Tracking file state (open/closed, versions) for LSP synchronization
/// - Watching for file changes and notifying LSP servers
/// - Collecting and caching diagnostics from LSP servers
///
/// # LSP Initialization Strategy
///
/// LSP servers are "lazy" - they don't index the workspace until a file is opened.
/// To ensure workspace-wide features (go-to-definition, find-references) work
/// immediately, we auto-open a "representative file" for each detected language.
/// This triggers the LSP to begin indexing the workspace.
///
/// The `representative_files` cache maps file extensions to the first non-config
/// source file found during workspace scanning (e.g., "rs" -> "/src/main.rs").
/// Config files like `tsconfig.json` are excluded since they don't trigger
/// proper workspace indexing.
pub struct WorkspaceManager {
    workspace_root: PathBuf,
    pub config_manager: ConfigManager,
    registry: LspRegistry,
    status: Arc<RwLock<WorkspaceStatus>>,
    pending_inits: Arc<AtomicUsize>,
    opened_files: HashMap<PathBuf, FileState>, // Track version and open state
    workspace_info: Option<WorkspaceInfo>,
    /// Cache mapping file extensions to representative source files.
    /// Used to auto-open files that trigger LSP workspace indexing.
    /// Populated during workspace scan, excludes config files.
    representative_files: Option<HashMap<String, PathBuf>>,
    diagnostics: Arc<RwLock<HashMap<PathBuf, Vec<lsp_types::Diagnostic>>>>, // shared diagnostics map

    // File watching infrastructure
    _file_watcher: Option<FileWatcher>,
    event_processor_handle: Option<tokio::task::JoinHandle<()>>,

    // Code store for pattern search/rewrite operations
    code_store: Arc<CodeStore>,
}

impl WorkspaceManager {
    /// Known config file patterns to skip when finding representative files
    const KNOWN_CONFIG_PATTERNS: &'static [&'static str] = &[
        "config",
        "tsconfig",
        "jest.",
        "vite.",
        "webpack.",
        "rollup.",
        "tsup.",
        "babel.",
        "eslint",
        "prettier",
        ".eslintrc",
        ".prettierrc",
    ];

    /// Check if any initialized LSP handles a file extension
    pub fn has_initialized_lsp_for_extension(&self, extension: &str) -> bool {
        self.registry.has_initialized_lsp_for_extension(extension)
    }

    /// Create new workspace manager with auto-detected workspace root
    pub fn new(workspace_root: PathBuf) -> Self {
        // Create config manager first (using workspace_root as base for .kiro/settings folder)
        let config_root = ConfigManager::config_root_from_workspace(&workspace_root);
        let config_manager = ConfigManager::new(config_root);

        // Get config for workspace detection
        let config = config_manager
            .get_config()
            .unwrap_or_else(|_| LanguagesConfig::default_config());

        // Now resolve actual workspace root using the config
        let resolved_root = Self::detect_workspace_root(&workspace_root, &config).unwrap_or(workspace_root.clone());

        let mut registry = LspRegistry::new();

        // Register all supported language servers using config manager
        for config in config_manager.all_configs() {
            registry.register_config(config);
        }

        Self {
            workspace_root: resolved_root,
            config_manager,
            registry,
            status: Arc::new(RwLock::new(WorkspaceStatus::NotInitialized)),
            pending_inits: Arc::new(AtomicUsize::new(0)),
            opened_files: HashMap::new(),
            workspace_info: None,
            representative_files: None,
            diagnostics: Arc::new(RwLock::new(HashMap::new())),
            _file_watcher: None,
            event_processor_handle: None,
            code_store: Arc::new(CodeStore::new()),
        }
    }

    /// Detect workspace root by walking up to find project markers
    fn detect_workspace_root(file_path: &Path, config: &LanguagesConfig) -> Option<PathBuf> {
        const MAX_DEPTH: usize = 10;

        tracing::debug!("Detecting workspace root from: {}", file_path.display());

        let current_dir;
        let start_dir = if file_path.is_file() {
            file_path.parent()?
        } else if file_path.is_dir() {
            file_path
        } else {
            current_dir = std::env::current_dir().ok()?;
            current_dir.as_path()
        };

        let mut current = start_dir;
        let mut depth = 0;

        // Detect language from file extension and use specific patterns
        if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
            if let Some(language) = config.get_language_for_extension(extension) {
                let language_patterns = config.get_project_patterns_for_language(&language);
                tracing::debug!("Language: {}, looking for patterns: {:?}", language, language_patterns);

                loop {
                    for pattern in &language_patterns {
                        let check_path = current.join(pattern);
                        if check_path.exists() {
                            tracing::info!("Found project marker '{}' at: {}", pattern, current.display());
                            return Some(current.to_path_buf());
                        }
                    }

                    depth += 1;
                    if depth >= MAX_DEPTH {
                        tracing::debug!("Reached max depth {} without finding project marker", MAX_DEPTH);
                        break;
                    }

                    current = current.parent()?;
                }
            } else {
                tracing::debug!("No language found for extension: {}", extension);
            }
        } else {
            // No extension (likely a directory) - check all known project patterns
            tracing::debug!("No extension found, checking all known project patterns");
            let all_languages = config.all_languages();

            loop {
                for language in &all_languages {
                    let patterns = config.get_project_patterns_for_language(language);
                    for pattern in &patterns {
                        let check_path = current.join(pattern);
                        if check_path.exists() {
                            tracing::info!("Found project marker '{}' at: {}", pattern, current.display());
                            return Some(current.to_path_buf());
                        }
                    }
                }

                depth += 1;
                if depth >= MAX_DEPTH {
                    tracing::debug!("Reached max depth {} without finding project marker", MAX_DEPTH);
                    break;
                }

                current = current.parent()?;
            }
        }

        tracing::warn!(
            "⚠️ No workspace root found, using start directory: {}",
            start_dir.display()
        );
        None
    }

    /// Initialize all registered language servers
    pub async fn initialize(&mut self) -> Result<()> {
        let init_start = std::time::Instant::now();
        tracing::debug!(
            "[CODE-INTEL] 🚀 Starting workspace initialization for: {}",
            self.workspace_root.display()
        );
        tracing::info!(
            "🚀 Starting workspace initialization for: {}",
            self.workspace_root.display()
        );

        // Check current status
        let current_status = *self.status.read().await;
        match current_status {
            WorkspaceStatus::Initialized => {
                tracing::info!("Workspace already initialized, skipping");
                return Ok(());
            },
            WorkspaceStatus::Initializing => {
                tracing::info!("Workspace initialization already in progress");
                return Ok(());
            },
            WorkspaceStatus::NotInitialized => {},
        }
        // Ensure config file exists (creates lsp.json if it doesn't exist)
        let stage_start = std::time::Instant::now();
        self.config_manager.ensure_config_exists()?;
        tracing::debug!("[CODE-INTEL] ✓ ensure_config_exists: {:?}", stage_start.elapsed());

        // Set status to Initializing
        *self.status.write().await = WorkspaceStatus::Initializing;

        // Auto-detect and register language servers if none are present
        let stage_start = std::time::Instant::now();
        tracing::debug!("Ensuring language servers are registered");
        self.ensure_language_servers()?;
        tracing::debug!("[CODE-INTEL] ✓ ensure_language_servers: {:?}", stage_start.elapsed());
        let workspace_uri = Url::from_file_path(&self.workspace_root).map_err(|_| {
            crate::error::CodeIntelligenceError::invalid_path(self.workspace_root.clone(), "Cannot convert to URI")
        })?;

        // Get detected languages to only initialize relevant LSPs
        let stage_start = std::time::Instant::now();
        let workspace_info = self.detect_workspace()?;
        tracing::debug!("[CODE-INTEL] ✓ detect_workspace: {:?}", stage_start.elapsed());

        let detected_languages: HashSet<String> = workspace_info.detected_languages.iter().cloned().collect();

        // Build set of server names that support detected languages
        let mut servers_for_detected_langs: HashSet<String> = HashSet::new();
        for language in &detected_languages {
            if let Ok(config) = self.config_manager.get_config_by_language(language) {
                servers_for_detected_langs.insert(config.name);
            }
        }

        // Filter registered servers to only those supporting detected languages
        let all_servers: Vec<String> = self.registry.registered_servers().into_iter().cloned().collect();
        let all_servers_count = all_servers.len();
        let server_names: Vec<String> = all_servers
            .into_iter()
            .filter(|name| servers_for_detected_langs.contains(name))
            .collect();

        tracing::info!(
            "Initializing {} of {} registered servers for detected languages {:?}: {:?}",
            server_names.len(),
            all_servers_count,
            detected_languages,
            server_names
        );

        // If no servers to initialize, mark as initialized immediately
        if server_names.is_empty() {
            *self.status.write().await = WorkspaceStatus::Initialized;
            tracing::info!("Workspace initialization completed (no LSP servers needed)");
            return Ok(());
        }

        // Set pending init count
        self.pending_inits.store(server_names.len(), Ordering::SeqCst);

        // Initialize clients for filtered servers - spawn background tasks
        for server_name in server_names {
            tracing::info!("Starting LSP server: {}", server_name);

            // Create the client (this spawns the process)
            // TODO: This manually reimplements LspClient::initialize() to enable parallel initialization
            // in spawned tasks. The registry returns &mut LspClient which cannot be moved into spawn.
            // Future improvement: Make LspClient cloneable or use Arc<LspClient> in registry to allow
            // calling client.initialize() directly and eliminate this duplication.
            match self.registry.get_client(&server_name, &self.workspace_root).await {
                Ok(client) => {
                    // Clone what we need for the background task
                    let workspace_uri = workspace_uri.clone();
                    let name = server_name.clone();

                    // Get Arc references from the client for background init
                    let status = client.status.clone();
                    let init_result = client.init_result.clone();
                    let init_duration = client.init_duration.clone();
                    let init_start = client.init_start;
                    let config = client.config.clone();
                    let stdin = client.stdin.clone();
                    let pending_requests = client.pending_requests.clone();
                    let next_id = client.next_id.clone();
                    let child = client.child.clone();

                    // Clone workspace status tracking
                    let pending_inits = self.pending_inits.clone();
                    let workspace_status = self.status.clone();

                    // Spawn background initialization with timeout
                    tokio::spawn(async move {
                        let init_future = async {
                            // Set status to Initializing
                            *status.lock().await = crate::lsp::LspStatus::Initializing;

                            // Build init params
                            let init_params = crate::lsp::LspConfig::build_initialize_params(
                                workspace_uri.clone(),
                                Some(&config),
                                config.initialization_options.clone(),
                            );

                            // Send initialize request
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            let id = {
                                let mut next = next_id.lock().await;
                                let current = *next;
                                *next += 1;
                                current
                            };

                            let request = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "method": "initialize",
                                "params": init_params
                            });

                            {
                                let mut pending = pending_requests.lock().await;
                                pending.insert(
                                    id.to_string(),
                                    Box::new(move |result| {
                                        let _ = tx.send(result);
                                    }),
                                );
                            }

                            let content = serde_json::to_string(&request)?;
                            let mut stdin_guard = stdin.lock().await;
                            crate::lsp::write_lsp_message(&mut *stdin_guard, &content).await?;
                            drop(stdin_guard);

                            // Wait for initialize response OR process exit (whichever comes first)
                            // This prevents hanging for 180s when the LSP process exits immediately
                            let init_result_val: lsp_types::InitializeResult = tokio::select! {
                                result = rx => {
                                    let response = result.map_err(|_| anyhow::anyhow!("Channel closed"))??;
                                    serde_json::from_value(response)?
                                }
                                exit_status = async {
                                    if let Some(child_ref) = child.lock().await.as_mut() {
                                        child_ref.wait().await
                                    } else {
                                        // Child already exited, wait forever (response path will complete)
                                        std::future::pending().await
                                    }
                                } => {
                                    let code = exit_status.ok().and_then(|s| s.code());
                                    return Err(anyhow::anyhow!(
                                        "LSP process exited during initialization with code: {code:?}"
                                    ));
                                }
                            };

                            // Store result and update status
                            *init_result.lock().await = Some(init_result_val);
                            *status.lock().await = crate::lsp::LspStatus::Initialized;
                            *init_duration.lock().await = Some(init_start.elapsed());

                            // Send initialized notification
                            let notification = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "initialized",
                                "params": {}
                            });
                            let content = serde_json::to_string(&notification)?;
                            let mut stdin_guard = stdin.lock().await;
                            crate::lsp::write_lsp_message(&mut *stdin_guard, &content).await?;

                            // Send didChangeConfiguration to kick Pyright (fixes 1.1.407 hang)
                            tracing::debug!("Sending workspace/didChangeConfiguration to LSP server: {}", name);
                            let config_notification = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "workspace/didChangeConfiguration",
                                "params": {"settings": {}}
                            });
                            let config_content = serde_json::to_string(&config_notification)?;
                            crate::lsp::write_lsp_message(&mut *stdin_guard, &config_content).await?;
                            drop(stdin_guard);

                            Ok::<_, anyhow::Error>(())
                        };

                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(config.request_timeout_secs),
                            init_future,
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                tracing::info!("LSP server '{}' initialized successfully", name);
                            },
                            Ok(Err(e)) => {
                                tracing::error!("LSP server '{}' initialization failed: {}", name, e);
                                *status.lock().await = crate::lsp::LspStatus::Failed(e.to_string());
                            },
                            Err(_) => {
                                tracing::warn!("LSP server '{}' timed out during initialization", name);
                                *status.lock().await =
                                    crate::lsp::LspStatus::Failed("Initialization timed out".to_string());
                            },
                        }

                        // Decrement pending count and check if all done
                        let remaining = pending_inits.fetch_sub(1, Ordering::SeqCst) - 1;
                        if remaining == 0 {
                            *workspace_status.write().await = WorkspaceStatus::Initialized;
                            tracing::info!("All LSP servers finished initialization");
                        }
                    });
                },
                Err(e) => {
                    tracing::error!("Failed to start LSP server '{}': {}", server_name, e);
                    // Decrement pending count for failed starts too
                    let remaining = self.pending_inits.fetch_sub(1, Ordering::SeqCst) - 1;
                    if remaining == 0 {
                        *self.status.write().await = WorkspaceStatus::Initialized;
                        tracing::info!("All LSP servers finished initialization");
                    }
                },
            }
        }

        tracing::info!("Workspace initialization started (LSP servers initializing in background)");

        // Subscribe to diagnostics from all initialized LSP clients
        let stage_start = std::time::Instant::now();
        tracing::debug!("Subscribing to diagnostics");
        if let Err(e) = self.subscribe_to_diagnostics().await {
            tracing::warn!("Failed to subscribe to diagnostics: {}", e);
        }
        tracing::debug!("[CODE-INTEL] ✓ subscribe_to_diagnostics: {:?}", stage_start.elapsed());

        // Start file watching after LSP initialization
        let stage_start = std::time::Instant::now();
        tracing::debug!("Starting file watching");
        if let Err(e) = self.start_file_watching() {
            tracing::warn!("Failed to start file watching: {}", e);
        }
        tracing::debug!("[CODE-INTEL] ✓ start_file_watching: {:?}", stage_start.elapsed());

        // Auto-open representative files (will skip files whose LSPs aren't ready yet)
        // Files will be opened on first actual use if LSPs are still initializing
        let stage_start = std::time::Instant::now();
        if let Err(e) = self.auto_open_representative_files().await {
            tracing::warn!("Failed to auto-open representative files: {}", e);
        }
        tracing::debug!(
            "[CODE-INTEL] ✓ auto_open_representative_files: {:?}",
            stage_start.elapsed()
        );
        tracing::debug!("[CODE-INTEL] ✅ Total initialization time: {:?}", init_start.elapsed());

        Ok(())
    }

    /// Get LSP client for file
    pub async fn get_client_for_file(&mut self, file_path: &Path) -> Result<Option<&mut crate::lsp::LspClient>> {
        let extension = file_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        self.registry
            .get_client_for_extension(extension, &self.workspace_root)
            .await
    }

    /// Subscribe to diagnostics from all initialized LSP clients and start background tasks
    pub async fn subscribe_to_diagnostics(&mut self) -> Result<()> {
        let detected_languages = self.get_detected_languages()?;
        tracing::debug!("Subscribing to diagnostics for languages: {:?}", detected_languages);

        for language in detected_languages {
            if let Ok(Some(client)) = self.get_client_by_language(&language).await {
                let mut receiver = client.subscribe_diagnostics();
                let diagnostics_map = self.diagnostics.clone(); // Share the single map
                let language_clone = language.clone(); // Clone for the async task

                tracing::debug!("Starting background diagnostic task for language: {}", language);

                // Start background task to collect diagnostics
                tokio::spawn(async move {
                    tracing::debug!("Background diagnostic task started for language: {}", language_clone);
                    while let Ok(diagnostic_event) = receiver.recv().await {
                        tracing::debug!(
                            "Background task received diagnostic event for: {}",
                            diagnostic_event.uri
                        );
                        if let Ok(file_path) = url::Url::parse(&diagnostic_event.uri)
                            .and_then(|url| url.to_file_path().map_err(|_| url::ParseError::InvalidPort))
                        {
                            let mut map = diagnostics_map.write().await;
                            map.insert(file_path.clone(), diagnostic_event.diagnostics.clone());
                            tracing::debug!(
                                "Stored {} diagnostics for file: {:?}",
                                diagnostic_event.diagnostics.len(),
                                file_path
                            );
                        }
                    }
                    tracing::debug!("Background diagnostic task ended for language: {}", language_clone);
                });

                tracing::debug!("Subscribed to diagnostics for language: {}", language);
            } else {
                tracing::warn!("No client available for language: {}", language);
            }
        }

        Ok(())
    }

    /// Get stored diagnostics for a specific file
    pub async fn get_diagnostics_for_file(&mut self, file_path: &Path) -> Result<Vec<lsp_types::Diagnostic>> {
        tracing::debug!("Getting diagnostics for file: {:?}", file_path);

        // Ensure file is opened
        let supports_pull = self.ensure_file_opened(file_path).await?;

        // Fetch diagnostics based on server capability
        if supports_pull {
            self.pull_diagnostics(file_path).await
        } else {
            self.get_cached_diagnostics(file_path).await
        }
    }

    /// Ensure file is opened, returns whether server supports pull diagnostics
    async fn ensure_file_opened(&mut self, file_path: &Path) -> Result<bool> {
        if self.is_file_opened(file_path) {
            // File already opened, check if server supports pull
            let extension = file_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            if let Some(language) = self.config_manager.get_language_for_extension(extension)
                && let Ok(Some(client)) = self.get_client_by_language(&language).await
            {
                return Ok(client.supports_pull_diagnostics());
            }
            return Ok(false);
        }

        tracing::debug!("File not opened yet, opening: {:?}", file_path);

        let extension = file_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let language = self
            .config_manager
            .get_language_for_extension(extension)
            .ok_or_else(|| {
                crate::error::CodeIntelligenceError::lsp_not_available(file_path.to_path_buf(), extension, None)
            })?;

        tracing::debug!("Detected language: {} for file: {:?}", language, file_path);

        if let Ok(Some(client)) = self.get_client_by_language(&language).await {
            let content = std::fs::read_to_string(file_path)?;
            let did_open_params = lsp_types::DidOpenTextDocumentParams {
                text_document: lsp_types::TextDocumentItem {
                    uri: url::Url::from_file_path(file_path).unwrap(),
                    language_id: language,
                    version: 1,
                    text: content,
                },
            };

            tracing::debug!("Sending didOpen for file: {:?}", file_path);
            let supports_pull = client.supports_pull_diagnostics();
            client.did_open(did_open_params).await?;

            self.mark_file_opened(file_path.to_path_buf());

            // For push diagnostics, wait for them to arrive
            if !supports_pull {
                tracing::debug!("Waiting 3 seconds for push diagnostics...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }

            return Ok(supports_pull);
        }

        Ok(false)
    }

    /// Pull fresh diagnostics from server (for pull-based diagnostics)
    async fn pull_diagnostics(&mut self, file_path: &Path) -> Result<Vec<lsp_types::Diagnostic>> {
        tracing::debug!("Pulling fresh diagnostics for: {:?}", file_path);

        let extension = file_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let language = self
            .config_manager
            .get_language_for_extension(extension)
            .ok_or_else(|| {
                crate::error::CodeIntelligenceError::lsp_not_available(file_path.to_path_buf(), extension, None)
            })?;

        if let Ok(Some(client)) = self.get_client_by_language(&language).await {
            let uri = url::Url::from_file_path(file_path).map_err(|_| {
                crate::error::CodeIntelligenceError::invalid_path(file_path.to_path_buf(), "Cannot convert to URI")
            })?;

            let params = lsp_types::DocumentDiagnosticParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                identifier: None,
                previous_result_id: None,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            if let Ok(Some(report)) = client.document_diagnostics(params).await {
                let diagnostics = match report {
                    lsp_types::DocumentDiagnosticReport::Full(full) => full.full_document_diagnostic_report.items,
                    lsp_types::DocumentDiagnosticReport::Unchanged(_) => {
                        // Return cached if unchanged
                        return self.get_cached_diagnostics(file_path).await;
                    },
                };
                tracing::debug!("Pulled {} diagnostics for: {:?}", diagnostics.len(), file_path);
                return Ok(diagnostics);
            }
        }

        // Fallback to cached
        self.get_cached_diagnostics(file_path).await
    }

    /// Get cached diagnostics (for push-based diagnostics or fallback)
    async fn get_cached_diagnostics(&mut self, file_path: &Path) -> Result<Vec<lsp_types::Diagnostic>> {
        let map = self.diagnostics.read().await;
        let diagnostics = map.get(file_path).cloned().unwrap_or_default();
        tracing::debug!(
            "Retrieved {} cached diagnostics for: {:?}",
            diagnostics.len(),
            file_path
        );
        Ok(diagnostics)
    }

    /// Get workspace root
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get the code store for pattern search/rewrite operations
    pub fn code_store(&self) -> &Arc<CodeStore> {
        &self.code_store
    }

    /// Get all registered server names for workspace-wide operations
    pub fn get_all_server_names(&self) -> Vec<String> {
        self.registry.registered_servers().into_iter().cloned().collect()
    }

    /// Get client by server name
    pub async fn get_client_by_name(&mut self, server_name: &str) -> Result<Option<&mut crate::lsp::LspClient>> {
        match self.registry.get_client(server_name, &self.workspace_root).await {
            Ok(client) => Ok(Some(client)),
            Err(_) => Ok(None),
        }
    }

    /// Get client by language name (maps language to server name)
    pub async fn get_client_by_language(&mut self, language: &str) -> Result<Option<&mut crate::lsp::LspClient>> {
        // Use ConfigManager to get server name for language
        let server_name = self
            .config_manager
            .get_server_name_for_language(language)
            .unwrap_or_else(|| language.to_string());

        self.get_client_by_name(&server_name).await
    }

    /// Add a language server configuration
    pub fn add_language_server(&mut self, config: crate::model::types::LanguageServerConfig) {
        self.registry.register_config(config);
    }

    /// Ensure language servers are registered, auto-detecting if none are present
    fn ensure_language_servers(&mut self) -> Result<()> {
        if self.registry.registered_servers().is_empty() {
            let workspace_info = self.detect_workspace()?;
            for language in workspace_info.detected_languages {
                if let Ok(config) = self.config_manager.get_config_by_language(&language) {
                    self.add_language_server(config);
                }
            }
        }
        Ok(())
    }

    /// Detect workspace languages and available LSPs
    pub fn detect_workspace(&mut self) -> Result<WorkspaceInfo> {
        tracing::info!("Starting workspace detection for: {}", self.workspace_root.display());

        // Use cached detection results if available, but always refresh LSP status
        let (detected_languages, project_markers) = if let Some(ref info) = self.workspace_info {
            tracing::info!("Using cached workspace detection (will refresh LSP status)");
            (info.detected_languages.clone(), info.project_markers.clone())
        } else {
            // Perform full workspace detection with unified scan
            let mut detected_languages = Vec::new();

            // Recursively scan workspace for file extensions AND representative files in one pass
            tracing::info!("Scanning directory for file extensions and representative files (respecting .gitignore)");
            let (file_extensions, representative_files) = self.scan_workspace_unified()?;
            self.representative_files = Some(representative_files); // Cache for auto-open
            tracing::info!(
                "Found {} unique file extensions: {:?}",
                file_extensions.len(),
                file_extensions
            );

            // Map extensions to languages using ConfigManager
            tracing::debug!("Mapping extensions to languages");
            for ext in &file_extensions {
                if let Some(language) = self.config_manager.get_language_for_extension(ext) {
                    tracing::debug!("Extension '{}' mapped to language '{}'", ext, language);
                    detected_languages.push(language);
                }
            }

            detected_languages.sort();
            detected_languages.dedup();
            tracing::info!(
                "Detected {} languages: {:?}",
                detected_languages.len(),
                detected_languages
            );

            // Detect project markers
            let mut project_markers = Vec::new();
            for language in &detected_languages {
                let patterns = self.config_manager.get_project_patterns_for_language(language);
                for pattern in patterns {
                    if self.workspace_root.join(&pattern).exists() {
                        project_markers.push(pattern);
                    }
                }
            }
            tracing::info!("Detected project markers: {:?}", project_markers);

            (detected_languages, project_markers)
        };

        // Always check current LSP status (don't cache as it changes during initialization)
        tracing::debug!("Checking available LSPs");
        let available_lsps = self.check_available_lsps();
        tracing::info!("Available LSPs: {:?}", available_lsps);

        let info = WorkspaceInfo {
            root_path: self.workspace_root.clone(),
            detected_languages,
            available_lsps,
            project_markers,
        };

        self.workspace_info = Some(info.clone());
        tracing::info!("Workspace detection completed");
        Ok(info)
    }

    /// Unified workspace scan: collects extensions AND representative files in one parallel pass
    fn scan_workspace_unified(&self) -> Result<(HashSet<String>, HashMap<String, PathBuf>)> {
        use dashmap::DashMap;

        use crate::utils::traversal::create_code_walker;

        // Detect if scanning home directory (cross-platform: Linux, Mac, Windows)
        let is_home_dir = dirs::home_dir()
            .and_then(|home| home.canonicalize().ok())
            .map(|home| self.workspace_root.canonicalize().ok() == Some(home))
            .unwrap_or(false);

        let max_depth = if is_home_dir {
            tracing::warn!("Workspace is home directory - limiting scan depth to 3 to avoid scanning entire home");
            3
        } else {
            15
        };

        let extensions: DashMap<String, ()> = DashMap::new();
        let rep_files: DashMap<String, PathBuf> = DashMap::new();

        create_code_walker(&self.workspace_root, Some(max_depth))
            .build_parallel()
            .run(|| {
                let extensions = &extensions;
                let rep_files = &rep_files;

                Box::new(move |entry| {
                    if let Ok(entry) = entry
                        && entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                        && let Some(ext) = entry.path().extension().and_then(|e| e.to_str())
                    {
                        let ext_str = ext.to_string();
                        extensions.insert(ext_str.clone(), ());

                        // Only insert non-config files as representatives
                        if !Self::is_config_file_static(entry.path()) {
                            rep_files.entry(ext_str).or_insert(entry.path().to_path_buf());
                        }
                    }
                    ignore::WalkState::Continue
                })
            });

        let ext_set: HashSet<String> = extensions.into_iter().map(|(k, _)| k).collect();
        let rep_map: HashMap<String, PathBuf> = rep_files.into_iter().collect();

        Ok((ext_set, rep_map))
    }

    /// Static version of is_config_file for use in closures
    fn is_config_file_static(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let name_lower = name.to_lowercase();
            Self::KNOWN_CONFIG_PATTERNS.iter().any(|p| name_lower.contains(p))
        } else {
            false
        }
    }

    /// Check which LSPs are available in the system
    pub fn check_available_lsps(&self) -> Vec<LspInfo> {
        let mut lsps = Vec::new();

        // Get all supported configurations from ConfigManager
        let configs = self.config_manager.all_configs();

        // Get initialized servers (actually running LSP clients)
        let initialized_servers: HashSet<String> = self.registry.initialized_servers().into_iter().cloned().collect();

        tracing::info!(
            "Checking {} LSP configurations against {} initialized servers",
            configs.len(),
            initialized_servers.len()
        );

        for config in configs {
            // Check if this LSP is in PATH using which crate (cross-platform)
            let is_available = which::which(&config.command).is_ok();

            // Get status from registry
            let status = self.registry.get_server_status(&config.name);
            let init_duration = self.registry.get_init_duration(&config.name);
            let is_initialized = status
                .as_ref()
                .map(|s| matches!(s, crate::lsp::LspStatus::Initialized))
                .unwrap_or(false);
            let status_str = status.map(|s| match s {
                crate::lsp::LspStatus::Registered => "registered".to_string(),
                crate::lsp::LspStatus::Initializing => "initializing".to_string(),
                crate::lsp::LspStatus::Initialized => "initialized".to_string(),
                crate::lsp::LspStatus::Failed(msg) => format!("failed: {msg}"),
            });

            tracing::debug!(
                "LSP {} - in PATH: {}, status: {:?}",
                config.name,
                is_available,
                status_str
            );

            // Map file extensions to languages using ConfigManager
            let languages: Vec<String> = config
                .file_extensions
                .iter()
                .filter_map(|ext| self.config_manager.get_language_for_extension(ext))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            // Compute workspace folders if multi_workspace is enabled and LSP is initialized
            let workspace_folders = if config.multi_workspace && is_initialized {
                let root_uri = url::Url::from_file_path(&self.workspace_root).ok();
                if let Some(uri) = root_uri {
                    crate::lsp::LspConfig::discover_workspaces(&uri, &config.project_patterns, &config.exclude_patterns)
                        .into_iter()
                        .map(|f| f.uri.to_string())
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            lsps.push(LspInfo {
                name: config.name,
                command: config.command,
                languages,
                is_available,
                is_initialized,
                status: status_str,
                version: None,
                workspace_folders,
                init_duration_ms: init_duration.map(|d| d.as_millis() as u64),
            });
        }

        tracing::info!(
            "Found {} available, {} initialized LSP servers",
            lsps.iter().filter(|l| l.is_available).count(),
            lsps.iter().filter(|l| l.is_initialized).count()
        );
        lsps
    }

    /// Check if workspace is fully initialized (all LSPs done)
    pub fn is_initialized(&self) -> bool {
        self.status
            .try_read()
            .map(|s| *s == WorkspaceStatus::Initialized)
            .unwrap_or(false)
    }

    /// Get current workspace initialization status
    pub fn workspace_status(&self) -> WorkspaceStatus {
        // Check in-memory status
        if let Ok(guard) = self.status.try_read() {
            return *guard;
        }
        WorkspaceStatus::NotInitialized
    }

    /// Check if lsp.json config exists (workspace was initialized before)
    pub fn config_exists(&self) -> bool {
        self.config_manager.config_exists()
    }

    /// Reset initialization state to allow re-initialization
    pub async fn reset_initialization(&mut self) {
        // Shutdown all LSP servers gracefully
        self.registry.shutdown_all().await;

        // Clear all cached state
        self.workspace_info = None;
        self.opened_files.clear();
        self.diagnostics.write().await.clear();

        // Stop file watcher if running
        if let Some(handle) = self.event_processor_handle.take() {
            handle.abort();
        }

        *self.status.write().await = WorkspaceStatus::NotInitialized;
        self.pending_inits.store(0, Ordering::SeqCst);
    }

    /// Check if a file is already opened
    pub fn is_file_opened(&self, file_path: &Path) -> bool {
        self.opened_files.get(file_path).is_some_and(|state| state.is_open)
    }

    /// Mark a file as opened with initial version
    pub fn mark_file_opened(&mut self, file_path: PathBuf) {
        self.opened_files.insert(file_path, FileState {
            version: 1,
            is_open: true,
        });
    }

    /// Get next version for file and increment it
    pub fn get_next_version(&mut self, file_path: &Path) -> i32 {
        if let Some(state) = self.opened_files.get_mut(file_path) {
            state.version += 1;
            state.version
        } else {
            // File not tracked, start at version 1
            self.opened_files.insert(file_path.to_path_buf(), FileState {
                version: 1,
                is_open: true,
            });
            1
        }
    }

    /// Mark file as closed
    pub fn mark_file_closed(&mut self, file_path: &Path) {
        if let Some(state) = self.opened_files.get_mut(file_path) {
            state.is_open = false;
            state.version = 0;
        }
    }

    /// Get detected workspace languages (cached)
    pub fn get_detected_languages(&mut self) -> Result<Vec<String>> {
        if self.workspace_info.is_none() {
            self.workspace_info = Some(self.detect_workspace()?);
        }
        Ok(self.workspace_info.as_ref().unwrap().detected_languages.clone())
    }

    /// Get languages that have initialized LSP servers
    pub fn get_initialized_lsp_languages(&self) -> Vec<String> {
        let configs = self.config_manager.all_configs();
        let mut languages = Vec::new();

        for config in configs {
            let status = self.registry.get_server_status(&config.name);
            let is_initialized = status
                .as_ref()
                .map(|s| matches!(s, crate::lsp::LspStatus::Initialized))
                .unwrap_or(false);

            if is_initialized {
                languages.push(config.language.clone());
            }
        }

        languages.sort();
        languages.dedup();
        languages
    }

    /// Check if code intelligence has been initialized (lsp.json exists)
    pub fn is_code_intelligence_initialized(&self) -> bool {
        self.workspace_root
            .join(".kiro")
            .join("settings")
            .join("lsp.json")
            .exists()
    }

    /// Start file watching with patterns based on detected languages
    pub fn start_file_watching(&mut self) -> Result<()> {
        let fw_start = std::time::Instant::now();
        let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();

        // Generate config from detected languages
        let mut include_patterns = Vec::new();
        let mut exclude_patterns = vec!["**/.git/**".to_string()]; // Always exclude .git

        // Get detected languages and their patterns
        let detected_languages = self.get_detected_languages()?;
        for language in &detected_languages {
            if let Ok(lang_config) = self.config_manager.get_config_by_language(language) {
                // Add include patterns from file extensions
                for ext in &lang_config.file_extensions {
                    include_patterns.push(format!("**/*.{ext}"));
                }
                // Add exclude patterns from language config
                exclude_patterns.extend(lang_config.exclude_patterns);
            }
        }
        tracing::debug!("[CODE-INTEL]   - config generation: {:?}", fw_start.elapsed());

        let config = FileWatcherConfig {
            include_patterns,
            exclude_patterns,
            respect_gitignore: true,
        };

        // Start file watcher
        let watcher_start = std::time::Instant::now();
        let file_watcher = FileWatcher::new(self.workspace_root.clone(), tx, config)?;
        tracing::debug!("[CODE-INTEL]   - FileWatcher::new: {:?}", watcher_start.elapsed());

        // Start event processor with workspace manager reference
        let processor = crate::sdk::file_watcher::EventProcessor::new(rx, self as *mut _, self.workspace_root.clone());
        let handle = tokio::spawn(async move {
            processor.run().await;
        });

        self._file_watcher = Some(file_watcher);
        self.event_processor_handle = Some(handle);

        tracing::info!("File watching started for languages: {:?}", detected_languages);
        Ok(())
    }

    /// Automatically open representative files for each language to enable workspace symbol search
    async fn auto_open_representative_files(&mut self) -> Result<()> {
        let detected_languages = self.get_detected_languages()?;

        for language in detected_languages {
            if let Some(file_path) = self.find_representative_file(&language).await? {
                tracing::debug!("Auto-opening representative file for {}: {:?}", language, file_path);

                // Read file content
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    // Check if file is already opened
                    if self.is_file_opened(&file_path) {
                        continue;
                    }

                    // Determine language ID from file extension
                    let language_id = if let Some(ext) = file_path.extension().and_then(|ext| ext.to_str()) {
                        self.config_manager
                            .get_language_for_extension(ext)
                            .unwrap_or_else(|| "plaintext".to_string())
                    } else {
                        "plaintext".to_string()
                    };

                    // Get LSP client for this file
                    if let Ok(Some(client)) = self.get_client_for_file(&file_path).await {
                        // Skip if client is not initialized yet
                        if !client.is_initialized() {
                            tracing::debug!("Skipping auto-open for {} - LSP not initialized yet", language);
                            continue;
                        }

                        // Create LSP parameters for opening the file
                        let uri = url::Url::from_file_path(&file_path).map_err(|_| {
                            crate::error::CodeIntelligenceError::invalid_path(
                                file_path.clone(),
                                "Cannot convert to URI",
                            )
                        })?;

                        let params = lsp_types::DidOpenTextDocumentParams {
                            text_document: lsp_types::TextDocumentItem {
                                uri,
                                language_id,
                                version: 1,
                                text: content,
                            },
                        };

                        // Open the file in the LSP client
                        if let Err(e) = client.did_open(params).await {
                            tracing::warn!("Failed to auto-open file {:?} for {}: {}", file_path, language, e);
                        } else {
                            tracing::debug!("Successfully auto-opened file for {} workspace indexing", language);

                            // Mark file as opened
                            self.opened_files.insert(file_path.clone(), FileState {
                                version: 1,
                                is_open: true,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Find a representative file for the given language to trigger workspace indexing
    async fn find_representative_file(&self, language: &str) -> Result<Option<PathBuf>> {
        let extensions = self.config_manager.get_extensions_for_language(language);

        // Search for files with matching extensions in the workspace
        for extension in extensions {
            if let Some(file_path) = self.find_first_file_with_extension(&extension).await? {
                return Ok(Some(file_path));
            }
        }

        Ok(None)
    }

    /// Find the first file with the given extension in the workspace, respecting exclude patterns
    async fn find_first_file_with_extension(&self, extension: &str) -> Result<Option<PathBuf>> {
        Ok(self
            .representative_files
            .as_ref()
            .and_then(|files| files.get(extension).cloned()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn create_temp_workspace(patterns: &[&str]) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        for pattern in patterns {
            let file_path = temp_dir.path().join(pattern);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&file_path, "").unwrap();
        }
        temp_dir
    }

    #[test]
    fn test_detect_workspace_root_rust_project() {
        let temp_dir = create_temp_workspace(&["Cargo.toml", "src/main.rs"]);
        let rust_file = temp_dir.path().join("src/main.rs");

        let config = LanguagesConfig::default_config();
        let workspace_root = WorkspaceManager::detect_workspace_root(&rust_file, &config);
        assert!(workspace_root.is_some());
        assert_eq!(workspace_root.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_detect_workspace_root_typescript_project() {
        let temp_dir = create_temp_workspace(&["package.json", "src/index.ts"]);
        let ts_file = temp_dir.path().join("src/index.ts");

        let config = LanguagesConfig::default_config();
        let workspace_root = WorkspaceManager::detect_workspace_root(&ts_file, &config);
        assert!(workspace_root.is_some());
        assert_eq!(workspace_root.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_detect_workspace_root_no_project() {
        let temp_dir = TempDir::new().unwrap();
        let random_file = temp_dir.path().join("random.txt");
        fs::write(&random_file, "").unwrap();

        let config = LanguagesConfig::default_config();
        let workspace_root = WorkspaceManager::detect_workspace_root(&random_file, &config);
        assert!(workspace_root.is_none());
    }

    #[test]
    fn test_workspace_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        assert_eq!(workspace_manager.workspace_root(), temp_dir.path());
    }

    #[test]
    fn test_get_detected_languages() {
        let temp_dir = create_temp_workspace(&["Cargo.toml", "src/main.rs"]);
        let mut workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        let languages = workspace_manager.get_detected_languages().unwrap();
        assert!(languages.contains(&"rust".to_string()));
    }

    #[test]
    fn test_detect_workspace_root_no_project_markers() {
        let temp_dir = create_temp_workspace(&["random.txt"]);
        let file_path = temp_dir.path().join("random.txt");

        let config = LanguagesConfig::default_config();
        let root = WorkspaceManager::detect_workspace_root(&file_path, &config);
        assert!(root.is_none());
    }

    #[test]
    fn test_detect_workspace_root_directory_input() {
        let temp_dir = create_temp_workspace(&["Cargo.toml", "src/main.rs"]);
        let file_path = temp_dir.path().join("src/main.rs");

        let config = LanguagesConfig::default_config();
        let root = WorkspaceManager::detect_workspace_root(&file_path, &config);
        assert_eq!(root, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_scan_workspace_finds_extensions() {
        let temp_dir = create_temp_workspace(&["test.rs", "test.ts", "test.py"]);
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        let (extensions, _) = workspace_manager.scan_workspace_unified().unwrap();

        assert!(extensions.contains("rs"));
        assert!(extensions.contains("ts"));
        assert!(extensions.contains("py"));
    }

    #[test]
    fn test_scan_workspace_skips_ignored_dirs() {
        let temp_dir = create_temp_workspace(&[
            "src/main.rs",
            "target/debug/app",
            "node_modules/package/index.js",
            ".git/config",
        ]);

        // Create .gitignore to ignore node_modules
        std::fs::write(temp_dir.path().join(".gitignore"), "node_modules/\ntarget/\n").unwrap();

        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        let (extensions, _) = workspace_manager.scan_workspace_unified().unwrap();

        assert!(extensions.contains("rs"));
        assert!(!extensions.contains("js")); // Should be skipped from node_modules via .gitignore
    }

    #[test]
    fn test_check_available_lsps_returns_list() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let lsps = workspace_manager.check_available_lsps();

        assert!(!lsps.is_empty());
        // Should contain at least the configured LSPs
        let lsp_names: Vec<&String> = lsps.iter().map(|lsp| &lsp.name).collect();
        assert!(lsp_names.contains(&&"rust-analyzer".to_string()));
        assert!(lsp_names.contains(&&"typescript-language-server".to_string()));
    }

    #[test]
    fn test_check_available_lsps_has_correct_structure() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let lsps = workspace_manager.check_available_lsps();

        for lsp in lsps {
            assert!(!lsp.name.is_empty());
            assert!(!lsp.command.is_empty());
            assert!(!lsp.languages.is_empty());
            // is_available can be true or false depending on system
        }
    }

    #[test]
    fn test_get_all_server_names() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        let server_names = workspace_manager.get_all_server_names();
        assert!(!server_names.is_empty());
        assert!(server_names.contains(&"rust-analyzer".to_string()));
        assert!(server_names.contains(&"typescript-language-server".to_string()));
    }

    #[test]
    fn test_workspace_root() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        assert_eq!(workspace_manager.workspace_root(), temp_dir.path());
    }

    #[test]
    fn test_is_initialized_default_false() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());

        assert!(!workspace_manager.is_initialized());
    }

    #[test]
    fn test_is_file_opened_default_false() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let test_path = temp_dir.path().join("test.rs");

        assert!(!workspace_manager.is_file_opened(&test_path));
    }

    #[test]
    fn test_mark_file_opened() {
        let temp_dir = TempDir::new().unwrap();
        let mut workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let test_path = temp_dir.path().join("test.rs");

        workspace_manager.mark_file_opened(test_path.clone());
        assert!(workspace_manager.is_file_opened(&test_path));
    }
}
