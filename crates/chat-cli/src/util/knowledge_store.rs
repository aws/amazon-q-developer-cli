use std::path::PathBuf;
use std::sync::{
    Arc,
    LazyLock as Lazy,
};

use eyre::Result;
use semantic_search_client::KnowledgeContext;
use semantic_search_client::client::AsyncSemanticSearchClient;
use semantic_search_client::types::{
    AddContextRequest,
    SearchResult,
};
use tokio::sync::Mutex;
use tracing::debug;
use uuid::Uuid;

use crate::os::Os;
use crate::util::directories;

/// Configuration for adding knowledge contexts
#[derive(Default)]
pub struct AddOptions {
    pub description: Option<String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
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

        Self {
            description: None,
            include_patterns: default_include,
            exclude_patterns: default_exclude,
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
}

#[derive(Debug)]
pub enum KnowledgeError {
    ClientError(String),
}

impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnowledgeError::ClientError(msg) => write!(f, "Client error: {}", msg),
        }
    }
}

impl std::error::Error for KnowledgeError {}

/// Async knowledge store - just a thin wrapper!
pub struct KnowledgeStore {
    client: AsyncSemanticSearchClient,
}

impl KnowledgeStore {
    /// Get singleton instance with directory from OS (includes migration)
    pub async fn get_async_instance_with_os(os: &Os) -> Result<Arc<Mutex<Self>>, directories::DirectoryError> {
        let knowledge_dir = crate::util::directories::knowledge_bases_dir(os)?;
        Self::migrate_legacy_knowledge_base(&knowledge_dir).await;
        Ok(Self::get_async_instance_with_os_settings(os, knowledge_dir).await)
    }

    /// Migrate legacy knowledge base from old location if needed
    async fn migrate_legacy_knowledge_base(knowledge_dir: &PathBuf) {
        let old_flat_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".semantic_search");

