use std::path::{
    Path,
    PathBuf,
};
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

use crate::constants::DEFAULT_AGENT_NAME;
use crate::os::Os;
use crate::util::paths;
use crate::util::paths::PathResolver;

/// Formats knowledge bases as a concise context string
pub fn format_knowledge_bases_as_context(contexts: &[Arc<KnowledgeContext>]) -> String {
    let mut output = String::from("Available Knowledge Bases:\n\n");
    for ctx in contexts {
        output.push_str(&format!(
            "- {} (ID: {}, Type: {:?})\n",
            ctx.name, ctx.id, ctx.embedding_type
        ));
        if let Some(path) = &ctx.source_path {
            output.push_str(&format!("  Path: {path}\n"));
        }
        if !ctx.description.is_empty() {
            output.push_str(&format!("  Description: {}\n", ctx.description));
        }
    }
    output
}

/// Retrieves and formats available knowledge bases for context injection
pub async fn get_available_knowledge_bases(
    os: &Os,
    agent_name: Option<&str>,
    agent_path: Option<&Path>,
) -> Option<String> {
    let store = KnowledgeStore::get_async_instance(os, agent_name, agent_path)
        .await
        .ok()?;
    let store_guard = store.lock().await;
    let contexts = store_guard.get_all().await.ok()?;

    if contexts.is_empty() {
        return None;
    }

    Some(format_knowledge_bases_as_context(&contexts))
}

/// Generate a unique identifier for an agent based on its path and name
fn generate_agent_unique_id(name: &str, path: Option<&Path>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{
        Hash,
        Hasher,
    };

    if let Some(path) = path {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let path_hash = hasher.finish();
        format!("{}_{:x}", name, path_hash)
    } else {
        name.to_string()
    }
}

/// Get the knowledge base directory path for a specific agent
fn agent_knowledge_dir(
    os: &Os,
    agent_name: Option<&str>,
    agent_path: Option<&Path>,
) -> Result<PathBuf, paths::DirectoryError> {
    let unique_id = if let Some(name) = agent_name {
        generate_agent_unique_id(name, agent_path)
    } else {
        DEFAULT_AGENT_NAME.to_string()
    };
    Ok(PathResolver::new(os).global().knowledge_bases_dir()?.join(unique_id))
}

