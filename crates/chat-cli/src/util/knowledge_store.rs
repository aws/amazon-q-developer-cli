use std::path::Path;
use std::sync::{
    Arc,
    LazyLock as Lazy,
};

use eyre::Result;
use semantic_search_client::KnowledgeContext;
use semantic_search_client::client::AsyncSemanticSearchClient;
use semantic_search_client::types::SearchResult;
use tokio::sync::Mutex;
use uuid::Uuid;

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
    /// Get singleton instance
    pub async fn get_async_instance(path: Option<impl AsRef<Path>>) -> Arc<Mutex<Self>> {
        static ASYNC_INSTANCE: Lazy<tokio::sync::OnceCell<Arc<Mutex<KnowledgeStore>>>> =
            Lazy::new(tokio::sync::OnceCell::new);

        if cfg!(test) {
            Arc::new(Mutex::new(
                KnowledgeStore::new(path)
                    .await
                    .expect("Failed to create test async knowledge store"),
            ))
        } else {
            ASYNC_INSTANCE
                .get_or_init(|| async {
                    Arc::new(Mutex::new(
                        KnowledgeStore::new(path)
                            .await
                            .expect("Failed to create async knowledge store"),
                    ))
                })
                .await
                .clone()
        }
    }

    pub async fn new(path: Option<impl AsRef<Path>>) -> Result<Self> {
        match path {
            Some(path) => {
                let client = AsyncSemanticSearchClient::new(path)
                    .await
                    .map_err(|e| eyre::eyre!("Failed to create client: {}", e))?;
                Ok(Self { client })
            },
            None => {
                let client = AsyncSemanticSearchClient::new_with_default_dir()
                    .await
                    .map_err(|e| eyre::eyre!("Failed to create client: {}", e))?;
                Ok(Self { client })
            },
        }
    }

    /// Add context - delegates to async client
    pub async fn add(&mut self, name: &str, path_str: &str) -> Result<String, String> {
        let path_buf = std::path::PathBuf::from(path_str);
        let canonical_path = path_buf
            .canonicalize()
            .map_err(|_io_error| format!("❌ Path does not exist: {}", path_str))?;

        match self
            .client
            .add_context_from_path(&canonical_path, name, &format!("Knowledge context for {}", name), true)
            .await
        {
            Ok((operation_id, _)) => Ok(format!(
                "🚀 Started indexing '{}'\n📁 Path: {}\n🆔 Operation ID: {}.",
                name,
                canonical_path.display(),
                &operation_id.to_string()[..8]
            )),
            Err(e) => Err(format!("Failed to start indexing: {}", e)),
        }
    }

    /// Get all contexts - delegates to async client
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
            // Cancel all operations
            self.client.cancel_all_operations().await.map_err(|e| e.to_string())
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

            // Then add it back with the same name
            self.add(&context.name, path_str).await
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

        // Then add it back with the same name
        self.add(&context_name, path_str).await
    }

    /// Update context by name
    pub async fn update_context_by_name(&mut self, name: &str, path_str: &str) -> Result<String, String> {
        if let Some(context) = self.client.get_context_by_name(name).await {
            // Remove the existing context first
            self.client
                .remove_context_by_id(&context.id)
                .await
                .map_err(|e| e.to_string())?;

            // Then add it back with the same name
            self.add(name, path_str).await
        } else {
            Err(format!("Context with name '{}' not found", name))
        }
    }
}
