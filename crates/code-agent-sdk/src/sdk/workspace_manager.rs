use crate::config::ConfigManager;
use crate::config::json_config::LanguagesConfig;
use crate::lsp::LspRegistry;

use crate::model::types::{LspInfo, WorkspaceInfo};
use crate::model::FsEvent;
use crate::sdk::file_watcher::{FileWatcher, FileWatcherConfig};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::warn;
use url::Url;

/// Tracks file state in LSP servers
#[derive(Debug, Clone)]
pub struct FileState {
    pub version: i32,
    pub is_open: bool,
}

/// Manages workspace detection and LSP client lifecycle
#[derive(Debug)]
pub struct WorkspaceManager {
    workspace_root: PathBuf,
    pub config_manager: ConfigManager,
    registry: LspRegistry,
    initialized: bool,
    opened_files: HashMap<PathBuf, FileState>, // Track version and open state
    workspace_info: Option<WorkspaceInfo>,
    diagnostics: Arc<RwLock<HashMap<PathBuf, Vec<lsp_types::Diagnostic>>>>, // shared diagnostics map
    
    // File watching infrastructure
    _file_watcher: Option<FileWatcher>,
    event_processor_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WorkspaceManager {
    /// Create new workspace manager with auto-detected workspace root
    pub fn new(workspace_root: PathBuf) -> Self {
        // Create config manager first (using workspace_root as base for .code-agent folder)
        let config_root = workspace_root.join(".code-agent");
        let config_manager = ConfigManager::new(config_root);
        
        // Get config for workspace detection
        let config = config_manager.get_config().unwrap_or_else(|_| LanguagesConfig::default_config());
        
        // Now resolve actual workspace root using the config
        let resolved_root = Self::detect_workspace_root(&workspace_root, &config).unwrap_or(workspace_root);
        
        let mut registry = LspRegistry::new();

        // Register all supported language servers using config manager
        for config in config_manager.all_configs() {
            registry.register_config(config);
        }

        Self {
            workspace_root: resolved_root,
            config_manager,
            registry,
            initialized: false,
            opened_files: HashMap::new(),
            workspace_info: None,
            diagnostics: Arc::new(RwLock::new(HashMap::new())),
            _file_watcher: None,
            event_processor_handle: None,
        }
    }

    /// Detect workspace root by walking up to find project markers
    fn detect_workspace_root(file_path: &Path, config: &LanguagesConfig) -> Option<PathBuf> {
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

        // Detect language from file extension and use specific patterns
        if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
            if let Some(language) = config.get_language_for_extension(extension) {
                let language_patterns = config.get_project_patterns_for_language(&language);

                loop {
                    for pattern in &language_patterns {
                        if current.join(pattern).exists() {
                            return Some(current.to_path_buf());
                        }
                    }
                    current = current.parent()?;
                }
            }
        }

        None
    }

    /// Initialize all registered language servers
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        let workspace_uri = Url::from_file_path(&self.workspace_root)
            .map_err(|_| anyhow::anyhow!("Invalid workspace path"))?;

        // Get list of server names first to avoid borrowing issues
        let server_names: Vec<String> = self
            .registry
            .registered_servers()
            .into_iter()
            .cloned()
            .collect();