/// Configuration for adding knowledge contexts
#[derive(Default)]
pub struct AddOptions {
    pub description: Option<String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub embedding_type: Option<String>,
    pub auto_sync: bool,
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
            auto_sync: false,
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
            KnowledgeError::SearchError(msg) => write!(f, "Search error: {msg}"),
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
        agent_name: Option<&str>,
        agent_path: Option<&Path>,
    ) -> Result<Arc<Mutex<Self>>, paths::DirectoryError> {
        static ASYNC_INSTANCE: Lazy<tokio::sync::Mutex<Option<Arc<Mutex<KnowledgeStore>>>>> =
            Lazy::new(|| tokio::sync::Mutex::new(None));

        if cfg!(test) {
            // For tests, create a new instance each time
            let store = Self::new_with_os_settings(os, agent_name, agent_path)
                .await
                .map_err(|_e| paths::DirectoryError::Io(std::io::Error::other("Failed to create store")))?;
            Ok(Arc::new(Mutex::new(store)))
        } else {
            let current_agent_dir = agent_knowledge_dir(os, agent_name, agent_path)?;

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

                let store = Self::new_with_os_settings(os, agent_name, agent_path)
                    .await
                    .map_err(|_e| paths::DirectoryError::Io(std::io::Error::other("Failed to create store")))?;
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

        // Migrate from knowledge_bases root - get file list first to avoid recursion
        if let Some(kb_root) = agent_dir.parent()
            && kb_root.exists()
            && let Ok(entries) = std::fs::read_dir(kb_root)
        {
            let files_to_migrate: Vec<_> = entries
                .flatten()
                .filter(|entry| {
                    let path = entry.path();
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    // Only migrate FILES, not directories (to avoid moving other agent directories)
                    path.is_file()
                        && name_str != current_agent_id
                        && name_str != DEFAULT_AGENT_NAME
                        && !name_str.starts_with('.')
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
    async fn new_with_os_settings(
        os: &crate::os::Os,
        agent_name: Option<&str>,
        agent_path: Option<&Path>,
    ) -> Result<Self> {
        let agent_dir = agent_knowledge_dir(os, agent_name, agent_path)?;
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

        // Validate path exists (canonicalize for validation only)
        let _ = path_buf
            .canonicalize()
            .map_err(|_io_error| format!("Path does not exist: {path_str}"))?;

        // Use provided description or generate default
        let description = options
            .description
            .unwrap_or_else(|| format!("Knowledge context for {name}"));

        // Create AddContextRequest with original path (preserves symlinks)
        let request = AddContextRequest {
            path: path_buf.clone(),
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
                        return Err(format!("Invalid embedding type '{s}'. Valid options are: fast, best"));
                    },
                },
                None => None,
            },
            auto_sync: options.auto_sync,
        };

        match self.agent_client.add_context(request).await {
            Ok((operation_id, _)) => {
                let mut message = format!(
                    "Started indexing '{}'\nPath: {}\nOperation ID: {}",
                    name,
                    path_buf.display(),
                    &operation_id.to_string()[..8]
                );
                if !options.include_patterns.is_empty() || !options.exclude_patterns.is_empty() {
                    message.push_str("\nPattern filtering applied:");
                    if !options.include_patterns.is_empty() {
                        message.push_str(&format!("\n   Include: {}", options.include_patterns.join(", ")));
                    }
                    if !options.exclude_patterns.is_empty() {
                        message.push_str(&format!("\n   Exclude: {}", options.exclude_patterns.join(", ")));
                    }
                    message.push_str("\nOnly matching files will be indexed");
                }
                Ok(message)
            },
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("Invalid include pattern") || error_msg.contains("Invalid exclude pattern") {
                    Err(error_msg)
                } else {
                    Err(format!("Failed to start indexing: {e}"))
                }
            },
        }
    }

    /// Get all contexts from agent client
    pub async fn get_all(&self) -> Result<Vec<Arc<KnowledgeContext>>, String> {
        Ok(self.agent_client.get_contexts().await)
    }

    /// Search with pagination support
    pub async fn search_paginated(
        &self,
        query: &str,
        context_id: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<SearchResult>, KnowledgeError> {
        if let Some(context_id) = context_id {
            // Search specific context
            let results = self
                .agent_client
                .search_context_paginated(context_id, query, limit, offset)
                .await
                .map_err(|e| KnowledgeError::SearchError(e.to_string()))?;
            Ok(results)
        } else {
            // Search all contexts
            let mut flattened = Vec::new();

            let agent_results = self
                .agent_client
                .search_all_paginated(query, limit, offset)
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
                "Started clearing all contexts in background.\nUse '/knowledge show' to check progress.\nOperation ID: {}",
                &operation_id.to_string()[..8]
            )),
            Err(e) => Err(format!("Failed to start clear operation: {e}")),
        }
    }

    /// Clear all contexts immediately (synchronous operation)
    pub async fn clear_immediate(&mut self) -> Result<String, String> {
        match self.agent_client.clear_all_immediate().await {
            Ok(count) => Ok(format!("Successfully cleared {count} knowledge base entries")),
            Err(e) => Err(format!("Failed to clear knowledge base: {e}")),
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
            Err(format!("No context found with path '{path}'"))
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
            Err(format!("No context found with name '{name}'"))
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
                embedding_type: Some(context.embedding_type.to_string().to_owned()),
                auto_sync: context.auto_sync,
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
            .ok_or_else(|| format!("Context '{context_id}' not found"))?;

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
            embedding_type: Some(context.embedding_type.to_string().to_owned()),
            auto_sync: context.auto_sync,
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
                embedding_type: Some(context.embedding_type.to_string().to_owned()),
                auto_sync: context.auto_sync,
            };
            self.add(name, path_str, options).await
        } else {
            Err(format!("Context with name '{name}' not found"))
        }
    }

    /// Sync agent resources to knowledge store
    /// - Resources from agent schema are marked as auto_sync=true
    /// - Only auto-synced resources are removed when removed from schema
    /// - Manual /knowledge add resources (auto_sync=false) persist across schema changes
    pub async fn sync_agent_resources(
        name: &str,
        path: Option<&Path>,
        resources: &[agent::agent_config::types::ResourcePath],
        os: &Os,
    ) -> Result<(), String> {
        let knowledge_store_arc = Self::get_async_instance(os, Some(name), path)
            .await
            .map_err(|e| e.to_string())?;
        let mut knowledge_store = knowledge_store_arc.lock().await;

        // Extract indexed resources from agent config
        let current_indexed_resources: Vec<_> = resources
            .iter()
            .filter_map(|resource| {
                use agent::agent_config::types::{
                    ComplexResource,
                    ResourcePath,
                };
                match resource {
                    ResourcePath::Complex(ComplexResource::KnowledgeBase {
                        source,
                        name,
                        description,
                        index_type,
                        include,
                        exclude,
                        auto_update,
                    }) => {
                        let file_path = source.trim_start_matches("file://");

                        // Use sanitize_path_tool_arg to handle ~ expansion and relative paths
                        let resolved_path = crate::cli::chat::legacy::sanitize_path_tool_arg(os, file_path);

                        Some((
                            name.as_deref().unwrap_or("unnamed"),
                            description
                                .as_deref()
                                .unwrap_or_else(|| name.as_deref().unwrap_or("unnamed")),
                            resolved_path.to_string_lossy().to_string(),
                            include.clone(),
                            exclude.clone(),
                            Self::index_type_to_string(index_type.as_ref()),
                            auto_update.unwrap_or(false),
                        ))
                    },
                    _ => None,
                }
            })
            .collect();

        let existing_contexts = knowledge_store.get_all().await.unwrap_or_default();

        // Remove auto-synced contexts no longer in agent schema
        for ctx in &existing_contexts {
            if ctx.auto_sync {
                let still_exists = current_indexed_resources
                    .iter()
                    .any(|(name, _, path, _, _, _, _)| ctx.name == *name || ctx.source_path.as_deref() == Some(path));

                if !still_exists {
                    let _ = knowledge_store.remove_by_name(&ctx.name).await;
                }
            }
        }

        // Add or update indexed resources from agent schema
        for (name, description, resolved_path, include, exclude, index_type, auto_update) in current_indexed_resources {
            let existing_ctx = existing_contexts
                .iter()
                .find(|ctx| ctx.name == name || ctx.source_path.as_deref() == Some(&resolved_path));

            let options = Self::build_sync_options(os, description, include, exclude, index_type);

            if let Some(ctx) = existing_ctx {
                if Self::should_reindex(ctx, &options, auto_update) {
                    let _ = knowledge_store.remove_by_name(&ctx.name).await;
                    let _ = knowledge_store.add(name, &resolved_path, options).await;
                }
            } else {
                let _ = knowledge_store.add(name, &resolved_path, options).await;
            }
        }

        Ok(())
    }

    /// Build AddOptions for an agent resource, starting from global defaults
    /// and overriding with agent config values.
    fn build_sync_options(
        os: &Os,
        description: &str,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        index_type: Option<String>,
    ) -> AddOptions {
        let mut options = AddOptions::with_db_defaults(os);
        options.auto_sync = true;
        options.description = Some(description.to_string());

        if let Some(include) = include {
            options.include_patterns = include;
        }

        if let Some(exclude) = exclude {
            options.exclude_patterns = exclude;
        }

        if let Some(index_type) = index_type {
            options.embedding_type = Some(index_type);
        }

        options
    }

    /// Determine whether an existing context needs re-indexing.
    fn should_reindex(ctx: &KnowledgeContext, options: &AddOptions, auto_update: bool) -> bool {
        // Element-order-sensitive: ["a","b"] != ["b","a"] will cause a
        // harmless extra re-index cycle rather than a missed update.
        let patterns_changed =
            ctx.include_patterns != options.include_patterns || ctx.exclude_patterns != options.exclude_patterns;

        auto_update || patterns_changed
    }

    fn index_type_to_string(index_type: Option<&agent::agent_config::types::IndexType>) -> Option<String> {
        use agent::agent_config::types::IndexType;
        index_type.map(|it| match it {
            IndexType::Fast => "fast".to_string(),
            IndexType::Best => "best".to_string(),
        })
    }
}