        if old_flat_dir.exists() && !knowledge_dir.exists() {
            // Create parent directories first
            if let Some(parent) = knowledge_dir.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    debug!(
                        "Warning: Failed to create parent directories for knowledge base migration: {}",
                        e
                    );
                    return;
                }
            }

            // Attempt migration
            if let Err(e) = std::fs::rename(&old_flat_dir, knowledge_dir) {
                debug!(
                    "Warning: Failed to migrate legacy knowledge base from {} to {}: {}",
                    old_flat_dir.display(),
                    knowledge_dir.display(),
                    e
                );
            } else {
                println!(
                    "✅ Migrated knowledge base from {} to {}",
                    old_flat_dir.display(),
                    knowledge_dir.display()
                );
            }
        }
    }

    /// Get singleton instance with OS settings (primary method)
    pub async fn get_async_instance_with_os_settings(os: &crate::os::Os, base_dir: PathBuf) -> Arc<Mutex<Self>> {
        static ASYNC_INSTANCE: Lazy<tokio::sync::OnceCell<Arc<Mutex<KnowledgeStore>>>> =
            Lazy::new(tokio::sync::OnceCell::new);

        if cfg!(test) {
            Arc::new(Mutex::new(
                KnowledgeStore::new_with_os_settings(os, base_dir)
                    .await
                    .expect("Failed to create test async knowledge store"),
            ))
        } else {
            ASYNC_INSTANCE
                .get_or_init(|| async {
                    Arc::new(Mutex::new(
                        KnowledgeStore::new_with_os_settings(os, base_dir)
                            .await
                            .expect("Failed to create async knowledge store"),
                    ))
                })
                .await
                .clone()
        }
    }

    /// Create SemanticSearchConfig from database settings with fallbacks to defaults
    fn create_config_from_db_settings(
        os: &crate::os::Os,
        base_dir: PathBuf,
    ) -> semantic_search_client::config::SemanticSearchConfig {
        use semantic_search_client::config::SemanticSearchConfig;

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

        SemanticSearchConfig {
            chunk_size,
            chunk_overlap,
            max_files,
            base_dir,
            ..default_config
        }
    }

    /// Create instance with database settings from OS
    pub async fn new_with_os_settings(os: &crate::os::Os, base_dir: PathBuf) -> Result<Self> {
        let config = Self::create_config_from_db_settings(os, base_dir.clone());
        let client = AsyncSemanticSearchClient::with_config(&base_dir, config)
            .await
            .map_err(|e| eyre::eyre!("Failed to create client: {}", e))?;

        Ok(Self { client })
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
        };

        match self.client.add_context(request).await {
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

    pub async fn get_all(&self) -> Result<Vec<KnowledgeContext>, KnowledgeError> {
        Ok(self.client.get_contexts().await)
    }

    /// Search - delegates to async client
    pub async fn search(&self, query: &str, _context_id: Option<&str>) -> Result<Vec<SearchResult>, KnowledgeError> {
        let results = self
            .client
            .search_all(query, None)
            .await
            .map_err(|e| KnowledgeError::ClientError(e.to_string()))?;

        let mut flattened = Vec::new();
        for (_, context_results) in results {
            flattened.extend(context_results);
        }

        flattened.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));

        Ok(flattened)
    }

    /// Get status data - delegates to async client
    pub async fn get_status_data(&self) -> Result<semantic_search_client::SystemStatus, String> {
        self.client
            .get_status_data()
            .await
            .map_err(|e| format!("Failed to get status data: {}", e))
    }

    /// Cancel operation - delegates to async client
    pub async fn cancel_operation(&mut self, operation_id: Option<&str>) -> Result<String, String> {
        if let Some(short_id) = operation_id {
            // Debug: List all available operations
            let available_ops = self.client.list_operation_ids().await;
            if available_ops.is_empty() {
                return Err("No active operations found".to_string());
            }

            // Try to parse as full UUID first
            if let Ok(uuid) = Uuid::parse_str(short_id) {
                self.client.cancel_operation(uuid).await.map_err(|e| e.to_string())
            } else {
                // Try to find by short ID (first 8 characters)
                if let Some(full_uuid) = self.client.find_operation_by_short_id(short_id).await {
                    self.client.cancel_operation(full_uuid).await.map_err(|e| e.to_string())
                } else {
                    Err(format!(
                        "No operation found matching ID: {}\nAvailable operations:\n{}",
                        short_id,
                        available_ops.join("\n")
                    ))
                }
            }
        } else {
            // Cancel most recent operation (not all operations)
            self.client
                .cancel_most_recent_operation()
                .await
                .map_err(|e| e.to_string())
        }
    }

    /// Clear all contexts (background operation)
    pub async fn clear(&mut self) -> Result<String, String> {
        match self.client.clear_all().await {
            Ok((operation_id, _cancel_token)) => Ok(format!(
                "🚀 Started clearing all contexts in background.\n📊 Use 'knowledge status' to check progress.\n🆔 Operation ID: {}",
                &operation_id.to_string()[..8]
            )),
            Err(e) => Err(format!("Failed to start clear operation: {}", e)),
        }
    }

    /// Clear all contexts immediately (synchronous operation)
    pub async fn clear_immediate(&mut self) -> Result<String, String> {
        match self.client.clear_all_immediate().await {
            Ok(count) => Ok(format!("✅ Successfully cleared {} knowledge base entries", count)),
            Err(e) => Err(format!("Failed to clear knowledge base: {}", e)),
        }
    }

    /// Remove context by path
    pub async fn remove_by_path(&mut self, path: &str) -> Result<(), String> {
        if let Some(context) = self.client.get_context_by_path(path).await {
            self.client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())
        } else {
            Err(format!("No context found with path '{}'", path))
        }
    }

    /// Remove context by name
    pub async fn remove_by_name(&mut self, name: &str) -> Result<(), String> {
        if let Some(context) = self.client.get_context_by_name(name).await {
            self.client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())
        } else {
            Err(format!("No context found with name '{}'", name))
        }
    }

    /// Remove context by ID
    pub async fn remove_by_id(&mut self, context_id: &str) -> Result<(), String> {
        self.client
            .remove_context_by_id(context_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Update context by path
    pub async fn update_by_path(&mut self, path_str: &str) -> Result<String, String> {
        if let Some(context) = self.client.get_context_by_path(path_str).await {
            // Remove the existing context first
            self.client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())?;

            // Then add it back with the same name and original patterns
            let options = AddOptions {
                description: None,
                include_patterns: context.include_patterns.clone(),
                exclude_patterns: context.exclude_patterns.clone(),
            };
            self.add(&context.name, path_str, options).await
        } else {
            // Debug: List all available contexts
            let available_paths = self.client.list_context_paths().await;
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
        let contexts = self.get_all().await.map_err(|e| e.to_string())?;
        let context = contexts
            .iter()
            .find(|c| c.id == context_id)
            .ok_or_else(|| format!("Context '{}' not found", context_id))?;

        let context_name = context.name.clone();

        // Remove the existing context first
        self.client
            .remove_context_by_id(context_id)
            .await
            .map_err(|e| e.to_string())?;

        // Then add it back with the same name and original patterns
        let options = AddOptions {
            description: None,
            include_patterns: context.include_patterns.clone(),
            exclude_patterns: context.exclude_patterns.clone(),
        };
        self.add(&context_name, path_str, options).await
    }

    /// Update context by name
    pub async fn update_context_by_name(&mut self, name: &str, path_str: &str) -> Result<String, String> {
        if let Some(context) = self.client.get_context_by_name(name).await {
            // Remove the existing context first
            self.client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())?;

            // Then add it back with the same name and original patterns
            let options = AddOptions {
                description: None,
                include_patterns: context.include_patterns.clone(),
                exclude_patterns: context.exclude_patterns.clone(),
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
