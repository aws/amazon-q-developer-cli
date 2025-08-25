use std::path::PathBuf;
use std::sync::{
    Arc,
    LazyLock as Lazy,
};

use eyre::Result;
use semantic_search_client::KnowledgeContext;
use semantic_search_client::client::AsyncSemanticSearchClient;
use semantic_search_client::embedding::EmbeddingType;
use semantic_search_client::types::{
    AddContextRequest,
    SearchResult,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::cli::DEFAULT_AGENT_NAME;
use crate::os::Os;
use crate::util::directories;

/// Configuration for adding knowledge contexts
#[derive(Default)]
pub struct AddOptions {
    pub description: Option<String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub embedding_type: Option<String>,
}

impl AddOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create AddOptions with DB default patterns
    pub fn with_db_defaults(os: &crate::os::Os) -> Self {
        let default_include = os
            .database
            .settings
            .get(crate::database::settings::Setting::KnowledgeDefaultIncludePatterns)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let default_exclude = os
            .database
            .settings
            .get(crate::database::settings::Setting::KnowledgeDefaultExcludePatterns)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let default_embedding_type = os
            .database
            .settings
            .get(crate::database::settings::Setting::KnowledgeIndexType)
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        Self {
            description: None,
            include_patterns: default_include,
            exclude_patterns: default_exclude,
            embedding_type: default_embedding_type,
        }
    }

    pub fn with_include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.include_patterns = patterns;
        self
    }

    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    pub fn with_embedding_type(mut self, embedding_type: Option<String>) -> Self {
        self.embedding_type = embedding_type;
        self
    }
}

#[derive(Debug)]
pub enum KnowledgeError {
    SearchError(String),
}

impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnowledgeError::SearchError(msg) => write!(f, "Search error: {}", msg),
        }
    }
}

impl std::error::Error for KnowledgeError {}

/// Async knowledge store - manages agent specific knowledge bases
pub struct KnowledgeStore {
    agent_client: AsyncSemanticSearchClient,
    agent_dir: PathBuf,
}

impl KnowledgeStore {
    /// Get singleton instance with optional agent
    pub async fn get_async_instance(
        os: &Os,
        agent: Option<&crate::cli::Agent>,
    ) -> Result<Arc<Mutex<Self>>, directories::DirectoryError> {
        static ASYNC_INSTANCE: Lazy<tokio::sync::Mutex<Option<Arc<Mutex<KnowledgeStore>>>>> =
            Lazy::new(|| tokio::sync::Mutex::new(None));

        if cfg!(test) {
            // For tests, create a new instance each time
            let store = Self::new_with_os_settings(os, agent)
                .await
                .map_err(|_e| directories::DirectoryError::Io(std::io::Error::other("Failed to create store")))?;
            Ok(Arc::new(Mutex::new(store)))
        } else {
            let current_agent_dir = crate::util::directories::agent_knowledge_dir(os, agent)?;

            let mut instance_guard = ASYNC_INSTANCE.lock().await;

            let needs_reinit = match instance_guard.as_ref() {
                None => true,
                Some(store) => {
                    let store_guard = store.lock().await;
                    store_guard.agent_dir != current_agent_dir
                },
            };

            if needs_reinit {
                // Check for migration before initializing the client
                Self::migrate_legacy_knowledge_base(&current_agent_dir).await;

                let store = Self::new_with_os_settings(os, agent)
                    .await
                    .map_err(|_e| directories::DirectoryError::Io(std::io::Error::other("Failed to create store")))?;
                *instance_guard = Some(Arc::new(Mutex::new(store)));
            }

            Ok(instance_guard.as_ref().unwrap().clone())
        }
    }

    /// Migrate legacy knowledge base from old location if needed
    async fn migrate_legacy_knowledge_base(agent_dir: &PathBuf) -> bool {
        let mut migrated = false;

        // Extract agent identifier from the directory path (last component)
        let current_agent_id = agent_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(DEFAULT_AGENT_NAME);

        // Migrate from legacy ~/.semantic_search
        let old_flat_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".semantic_search");

        if old_flat_dir.exists() && !agent_dir.exists() {
            if let Some(parent) = agent_dir.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            if std::fs::rename(&old_flat_dir, agent_dir).is_ok() {
                println!(
                    "✅ Migrated knowledge base from {} to {}",
                    old_flat_dir.display(),
                    agent_dir.display()
                );
                return true;
            }
        }