/// Wraps an `Arc<Mutex<KnowledgeStore>>` so it can be injected into the agent
/// crate as a `dyn KnowledgeProvider`.
pub struct KnowledgeStoreProvider {
    store: Arc<Mutex<KnowledgeStore>>,
}

impl KnowledgeStoreProvider {
    pub fn new(store: Arc<Mutex<KnowledgeStore>>) -> Self {
        Self { store }
    }
}

impl std::fmt::Debug for KnowledgeStoreProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowledgeStoreProvider").finish()
    }
}

#[async_trait::async_trait]
impl agent::tools::KnowledgeProvider for KnowledgeStoreProvider {
    async fn execute(&self, command: agent::tools::Knowledge) -> agent::tools::ToolExecutionResult {
        use agent::tools::{
            Knowledge,
            ToolExecutionError,
            ToolExecutionOutput,
            ToolExecutionOutputItem,
        };

        let result = {
            let mut store = self.store.lock().await;
            match command {
                Knowledge::Show => format_show(&store).await,
                Knowledge::Status => format_status(&store).await,
                Knowledge::Add { name, value } => {
                    let options = AddOptions::new();
                    match store.add(&name, &value, options).await {
                        Ok(id) => Ok(format!(
                            "Added '{name}' to knowledge base with ID: {id}. Use 'show' to track progress."
                        )),
                        Err(e) => Err(format!("Failed to add: {e}")),
                    }
                },
                Knowledge::Remove { name, context_id, path } => {
                    execute_remove(&mut store, &name, &context_id, &path).await
                },
                Knowledge::Clear => store.clear().await,
                Knowledge::Search {
                    query,
                    context_id,
                    limit,
                    offset,
                    snippet_length,
                    sort_by,
                    file_type,
                } => {
                    execute_search(&store, &query, SearchParams {
                        context_id: context_id.as_deref(),
                        limit,
                        offset,
                        snippet_length,
                        sort_by: sort_by.as_deref(),
                        file_type: file_type.as_deref(),
                    })
                    .await
                },
                Knowledge::Update { path, context_id, name } => {
                    execute_update(&mut store, &path, &context_id, &name).await
                },
                Knowledge::Cancel { operation_id } => store.cancel_operation(operation_id.as_deref()).await,
            }
        };

        result
            .map(|text| ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(text)]))
            .map_err(ToolExecutionError::Custom)
    }
}