        // Initialize clients for all registered servers with timeout protection
        for server_name in server_names {
            let init_future = async {
                if let Ok(client) = self
                    .registry
                    .get_client(&server_name, &self.workspace_root)
                    .await
                {
                    let _ = client.initialize(workspace_uri.clone()).await;
                }
            };

            // Add 10-second timeout to prevent hanging on unavailable servers (increased for TypeScript)
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), init_future).await {
                Ok(_) => {
                }
                Err(_) => {
                    warn!(
                        "⏰ Warning: LSP server '{}' timed out during initialization",
                        server_name
                    );
                }
            }
            // Small delay between server initializations to prevent conflicts
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        self.initialized = true;
        
        // Subscribe to diagnostics from all initialized LSP clients
        if let Err(e) = self.subscribe_to_diagnostics().await {
            tracing::warn!("Failed to subscribe to diagnostics: {}", e);
        }
        
        // Start file watching after LSP initialization
        if let Err(e) = self.start_file_watching() {
            tracing::warn!("Failed to start file watching: {}", e);
        }
        
        Ok(())
    }

    /// Get LSP client for file
    pub async fn get_client_for_file(
        &mut self,
        file_path: &Path,
    ) -> Result<Option<&mut crate::lsp::LspClient>> {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

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
                        tracing::debug!("Background task received diagnostic event for: {}", diagnostic_event.uri);
                        if let Ok(file_path) = url::Url::parse(&diagnostic_event.uri)
                            .and_then(|url| url.to_file_path().map_err(|_| url::ParseError::InvalidPort)) {
                            
                            let mut map = diagnostics_map.write().await;
                            map.insert(file_path.clone(), diagnostic_event.diagnostics.clone());
                            tracing::debug!("Stored {} diagnostics for file: {:?}", diagnostic_event.diagnostics.len(), file_path);
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
        
        // First, check if we need to open the file to trigger diagnostics
        if !self.is_file_opened(file_path) {
            tracing::debug!("File not opened yet, opening: {:?}", file_path);
            
            // Determine language and get client
            let extension = file_path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");
                
            let language = self.config_manager.get_language_for_extension(extension)
                .ok_or_else(|| anyhow::anyhow!("No language server for file: {}", file_path.display()))?;

            tracing::debug!("Detected language: {} for file: {:?}", language, file_path);

            if let Ok(Some(client)) = self.get_client_by_language(&language).await {
                // Read file content and open it
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
                client.did_open(did_open_params).await?;
                self.mark_file_opened(file_path.to_path_buf());
                
                tracing::debug!("Waiting 3 seconds for diagnostics to arrive...");
                // Wait longer for diagnostics to arrive (like the test does)
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                tracing::debug!("Finished waiting for diagnostics");
            } else {
                tracing::warn!("No client available for language: {}", language);
            }
        } else {
            tracing::debug!("File already opened: {:?}", file_path);
        }
        
        // Return stored diagnostics from the shared map (read lock for better performance)
        let map = self.diagnostics.read().await;
        let diagnostics = map.get(file_path).cloned().unwrap_or_default();
        tracing::debug!("Retrieved {} diagnostics for file: {:?}", diagnostics.len(), file_path);
        Ok(diagnostics)
    }

    /// Get workspace root
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get all registered server names for workspace-wide operations
    pub fn get_all_server_names(&self) -> Vec<String> {
        self.registry
            .registered_servers()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get client by server name
    pub async fn get_client_by_name(
        &mut self,
        server_name: &str,
    ) -> Result<Option<&mut crate::lsp::LspClient>> {
        match self
            .registry
            .get_client(server_name, &self.workspace_root)
            .await
        {
            Ok(client) => Ok(Some(client)),
            Err(_) => Ok(None),
        }
    }

    /// Get client by language name (maps language to server name)
    pub async fn get_client_by_language(
        &mut self,
        language: &str,
    ) -> Result<Option<&mut crate::lsp::LspClient>> {
        // Use ConfigManager to get server name for language
        let server_name = self.config_manager.get_server_name_for_language(language)
            .unwrap_or_else(|| language.to_string());

        self.get_client_by_name(&server_name).await
    }

    /// Add a language server configuration
    pub fn add_language_server(&mut self, config: crate::model::types::LanguageServerConfig) {
        self.registry.register_config(config);
    }

    /// Detect workspace languages and available LSPs
    pub fn detect_workspace(&mut self) -> Result<WorkspaceInfo> {
        if let Some(ref info) = self.workspace_info {
            return Ok(info.clone());
        }

        let mut detected_languages = Vec::new();
        let mut file_extensions = HashSet::new();

        // Recursively scan workspace for file extensions
        self.scan_directory(&self.workspace_root, &mut file_extensions)?;

        // Map extensions to languages using ConfigManager
        for ext in &file_extensions {
            if let Some(language) = self.config_manager.get_language_for_extension(ext) {
                detected_languages.push(language);
            }
        }

        detected_languages.sort();
        detected_languages.dedup();

        // Check available LSPs
        let available_lsps = self.check_available_lsps();

        let info = WorkspaceInfo {
            root_path: self.workspace_root.clone(),
            detected_languages,
            available_lsps,
        };

        self.workspace_info = Some(info.clone());
        Ok(info)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn scan_directory(&self, dir: &Path, extensions: &mut HashSet<String>) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(ext) = path.extension() {
                            if let Some(ext_str) = ext.to_str() {
                                extensions.insert(ext_str.to_string());
                            }
                        }
                    } else if metadata.is_dir() {
                        // Skip common directories that don't contain source code
                        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                            if !matches!(
                                dir_name,
                                "target" | "node_modules" | ".git" | "build" | "dist"
                            ) {
                                self.scan_directory(&path, extensions)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check which LSPs are available in the system
    pub fn check_available_lsps(&self) -> Vec<LspInfo> {
        let mut lsps = Vec::new();

        // Get all supported configurations from ConfigManager
        let configs = self.config_manager.all_configs();

        for config in configs {
            let is_available = std::process::Command::new(&config.command)
                .arg("--version")
                .output()
                .is_ok();

            // Map file extensions to languages using ConfigManager
            let languages: Vec<String> = config
                .file_extensions
                .iter()
                .filter_map(|ext| self.config_manager.get_language_for_extension(ext))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            lsps.push(LspInfo {
                name: config.name,
                command: config.command,
                languages,
                is_available,
                version: None,
            });
        }

        lsps
    }

    /// Check if workspace is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Check if a file is already opened
    pub fn is_file_opened(&self, file_path: &Path) -> bool {
        self.opened_files.get(file_path).map_or(false, |state| state.is_open)
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
        Ok(self
            .workspace_info
            .as_ref()
            .unwrap()
            .detected_languages
            .clone())
    }

    /// Start file watching with patterns based on detected languages
    pub fn start_file_watching(&mut self) -> Result<()> {
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
                    include_patterns.push(format!("**/*.{}", ext));
                }
                // Add exclude patterns from language config
                exclude_patterns.extend(lang_config.exclude_patterns);
            }
        }
        
        let config = FileWatcherConfig {
            include_patterns,
            exclude_patterns,
            respect_gitignore: true,
        };
        
        // Start file watcher
        let file_watcher = FileWatcher::new(self.workspace_root.clone(), tx, config)?;
        
        // Start event processor with workspace manager reference
        let processor = crate::sdk::file_watcher::EventProcessor::new(rx, self as *mut _, self.workspace_root.clone());
        let handle = tokio::spawn(async move {
            processor.run().await;
        });
        
        self._file_watcher = Some(file_watcher);
        self.event_processor_handle = Some(handle);
        
        tracing::info!("🔍 File watching started for languages: {:?}", detected_languages);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

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
    fn test_scan_directory_finds_extensions() {
        let temp_dir = create_temp_workspace(&["test.rs", "test.ts", "test.py"]);
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let mut extensions = HashSet::new();
        
        workspace_manager.scan_directory(temp_dir.path(), &mut extensions).unwrap();
        
        assert!(extensions.contains("rs"));
        assert!(extensions.contains("ts"));
        assert!(extensions.contains("py"));
    }

    #[test]
    fn test_scan_directory_skips_ignored_dirs() {
        let temp_dir = create_temp_workspace(&[
            "src/main.rs",
            "target/debug/app",
            "node_modules/package/index.js",
            ".git/config"
        ]);
        let workspace_manager = WorkspaceManager::new(temp_dir.path().to_path_buf());
        let mut extensions = HashSet::new();
        
        workspace_manager.scan_directory(temp_dir.path(), &mut extensions).unwrap();
        
        assert!(extensions.contains("rs"));
        assert!(!extensions.contains("js")); // Should be skipped from node_modules
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