        // Migrate from knowledge_bases root - get file list first to avoid recursion
        if let Some(kb_root) = agent_dir.parent() {
            if kb_root.exists() {
                if let Ok(entries) = std::fs::read_dir(kb_root) {
                    let files_to_migrate: Vec<_> = entries
                        .flatten()
                        .filter(|entry| {
                            let name = entry.file_name();
                            let name_str = name.to_string_lossy();
                            name_str != current_agent_id && name_str != DEFAULT_AGENT_NAME && !name_str.starts_with('.')
                        })
                        .collect();

                    std::fs::create_dir_all(agent_dir).ok();
                    for entry in files_to_migrate {
                        let dst_path = agent_dir.join(entry.file_name());
                        if !dst_path.exists() && std::fs::rename(entry.path(), &dst_path).is_ok() {
                            migrated = true;
                        }
                    }
                }
            }
        }
        migrated
    }

    /// Create SemanticSearchConfig from database settings with fallbacks to defaults
    fn create_config_from_db_settings(
        os: &crate::os::Os,
        base_dir: PathBuf,
    ) -> semantic_search_client::config::SemanticSearchConfig {
        use semantic_search_client::config::SemanticSearchConfig;
        use semantic_search_client::embedding::EmbeddingType;

        use crate::database::settings::Setting;

        // Create default config first
        let default_config = SemanticSearchConfig {
            base_dir: base_dir.clone(),
            ..Default::default()
        };

        // Override with DB settings if provided, otherwise use defaults
        let chunk_size = os
            .database
            .settings
            .get_int_or(Setting::KnowledgeChunkSize, default_config.chunk_size);
        let chunk_overlap = os
            .database
            .settings
            .get_int_or(Setting::KnowledgeChunkOverlap, default_config.chunk_overlap);
        let max_files = os
            .database
            .settings
            .get_int_or(Setting::KnowledgeMaxFiles, default_config.max_files);

        // Get embedding type from settings
        let embedding_type = os
            .database
            .settings
            .get_string(Setting::KnowledgeIndexType)
            .and_then(|s| EmbeddingType::from_str(&s))
            .unwrap_or_default();

        SemanticSearchConfig {
            chunk_size,
            chunk_overlap,
            max_files,
            embedding_type,
            base_dir,
            ..default_config
        }
    }

    /// Create instance with database settings from OS
    async fn new_with_os_settings(os: &crate::os::Os, agent: Option<&crate::cli::Agent>) -> Result<Self> {
        let agent_dir = crate::util::directories::agent_knowledge_dir(os, agent)?;
        let agent_config = Self::create_config_from_db_settings(os, agent_dir.clone());
        let agent_client = AsyncSemanticSearchClient::with_config(&agent_dir, agent_config)
            .await
            .map_err(|e| eyre::eyre!("Failed to create agent client at {}: {}", agent_dir.display(), e))?;

        let store = Self {
            agent_client,
            agent_dir,
        };
        Ok(store)
    }

    /// Add context with flexible options
    pub async fn add(&mut self, name: &str, path_str: &str, options: AddOptions) -> Result<String, String> {
        let path_buf = std::path::PathBuf::from(path_str);
        let canonical_path = path_buf
            .canonicalize()
            .map_err(|_io_error| format!("❌ Path does not exist: {}", path_str))?;

        // Use provided description or generate default
        let description = options
            .description
            .unwrap_or_else(|| format!("Knowledge context for {}", name));

        // Create AddContextRequest with all options
        let request = AddContextRequest {
            path: canonical_path.clone(),
            name: name.to_string(),
            description: if !options.include_patterns.is_empty() || !options.exclude_patterns.is_empty() {
                let mut full_description = description;
                if !options.include_patterns.is_empty() {
                    full_description.push_str(&format!(" [Include: {}]", options.include_patterns.join(", ")));
                }
                if !options.exclude_patterns.is_empty() {
                    full_description.push_str(&format!(" [Exclude: {}]", options.exclude_patterns.join(", ")));
                }
                full_description
            } else {
                description
            },
            persistent: true,
            include_patterns: if options.include_patterns.is_empty() {
                None
            } else {
                Some(options.include_patterns.clone())
            },
            exclude_patterns: if options.exclude_patterns.is_empty() {
                None
            } else {
                Some(options.exclude_patterns.clone())
            },
            embedding_type: match options.embedding_type.as_ref() {
                Some(s) => match EmbeddingType::from_str(s) {
                    Some(et) => Some(et),
                    None => {
                        return Err(format!("Invalid embedding type '{}'. Valid options are: fast, best", s));
                    },
                },
                None => None,
            },
        };

        match self.agent_client.add_context(request).await {
            Ok((operation_id, _)) => {
                let mut message = format!(
                    "🚀 Started indexing '{}'\n📁 Path: {}\n🆔 Operation ID: {}",
                    name,
                    canonical_path.display(),
                    &operation_id.to_string()[..8]
                );
                if !options.include_patterns.is_empty() || !options.exclude_patterns.is_empty() {
                    message.push_str("\n📋 Pattern filtering applied:");
                    if !options.include_patterns.is_empty() {
                        message.push_str(&format!("\n   Include: {}", options.include_patterns.join(", ")));
                    }
                    if !options.exclude_patterns.is_empty() {
                        message.push_str(&format!("\n   Exclude: {}", options.exclude_patterns.join(", ")));
                    }
                    message.push_str("\n✅ Only matching files will be indexed");
                }
                Ok(message)
            },
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("Invalid include pattern") || error_msg.contains("Invalid exclude pattern") {
                    Err(error_msg)
                } else {
                    Err(format!("Failed to start indexing: {}", e))
                }
            },
        }
    }

    /// Get all contexts from agent client
    pub async fn get_all(&self) -> Result<Vec<KnowledgeContext>, String> {
        Ok(self.agent_client.get_contexts().await)
    }

    /// Search - delegates to async client
    pub async fn search(&self, query: &str, context_id: Option<&str>) -> Result<Vec<SearchResult>, KnowledgeError> {
        if let Some(context_id) = context_id {
            // Search specific context
            let results = self
                .agent_client
                .search_context(context_id, query, None)
                .await
                .map_err(|e| KnowledgeError::SearchError(e.to_string()))?;
            Ok(results)
        } else {
            // Search all contexts
            let mut flattened = Vec::new();

            let agent_results = self
                .agent_client
                .search_all(query, None)
                .await
                .map_err(|e| KnowledgeError::SearchError(e.to_string()))?;

            for (_, context_results) in agent_results {
                flattened.extend(context_results);
            }

            flattened.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));

            Ok(flattened)
        }
    }

    /// Get status data
    pub async fn get_status_data(&self) -> Result<semantic_search_client::SystemStatus, String> {
        self.agent_client.get_status_data().await.map_err(|e| e.to_string())
    }

    /// Cancel active operation.
    /// last operation if no operation id is provided.
    pub async fn cancel_operation(&mut self, operation_id: Option<&str>) -> Result<String, String> {
        if let Some(short_id) = operation_id {
            let available_ops = self.agent_client.list_operation_ids().await;
            if available_ops.is_empty() {
                return Ok("No active operations to cancel".to_string());
            }

            // Try to parse as full UUID first
            if let Ok(uuid) = Uuid::parse_str(short_id) {
                self.agent_client
                    .cancel_operation(uuid)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                // Try to find by short ID (first 8 characters)
                if let Some(full_uuid) = self.agent_client.find_operation_by_short_id(short_id).await {
                    self.agent_client
                        .cancel_operation(full_uuid)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    let available_ops_str: Vec<String> =
                        available_ops.iter().map(|id| id.clone()[..8].to_string()).collect();
                    Err(format!(
                        "Operation '{}' not found. Available operations: {}",
                        short_id,
                        available_ops_str.join(", ")
                    ))
                }
            }
        } else {
            // Cancel most recent operation
            self.agent_client
                .cancel_most_recent_operation()
                .await
                .map_err(|e| e.to_string())
        }
    }

    /// Clear all contexts (background operation)
    pub async fn clear(&mut self) -> Result<String, String> {
        match self.agent_client.clear_all().await {
            Ok((operation_id, _cancel_token)) => Ok(format!(
                "🚀 Started clearing all contexts in background.\n📊 Use 'knowledge status' to check progress.\n🆔 Operation ID: {}",
                &operation_id.to_string()[..8]
            )),
            Err(e) => Err(format!("Failed to start clear operation: {}", e)),
        }
    }

    /// Clear all contexts immediately (synchronous operation)
    pub async fn clear_immediate(&mut self) -> Result<String, String> {
        match self.agent_client.clear_all_immediate().await {
            Ok(count) => Ok(format!("✅ Successfully cleared {} knowledge base entries", count)),
            Err(e) => Err(format!("Failed to clear knowledge base: {}", e)),
        }
    }

    /// Remove context by path
    pub async fn remove_by_path(&mut self, path: &str) -> Result<(), String> {
        if let Some(context) = self.agent_client.get_context_by_path(path).await {
            self.agent_client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())
        } else {
            Err(format!("No context found with path '{}'", path))
        }
    }

    /// Remove context by name
    pub async fn remove_by_name(&mut self, name: &str) -> Result<(), String> {
        if let Some(context) = self.agent_client.get_context_by_name(name).await {
            self.agent_client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())
        } else {
            Err(format!("No context found with name '{}'", name))
        }
    }

    /// Remove context by ID
    pub async fn remove_by_id(&mut self, context_id: &str) -> Result<(), String> {
        self.agent_client
            .remove_context_by_id(context_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Update context by path
    pub async fn update_by_path(&mut self, path_str: &str) -> Result<String, String> {
        if let Some(context) = self.agent_client.get_context_by_path(path_str).await {
            // Remove the existing context first
            self.agent_client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())?;

            // Then add it back with the same name and original patterns (agent scope)
            let options = AddOptions {
                description: None,
                include_patterns: context.include_patterns.clone(),
                exclude_patterns: context.exclude_patterns.clone(),
                embedding_type: None,
            };
            self.add(&context.name, path_str, options).await
        } else {
            // Debug: List all available contexts
            let available_paths = self.agent_client.list_context_paths().await;
            if available_paths.is_empty() {
                Err("No contexts found. Add a context first with 'knowledge add <name> <path>'".to_string())
            } else {
                Err(format!(
                    "No context found with path '{}'\nAvailable contexts:\n{}",
                    path_str,
                    available_paths.join("\n")
                ))
            }
        }
    }

    /// Update context by ID
    pub async fn update_context_by_id(&mut self, context_id: &str, path_str: &str) -> Result<String, String> {
        let contexts = self.get_all().await.map_err(|e| e.clone())?;
        let context = contexts
            .iter()
            .find(|c| c.id == context_id)
            .ok_or_else(|| format!("Context '{}' not found", context_id))?;

        let context_name = context.name.clone();

        // Remove the existing context first
        self.agent_client
            .remove_context_by_id(context_id)
            .await
            .map_err(|e| e.to_string())?;

        // Then add it back with the same name and original patterns
        let options = AddOptions {
            description: None,
            include_patterns: context.include_patterns.clone(),
            exclude_patterns: context.exclude_patterns.clone(),
            embedding_type: None,
        };
        self.add(&context_name, path_str, options).await
    }

    /// Update context by name
    pub async fn update_context_by_name(&mut self, name: &str, path_str: &str) -> Result<String, String> {
        if let Some(context) = self.agent_client.get_context_by_name(name).await {
            // Remove the existing context first
            self.agent_client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())?;

            // Then add it back with the same name and original patterns (agent scope)
            let options = AddOptions {
                description: None,
                include_patterns: context.include_patterns.clone(),
                exclude_patterns: context.exclude_patterns.clone(),
                embedding_type: None,
            };
            self.add(name, path_str, options).await
        } else {
            Err(format!("Context with name '{}' not found", name))
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::os::Os;

    async fn create_test_os(temp_dir: &TempDir) -> Os {
        let os = Os::new().await.unwrap();
        // Override home directory to use temp directory
        unsafe {
            os.env.set_var("HOME", temp_dir.path().to_str().unwrap());
        }
        os
    }

    #[tokio::test]
    async fn test_create_config_from_db_settings() {
        let temp_dir = TempDir::new().unwrap();
        let os = create_test_os(&temp_dir).await;
        let base_dir = temp_dir.path().join("test_kb");

        // Test config creation with default settings
        let config = KnowledgeStore::create_config_from_db_settings(&os, base_dir.clone());

        // Should use defaults when no database settings exist
        assert_eq!(config.chunk_size, 512); // Default chunk size
        assert_eq!(config.chunk_overlap, 128); // Default chunk overlap
        assert_eq!(config.max_files, 10000); // Default max files
        assert_eq!(config.base_dir, base_dir);
    }

    #[tokio::test]
    async fn test_knowledge_bases_dir_structure() {
        let temp_dir = TempDir::new().unwrap();
        let os = create_test_os(&temp_dir).await;

        let base_dir = crate::util::directories::knowledge_bases_dir(&os).unwrap();

        // Verify directory structure
        assert!(base_dir.to_string_lossy().contains("knowledge_bases"));
    }
}