async fn execute_remove(
    store: &mut KnowledgeStore,
    name: &str,
    context_id: &str,
    path: &str,
) -> Result<String, String> {
    if !context_id.is_empty() {
        store
            .remove_by_id(context_id)
            .await
            .map(|()| format!("Removed context '{context_id}'"))
    } else if !name.is_empty() {
        store
            .remove_by_name(name)
            .await
            .map(|()| format!("Removed context '{name}'"))
    } else if !path.is_empty() {
        store
            .remove_by_path(path)
            .await
            .map(|()| format!("Removed context with path '{path}'"))
    } else {
        Err("No identifier provided. Specify name, context_id, or path.".into())
    }
}

struct SearchParams<'a> {
    context_id: Option<&'a str>,
    limit: Option<usize>,
    offset: Option<usize>,
    snippet_length: Option<usize>,
    sort_by: Option<&'a str>,
    file_type: Option<&'a str>,
}

async fn execute_search(store: &KnowledgeStore, query: &str, params: SearchParams<'_>) -> Result<String, String> {
    let mut results = store
        .search_paginated(query, params.context_id, params.limit, params.offset)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(ft) = params.file_type {
        results.retain(|r| {
            r.point
                .payload
                .get("file_type")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.to_lowercase().contains(&ft.to_lowercase()))
        });
    }

    if let Some(sort) = params.sort_by {
        match sort {
            "path" | "name" => results.sort_by(|a, b| {
                let pa = a.point.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let pb = b.point.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
                pa.cmp(pb)
            }),
            "relevance" => {
                results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
            },
            _ => {},
        }
    }

    if results.is_empty() {
        return Ok(format!("No results found for '{query}'"));
    }

    let mut out = format!("Found {} results for '{query}'\n\n", results.len());
    for (i, r) in results.iter().enumerate() {
        let path = r
            .point
            .payload
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let chunk = r.point.payload.get("chunk_index").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = r
            .point
            .payload
            .get("total_chunks")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let ft = r
            .point
            .payload
            .get("file_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        out.push_str(&format!(
            "Result {}:\nFile: {path}\nChunk: {}/{total} | Type: {ft} | Score: {:.4}\n",
            i + 1,
            chunk + 1,
            r.distance
        ));
        if let Some(text) = r.text() {
            let display = match params.snippet_length {
                Some(max) if text.len() > max => format!("{}...", &text[..max]),
                _ => text.to_string(),
            };
            out.push_str(&format!("---\n{display}\n---\n\n"));
        }
    }
    Ok(out)
}

async fn execute_update(
    store: &mut KnowledgeStore,
    path: &str,
    context_id: &str,
    name: &str,
) -> Result<String, String> {
    if path.is_empty() {
        return Err("No path provided for update.".into());
    }
    if !context_id.is_empty() {
        store.update_context_by_id(context_id, path).await
    } else if !name.is_empty() {
        store.update_context_by_name(name, path).await
    } else {
        store.update_by_path(path).await
    }
}

async fn format_show(store: &KnowledgeStore) -> Result<String, String> {
    let contexts = store.get_all().await.unwrap_or_default();
    let status = store.get_status_data().await;

    let mut out = String::new();
    if contexts.is_empty() {
        out.push_str("No knowledge base entries found\n");
    } else {
        out.push_str("Knowledge base entries:\n");
        for c in &contexts {
            out.push_str(&format!(
                "- ID: {}\n  Name: {}\n  Description: {}\n  Items: {}\n\n",
                c.id, c.name, c.description, c.item_count
            ));
        }
    }
    if let Ok(s) = status {
        out.push_str(&format_status_display(&s));
    }
    Ok(out)
}

async fn format_status(store: &KnowledgeStore) -> Result<String, String> {
    let status = store.get_status_data().await?;
    Ok(format_status_display(&status))
}

fn format_status_display(status: &semantic_search_client::SystemStatus) -> String {
    if status.operations.is_empty() {
        return "No active operations".to_string();
    }
    let mut out = String::new();
    for op in &status.operations {
        out.push_str(&format!("{} ({})\n", op.operation_type.display_name(), &op.short_id));
        let desc = match &op.operation_type {
            semantic_search_client::OperationType::Indexing { path, .. } => path.clone(),
            semantic_search_client::OperationType::Clearing => op.message.clone(),
        };
        out.push_str(&format!("   {desc}\n"));
        if op.is_cancelled {
            out.push_str("   Cancelled\n");
        } else if op.is_failed {
            out.push_str("   Failed\n");
        } else if op.total > 0 {
            let pct = (op.current as f64 / op.total as f64 * 100.0) as u8;
            match op.eta {
                Some(eta) => out.push_str(&format!("   {pct}% • ETA: {}s\n", eta.as_secs())),
                None => out.push_str(&format!("   {pct}%\n")),
            }
        } else {
            out.push_str("   In progress\n");
        }
    }
    out.trim_end().to_string()
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

        let base_dir = crate::util::paths::PathResolver::new(&os)
            .global()
            .knowledge_bases_dir()
            .unwrap();

        // Verify directory structure
        assert!(base_dir.to_string_lossy().contains("knowledge_bases"));
    }

    #[tokio::test]
    async fn test_add_options_with_db_defaults_picks_up_global_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let mut os = create_test_os(&temp_dir).await;

        os.database
            .settings
            .set(
                crate::database::settings::Setting::KnowledgeDefaultExcludePatterns,
                serde_json::json!([".obsidian/**", "Attachments/**"]),
            )
            .await
            .unwrap();
        os.database
            .settings
            .set(
                crate::database::settings::Setting::KnowledgeDefaultIncludePatterns,
                serde_json::json!(["**/*.md"]),
            )
            .await
            .unwrap();

        let options = AddOptions::with_db_defaults(&os);

        assert_eq!(options.include_patterns, vec!["**/*.md"]);
        assert_eq!(options.exclude_patterns, vec![".obsidian/**", "Attachments/**"]);
    }

    #[tokio::test]
    async fn test_build_sync_options_agent_config_overrides_global_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let mut os = create_test_os(&temp_dir).await;

        os.database
            .settings
            .set(
                crate::database::settings::Setting::KnowledgeDefaultExcludePatterns,
                serde_json::json!(["node_modules/**"]),
            )
            .await
            .unwrap();

        let options = KnowledgeStore::build_sync_options(
            &os,
            "test",
            Some(vec!["**/*.md".to_string()]),
            Some(vec![".obsidian/**".to_string()]),
            None,
        );

        assert_eq!(options.include_patterns, vec!["**/*.md"]);
        assert_eq!(options.exclude_patterns, vec![".obsidian/**"]);
        assert!(options.auto_sync);
    }

    #[tokio::test]
    async fn test_build_sync_options_uses_global_defaults_when_agent_has_none() {
        let temp_dir = TempDir::new().unwrap();
        let mut os = create_test_os(&temp_dir).await;

        os.database
            .settings
            .set(
                crate::database::settings::Setting::KnowledgeDefaultExcludePatterns,
                serde_json::json!(["node_modules/**", ".git/**"]),
            )
            .await
            .unwrap();

        let options = KnowledgeStore::build_sync_options(&os, "test", None, None, None);

        assert!(options.include_patterns.is_empty());
        assert_eq!(options.exclude_patterns, vec!["node_modules/**", ".git/**"]);
    }

    #[tokio::test]
    async fn test_build_sync_options_also_applies_global_embedding_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut os = create_test_os(&temp_dir).await;

        os.database
            .settings
            .set(
                crate::database::settings::Setting::KnowledgeIndexType,
                serde_json::json!("best"),
            )
            .await
            .unwrap();

        let options = KnowledgeStore::build_sync_options(&os, "test", None, None, None);
        assert_eq!(options.embedding_type.as_deref(), Some("best"));

        let options = KnowledgeStore::build_sync_options(&os, "test", None, None, Some("fast".to_string()));
        assert_eq!(options.embedding_type.as_deref(), Some("fast"));
    }

    #[test]
    fn test_should_reindex_detects_pattern_change() {
        let ctx = KnowledgeContext::new(
            "id".to_string(),
            "test",
            "desc",
            true,
            None,
            (vec![], vec![]),
            0,
            semantic_search_client::embedding::EmbeddingType::Fast,
            true,
        );

        // Same patterns, auto_update false → no re-index
        let options = AddOptions {
            include_patterns: vec![],
            exclude_patterns: vec![],
            ..Default::default()
        };
        assert!(!KnowledgeStore::should_reindex(&ctx, &options, false));

        // Same patterns, auto_update true → re-index
        assert!(KnowledgeStore::should_reindex(&ctx, &options, true));

        // Different include patterns → re-index even without auto_update
        let options = AddOptions {
            include_patterns: vec!["**/*.md".to_string()],
            exclude_patterns: vec![],
            ..Default::default()
        };
        assert!(KnowledgeStore::should_reindex(&ctx, &options, false));

        // Different exclude patterns → re-index
        let options = AddOptions {
            include_patterns: vec![],
            exclude_patterns: vec![".obsidian/**".to_string()],
            ..Default::default()
        };
        assert!(KnowledgeStore::should_reindex(&ctx, &options, false));
    }
}
