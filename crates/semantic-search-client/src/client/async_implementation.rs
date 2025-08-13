use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::{
    Mutex,
    RwLock,
    Semaphore,
    mpsc,
};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use crate::client::semantic_context::SemanticContext;
use crate::client::{
    embedder_factory,
    utils,
};
use crate::config;
use crate::config::SemanticSearchConfig;
use crate::embedding::{
    EmbeddingType,
    TextEmbedderTrait,
};
use crate::error::{
    Result,
    SemanticSearchError,
};
use crate::types::{
    AddContextRequest,
    ContextId,
    DataPoint,
    IndexingJob,
    IndexingParams,
    KnowledgeContext,
    OperationHandle,
    OperationStatus,
    OperationType,
    ProgressInfo,
    ProgressStatus,
    SearchResults,
    SystemStatus,
};

/// Async Semantic Search Client with proper cancellation support
///
/// This is a fully async version of the semantic search client that provides:
/// - Deterministic cancellation of indexing operations
/// - Concurrent read access during indexing
/// - Background job processing with proper resource management
/// - Non-blocking operations for better user experience
pub struct AsyncSemanticSearchClient {
    /// Base directory for storing persistent contexts
    base_dir: PathBuf,
    /// Contexts that can be read concurrently during indexing
    contexts: Arc<RwLock<HashMap<ContextId, KnowledgeContext>>>,
    /// Volatile contexts for active semantic data
    volatile_contexts: Arc<RwLock<HashMap<ContextId, Arc<Mutex<SemanticContext>>>>>,
    /// Text embedder for generating embeddings
    embedder: Box<dyn TextEmbedderTrait>,
    /// Configuration for the client
    config: SemanticSearchConfig,
    /// Background job processor
    job_tx: mpsc::UnboundedSender<IndexingJob>,
    /// Active operations tracking
    pub active_operations: Arc<RwLock<HashMap<Uuid, OperationHandle>>>,
}

/// Background worker for processing indexing jobs
struct BackgroundWorker {
    job_rx: mpsc::UnboundedReceiver<IndexingJob>,
    contexts: Arc<RwLock<HashMap<ContextId, KnowledgeContext>>>,
    volatile_contexts: Arc<RwLock<HashMap<ContextId, Arc<Mutex<SemanticContext>>>>>,
    active_operations: Arc<RwLock<HashMap<Uuid, OperationHandle>>>,
    embedder: Box<dyn TextEmbedderTrait>,
    config: SemanticSearchConfig,
    base_dir: PathBuf,
    indexing_semaphore: Arc<Semaphore>,
}

const MAX_CONCURRENT_OPERATIONS: usize = 3;

impl AsyncSemanticSearchClient {
    /// Create a new async semantic search client with custom config
    pub async fn with_config(base_dir: impl AsRef<Path>, config: SemanticSearchConfig) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        tokio::fs::create_dir_all(&base_dir).await?;

        // Create models directory
        crate::config::ensure_models_dir(&base_dir)?;

        // Pre-download models if the embedding type requires them
        Self::ensure_models_downloaded(&config.embedding_type).await?;

        let embedder = embedder_factory::create_embedder(config.embedding_type)?;

        // Load metadata for persistent contexts
        let contexts_file = base_dir.join("contexts.json");
        let persistent_contexts: HashMap<ContextId, KnowledgeContext> = utils::load_json_from_file(&contexts_file)?;

        let contexts = Arc::new(RwLock::new(persistent_contexts));
        let volatile_contexts = Arc::new(RwLock::new(HashMap::new()));
        let active_operations = Arc::new(RwLock::new(HashMap::new()));
        let (job_tx, job_rx) = mpsc::unbounded_channel();

        // Start background worker
        let worker_embedder = embedder_factory::create_embedder(config.embedding_type)?;
        let worker = BackgroundWorker {
            job_rx,
            contexts: contexts.clone(),
            volatile_contexts: volatile_contexts.clone(),
            active_operations: active_operations.clone(),
            embedder: worker_embedder,
            config: config.clone(),
            base_dir: base_dir.clone(),
            indexing_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS)),
        };

        tokio::spawn(worker.run());

        let mut client = Self {
            base_dir,
            contexts,
            volatile_contexts,
            embedder,
            config,
            job_tx,
            active_operations,
        };

        // Load all persistent contexts
        client.load_persistent_contexts().await?;

        Ok(client)
    }

    /// Create a new async semantic search client with default config
    pub async fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir_path = base_dir.as_ref().to_path_buf();
        let config = SemanticSearchConfig {
            base_dir: base_dir_path,
            ..Default::default()
        };
        Self::with_config(base_dir, config).await
    }

    /// Create a new semantic search client with the default base directory
    ///
    /// # Returns
    ///
    /// A new SemanticSearchClient instance
    pub async fn new_with_default_dir() -> Result<Self> {
        let base_dir = Self::get_default_base_dir();
        Self::new(base_dir).await
    }

    /// Ensure models are downloaded for the given embedding type
    async fn ensure_models_downloaded(embedding_type: &EmbeddingType) -> Result<()> {
        match embedding_type {
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            EmbeddingType::Candle => {
                use crate::client::hosted_model_client::HostedModelClient;
                use crate::embedding::ModelType;

                let model_config = ModelType::default().get_config();
                let (model_path, _tokenizer_path) = model_config.get_local_paths();

                // Create model directory if it doesn't exist
                if let Some(parent) = model_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(SemanticSearchError::IoError)?;
                }

                debug!("Reviewing model files for {}...", model_config.name);

                // Get the target directory (parent of model_path, which should be the model directory)
                let target_dir = model_path
                    .parent()
                    .ok_or_else(|| SemanticSearchError::EmbeddingError("Invalid model path".to_string()))?;

                // Get the hosted models base URL from config
                let semantic_config = crate::config::get_config();
                let base_url = &semantic_config.hosted_models_base_url;

                // Create hosted model client and download with progress bar
                let client = HostedModelClient::new(base_url.clone());
                client
                    .ensure_model(&model_config, target_dir)
                    .await
                    .map_err(|e| SemanticSearchError::EmbeddingError(format!("Failed to download model: {}", e)))?;

                debug!("Model download completed for {}", model_config.name);
            },
            EmbeddingType::BM25 => {
                // BM25 doesn't require model downloads
            },
            #[cfg(test)]
            EmbeddingType::Mock => {
                // Mock doesn't require model downloads
            },
        }
        Ok(())
    }

    /// Get the default base directory for memory bank
    ///
    /// # Returns
    ///
    /// The default base directory path
    pub fn get_default_base_dir() -> PathBuf {
        config::get_default_base_dir()
    }

    /// Add context using structured request
    pub async fn add_context(&self, request: AddContextRequest) -> Result<(Uuid, CancellationToken)> {
        let canonical_path = request.path.canonicalize().map_err(|_e| {
            SemanticSearchError::InvalidPath(format!(
                "Path does not exist or is not accessible: {}",
                request.path.display()
            ))
        })?;

        // Check for conflicts
        self.check_path_exists(&canonical_path).await?;

        // Validate patterns early to fail fast
        if let Some(ref include_patterns) = request.include_patterns {
            crate::pattern_filter::PatternFilter::new(include_patterns, &[])
                .map_err(|e| SemanticSearchError::InvalidArgument(format!("Invalid include pattern: {}", e)))?;
        }
        if let Some(ref exclude_patterns) = request.exclude_patterns {
            crate::pattern_filter::PatternFilter::new(&[], exclude_patterns)
                .map_err(|e| SemanticSearchError::InvalidArgument(format!("Invalid exclude pattern: {}", e)))?;
        }

        let operation_id = Uuid::new_v4();
        let cancel_token = CancellationToken::new();

        // Register operation for tracking
        self.register_operation(
            operation_id,
            OperationType::Indexing {
                name: request.name.clone(),
                path: canonical_path.to_string_lossy().to_string(),
            },
            cancel_token.clone(),
        )
        .await;

        // Submit job to background worker
        let job = IndexingJob::AddDirectory {
            id: operation_id,
            cancel: cancel_token.clone(),
            path: canonical_path,
            name: request.name,
            description: request.description,
            persistent: request.persistent,
            include_patterns: request.include_patterns,
            exclude_patterns: request.exclude_patterns,
        };

        self.job_tx
            .send(job)
            .map_err(|_send_error| SemanticSearchError::OperationFailed("Background worker unavailable".to_string()))?;

        Ok((operation_id, cancel_token))
    }

    /// Get all contexts (concurrent with indexing)
    pub async fn get_contexts(&self) -> Vec<KnowledgeContext> {
        // Try to get a read lock with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(2), self.contexts.read()).await {
            Ok(contexts_guard) => contexts_guard.values().cloned().collect(),
            Err(_) => {
                // If we can't get the lock quickly, try non-blocking
                if let Ok(contexts_guard) = self.contexts.try_read() {
                    contexts_guard.values().cloned().collect()
                } else {
                    // Heavy indexing in progress, return empty for now
                    tracing::warn!("Could not access contexts - heavy indexing in progress");
                    Vec::new()
                }
            },
        }
    }

    /// Search across all contexts (concurrent with indexing)
    pub async fn search_all(
        &self,
        query_text: &str,
        result_limit: Option<usize>,
    ) -> Result<Vec<(ContextId, SearchResults)>> {
        if query_text.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Query text cannot be empty".to_string(),
            ));
        }

        let effective_limit = result_limit.unwrap_or(self.config.default_results);
        let query_vector = self.embedder.embed(query_text)?;

        // Try to get volatile contexts with timeout
        let volatile_contexts =
            match tokio::time::timeout(std::time::Duration::from_millis(100), self.volatile_contexts.read()).await {
                Ok(contexts_guard) => contexts_guard,
                Err(_) => {
                    if let Ok(contexts_guard) = self.volatile_contexts.try_read() {
                        contexts_guard
                    } else {
                        // Can't search during heavy indexing
                        return Ok(Vec::new());
                    }
                },
            };

        let mut all_results = Vec::new();

        for (context_id, context) in volatile_contexts.iter() {
            if let Ok(context_guard) = context.try_lock() {
                match context_guard.search(&query_vector, effective_limit) {
                    Ok(results) => {
                        if !results.is_empty() {
                            all_results.push((context_id.clone(), results));
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to search context {}: {}", context_id, e);
                    },
                }
            }
        }

        // Sort by best match
        all_results.sort_by(|(_, a), (_, b)| {
            if a.is_empty() || b.is_empty() {
                return std::cmp::Ordering::Equal;
            }
            a[0].distance
                .partial_cmp(&b[0].distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(all_results)
    }

    /// Cancel an operation by ID
    pub async fn cancel_operation(&self, operation_id: Uuid) -> Result<String> {
        let mut operations = self.active_operations.write().await;

        if let Some(handle) = operations.get_mut(&operation_id) {
            // Cancel the token
            handle.cancel_token.cancel();

            // Abort the actual task if it exists
            if let Some(task_handle) = &handle.task_handle {
                task_handle.abort();
            }

            let op_type = handle.operation_type.display_name();
            let id_display = &operation_id.to_string()[..8];

            // Update progress to show cancellation
            if let Ok(mut progress) = handle.progress.try_lock() {
                progress.message = "Operation cancelled by user".to_string();
            }

            Ok(format!("✅ Cancelled operation: {} (ID: {})", op_type, id_display))
        } else {
            Err(SemanticSearchError::OperationFailed(format!(
                "Operation not found: {}",
                &operation_id.to_string()[..8]
            )))
        }
    }

    /// Cancel the most recent operation
    pub async fn cancel_most_recent_operation(&self) -> Result<String> {
        let operations = self.active_operations.read().await;

        if operations.is_empty() {
            return Err(SemanticSearchError::OperationFailed(
                "No active operations to cancel".to_string(),
            ));
        }

        // Find the most recent operation (highest started_at time)
        let most_recent = operations
            .iter()
            .max_by_key(|(_, handle)| handle.started_at)
            .map(|(id, _)| *id);

        drop(operations); // Release the read lock

        if let Some(operation_id) = most_recent {
            self.cancel_operation(operation_id).await
        } else {
            Err(SemanticSearchError::OperationFailed(
                "No active operations found".to_string(),
            ))
        }
    }

    /// Cancel all active operations
    pub async fn cancel_all_operations(&self) -> Result<String> {
        let mut operations = self.active_operations.write().await;
        let count = operations.len();

        if count == 0 {
            return Ok("No active operations to cancel".to_string());
        }

        // Cancel all operations
        for handle in operations.values_mut() {
            // Cancel the token
            handle.cancel_token.cancel();

            // Abort the actual task if it exists
            if let Some(task_handle) = &handle.task_handle {
                task_handle.abort();
            }

            // Update progress to show cancelled
            if let Ok(mut progress) = handle.progress.try_lock() {
                progress.message = "Operation cancelled by user".to_string();
                progress.current = 0;
                progress.total = 0;
            }
        }

        Ok(format!("✅ Cancelled {} active operations", count))
    }

    /// Find operation by short ID (first 8 characters)
    pub async fn find_operation_by_short_id(&self, short_id: &str) -> Option<Uuid> {
        let operations = self.active_operations.read().await;
        operations
            .iter()
            .find(|(id, _)| id.to_string().starts_with(short_id))
            .map(|(id, _)| *id)
    }

    /// List all operation IDs for debugging
    pub async fn list_operation_ids(&self) -> Vec<String> {
        let operations = self.active_operations.read().await;
        operations
            .iter()
            .map(|(id, _)| format!("{} (short: {})", id, &id.to_string()[..8]))
            .collect()
    }

    /// Get status of all active operations (returns structured data)
    pub async fn get_status_data(&self) -> Result<SystemStatus> {
        let mut operations = self.active_operations.write().await;
        let contexts = self.contexts.read().await;

        // Clean up old cancelled operations
        let now = SystemTime::now();
        let cleanup_threshold = std::time::Duration::from_secs(30);

        operations.retain(|_, handle| {
            if let Ok(progress) = handle.progress.try_lock() {
                let is_cancelled = progress.message.to_lowercase().contains("cancelled");
                let is_failed = progress.message.to_lowercase().contains("failed");
                if is_cancelled || is_failed {
                    now.duration_since(handle.started_at).unwrap_or_default() < cleanup_threshold
                } else {
                    true
                }
            } else {
                true
            }
        });

        // Collect context information
        let total_contexts = contexts.len();
        let persistent_contexts = contexts.values().filter(|c| c.persistent).count();
        let volatile_contexts = total_contexts - persistent_contexts;

        // Collect operation information
        let mut operation_statuses = Vec::new();
        let mut active_count = 0;
        let mut waiting_count = 0;

        for (id, handle) in operations.iter() {
            if let Ok(progress) = handle.progress.try_lock() {
                let is_failed = progress.message.to_lowercase().contains("failed");
                let is_cancelled = progress.message.to_lowercase().contains("cancelled");
                let is_waiting = Self::is_operation_waiting(&progress);

                // Count operations
                if is_cancelled {
                    // Don't count cancelled operations
                } else if is_failed || is_waiting {
                    waiting_count += 1;
                } else {
                    active_count += 1;
                }

                let operation_status = OperationStatus {
                    id: id.to_string(),
                    short_id: id.to_string()[..8].to_string(),
                    operation_type: handle.operation_type.clone(),
                    started_at: handle.started_at,
                    current: progress.current,
                    total: progress.total,
                    message: progress.message.clone(),
                    is_cancelled,
                    is_failed,
                    is_waiting,
                    eta: progress.calculate_eta(),
                };

                operation_statuses.push(operation_status);
            }
        }

        Ok(SystemStatus {
            total_contexts,
            persistent_contexts,
            volatile_contexts,
            operations: operation_statuses,
            active_count,
            waiting_count,
            max_concurrent: MAX_CONCURRENT_OPERATIONS,
        })
    }

    /// Clear all contexts (async, cancellable)
    pub async fn clear_all(&self) -> Result<(Uuid, CancellationToken)> {
        let operation_id = Uuid::new_v4();
        let cancel_token = CancellationToken::new();

        // Register operation for tracking
        self.register_operation(operation_id, OperationType::Clearing, cancel_token.clone())
            .await;

        // Submit job to background worker
        let job = IndexingJob::Clear {
            id: operation_id,
            cancel: cancel_token.clone(),
        };

        self.job_tx
            .send(job)
            .map_err(|_send_error| SemanticSearchError::OperationFailed("Background worker unavailable".to_string()))?;

        Ok((operation_id, cancel_token))
    }

    /// Clear all contexts immediately (synchronous operation)
    pub async fn clear_all_immediate(&self) -> Result<usize> {
        let context_count = {
            let contexts = self.contexts.read().await;
            contexts.len()
        };

        // Clear all contexts
        {
            let mut contexts = self.contexts.write().await;
            contexts.clear();
        }

        // Clear volatile contexts
        {
            let mut volatile_contexts = self.volatile_contexts.write().await;
            volatile_contexts.clear();
        }

        // Clear persistent files from disk
        if self.base_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&self.base_dir) {
                return Err(SemanticSearchError::IoError(e));
            }
            // Recreate the base directory
            if let Err(e) = std::fs::create_dir_all(&self.base_dir) {
                return Err(SemanticSearchError::IoError(e));
            }
        }

        Ok(context_count)
    }

    async fn check_path_exists(&self, canonical_path: &Path) -> Result<()> {
        // Check if there's already an ACTIVE indexing operation for this exact path
        // (ignore cancelled, failed, or completed operations)
        if let Ok(operations) = self.active_operations.try_read() {
            for handle in operations.values() {
                if let OperationType::Indexing { path, name } = &handle.operation_type {
                    if let Ok(operation_canonical) = PathBuf::from(path).canonicalize() {
                        if operation_canonical == canonical_path {
                            if let Ok(progress) = handle.progress.try_lock() {
                                // Only block if the operation is truly active (not cancelled, failed, or completed)
                                let is_cancelled = progress.message.contains("cancelled");
                                let is_failed =
                                    progress.message.contains("failed") || progress.message.contains("error");
                                let is_completed = progress.message.contains("complete");

                                if !is_cancelled && !is_failed && !is_completed {
                                    return Err(SemanticSearchError::InvalidArgument(format!(
                                        "Already indexing this path: {} (Operation: {})",
                                        path, name
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check if this canonical path already exists in the knowledge base
        if let Ok(contexts_guard) = self.contexts.try_read() {
            for context in contexts_guard.values() {
                if let Some(existing_path) = &context.source_path {
                    let existing_path_buf = PathBuf::from(existing_path);
                    if let Ok(existing_canonical) = existing_path_buf.canonicalize() {
                        if existing_canonical == *canonical_path {
                            return Err(SemanticSearchError::InvalidArgument(format!(
                                "Path already exists in knowledge base: {} (Context: '{}')",
                                existing_path, context.name
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn register_operation(
        &self,
        operation_id: Uuid,
        operation_type: OperationType,
        cancel_token: CancellationToken,
    ) {
        let handle = OperationHandle {
            operation_type,
            started_at: SystemTime::now(),
            progress: Arc::new(Mutex::new(ProgressInfo::new())),
            cancel_token,
            task_handle: None,
        };

        let mut operations = self.active_operations.write().await;
        operations.insert(operation_id, handle);
    }

    async fn load_persistent_contexts(&mut self) -> Result<()> {
        let context_ids: Vec<String> = {
            let contexts = self.contexts.read().await;
            contexts.keys().cloned().collect()
        };

        for id in context_ids {
            if let Err(e) = self.load_persistent_context(&id).await {
                tracing::error!("Failed to load persistent context {}: {}", id, e);
            }
        }

        Ok(())
    }

    async fn load_persistent_context(&self, context_id: &str) -> Result<()> {
        // Check if already loaded
        {
            let volatile_contexts = self.volatile_contexts.read().await;
            if volatile_contexts.contains_key(context_id) {
                return Ok(());
            }
        }

        // Create the context directory path
        let context_dir = self.base_dir.join(context_id);
        if !context_dir.exists() {
            return Err(SemanticSearchError::InvalidPath(format!(
                "Context directory does not exist: {}",
                context_dir.display()
            )));
        }

        // Create a new semantic context
        let data_file = context_dir.join("data.json");
        let semantic_context = SemanticContext::new(data_file)?;

        // Store the semantic context
        let mut volatile_contexts = self.volatile_contexts.write().await;
        volatile_contexts.insert(context_id.to_string(), Arc::new(Mutex::new(semantic_context)));

        Ok(())
    }

    fn is_operation_waiting(progress: &ProgressInfo) -> bool {
        // Only consider it waiting if it explicitly says so or has no progress data
        progress.message.contains("Waiting")
            || progress.message.contains("queue")
            || progress.message.contains("slot")
            || progress.message.contains("write access")
            || progress.message.contains("Initializing")
            || progress.message.contains("Starting")
            || (progress.current == 0 && progress.total == 0 && !progress.message.contains("complete"))
    }

    /// Remove context by ID
    pub async fn remove_context_by_id(&self, context_id: &str) -> Result<()> {
        // Remove from contexts map
        {
            let mut contexts = self.contexts.write().await;
            contexts.remove(context_id);
        }

        // Remove from volatile contexts
        {
            let mut volatile_contexts = self.volatile_contexts.write().await;
            volatile_contexts.remove(context_id);
        }

        // Remove persistent storage if it exists
        let context_dir = self.base_dir.join(context_id);
        if context_dir.exists() {
            tokio::fs::remove_dir_all(&context_dir).await.map_err(|e| {
                SemanticSearchError::OperationFailed(format!("Failed to remove context directory: {}", e))
            })?;
        }

        // Save updated contexts metadata
        self.save_contexts_metadata_sync()
            .await
            .map_err(SemanticSearchError::OperationFailed)?;

        Ok(())
    }

    /// Get context by path
    pub async fn get_context_by_path(&self, path: &str) -> Option<KnowledgeContext> {
        let contexts = self.contexts.read().await;

        // Try to canonicalize the input path
        let canonical_input = PathBuf::from(path).canonicalize().ok();

        contexts
            .values()
            .find(|c| {
                if let Some(source_path) = &c.source_path {
                    // First try exact match
                    if source_path == path {
                        return true;
                    }

                    // Then try canonical path comparison if available
                    if let Some(ref canonical_input) = canonical_input {
                        if let Ok(canonical_source) = PathBuf::from(source_path).canonicalize() {
                            return canonical_input == &canonical_source;
                        }
                    }

                    // Finally try string comparison after normalizing separators
                    let normalized_source = source_path.replace('\\', "/");
                    let normalized_input = path.replace('\\', "/");
                    normalized_source == normalized_input
                } else {
                    false
                }
            })
            .cloned()
    }

    /// Get context by name
    pub async fn get_context_by_name(&self, name: &str) -> Option<KnowledgeContext> {
        let contexts = self.contexts.read().await;
        contexts.values().find(|c| c.name == name).cloned()
    }

    /// List all context paths for debugging
    pub async fn list_context_paths(&self) -> Vec<String> {
        let contexts = self.contexts.read().await;
        contexts
            .values()
            .map(|c| format!("{} -> {}", c.name, c.source_path.as_deref().unwrap_or("None")))
            .collect()
    }

    /// Save contexts metadata (sync version for client)
    async fn save_contexts_metadata_sync(&self) -> std::result::Result<(), String> {
        let contexts = self.contexts.read().await;
        let contexts_file = self.base_dir.join("contexts.json");

        // Convert to persistent contexts only
        let persistent_contexts: HashMap<String, KnowledgeContext> = contexts
            .iter()
            .filter(|(_, ctx)| ctx.persistent)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        utils::save_json_to_file(&contexts_file, &persistent_contexts)
            .map_err(|e| format!("Failed to save contexts metadata: {}", e))
    }
}

// Background Worker Implementation
impl BackgroundWorker {
    /// Create pattern filter from include/exclude patterns
    fn create_pattern_filter(
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
    ) -> std::result::Result<Option<crate::pattern_filter::PatternFilter>, String> {
        if include_patterns.is_some() || exclude_patterns.is_some() {
            let inc = include_patterns.as_deref().unwrap_or(&[]);
            let exc = exclude_patterns.as_deref().unwrap_or(&[]);
            Ok(Some(
                crate::pattern_filter::PatternFilter::new(inc, exc).map_err(|e| format!("Invalid patterns: {}", e))?,
            ))
        } else {
            Ok(None)
        }
    }

    async fn run(mut self) {
        debug!("Background worker started for async semantic search client");

        while let Some(job) = self.job_rx.recv().await {
            match job {
                IndexingJob::AddDirectory {
                    id,
                    cancel,
                    path,
                    name,
                    description,
                    persistent,
                    include_patterns,
                    exclude_patterns,
                } => {
                    let params = IndexingParams {
                        path,
                        name,
                        description,
                        persistent,
                        include_patterns,
                        exclude_patterns,
                    };
                    self.process_add_directory(id, params, cancel).await;
                },
                IndexingJob::Clear { id, cancel } => {
                    self.process_clear(id, cancel).await;
                },
            }
        }

        debug!("Background worker stopped");
    }

    async fn process_add_directory(&self, operation_id: Uuid, params: IndexingParams, cancel_token: CancellationToken) {
        debug!(
            "Processing AddDirectory job: {} -> {}",
            params.name,
            params.path.display()
        );

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        // Update status and acquire semaphore
        self.update_operation_status(operation_id, "Waiting in queue...".to_string())
            .await;

        let _permit = match self.indexing_semaphore.try_acquire() {
            Ok(permit) => {
                self.update_operation_status(operation_id, "Acquired slot, starting indexing...".to_string())
                    .await;
                permit
            },
            Err(_) => {
                self.update_operation_status(
                    operation_id,
                    format!(
                        "Waiting for available slot (max {} concurrent)...",
                        MAX_CONCURRENT_OPERATIONS
                    ),
                )
                .await;
                match self.indexing_semaphore.acquire().await {
                    Ok(permit) => {
                        self.update_operation_status(operation_id, "Acquired slot, starting indexing...".to_string())
                            .await;
                        permit
                    },
                    Err(_) => {
                        self.mark_operation_failed(operation_id, "Semaphore unavailable".to_string())
                            .await;
                        return;
                    },
                }
            },
        };

        // Perform actual indexing
        let indexing_params = IndexingParams {
            path: params.path,
            name: params.name,
            description: params.description,
            persistent: params.persistent,
            include_patterns: params.include_patterns,
            exclude_patterns: params.exclude_patterns,
        };
        let result = self.perform_indexing(operation_id, indexing_params, cancel_token).await;

        match result {
            Ok(context_id) => {
                debug!("Successfully indexed context: {}", context_id);
                self.mark_operation_completed(operation_id).await;
            },
            Err(e) => {
                tracing::error!("Indexing failed: {}", e);
                self.mark_operation_failed(operation_id, e).await;
            },
        }
    }

    async fn perform_indexing(
        &self,
        operation_id: Uuid,
        params: IndexingParams,
        cancel_token: CancellationToken,
    ) -> std::result::Result<String, String> {
        if !params.path.exists() {
            return Err(format!("Path '{}' does not exist", params.path.display()));
        }

        // Check for cancellation before starting
        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled".to_string());
        }

        // Generate a unique ID for this context
        let context_id = utils::generate_context_id();

        // Create progress callback (not used in this simplified version)
        let _progress_callback = self.create_progress_callback(operation_id, cancel_token.clone());

        // Perform indexing in a cancellable task
        let embedder = &self.embedder;
        let config = &self.config;
        let base_dir = self.base_dir.clone();
        let cancel_token_clone = cancel_token.clone();

        // Create context directory
        let context_dir = if params.persistent {
            base_dir.join(&context_id)
        } else {
            std::env::temp_dir().join("semantic_search").join(&context_id)
        };

        tokio::fs::create_dir_all(&context_dir)
            .await
            .map_err(|e| format!("Failed to create context directory: {}", e))?;

        // Check cancellation after directory creation
        if cancel_token_clone.is_cancelled() {
            return Err("Operation was cancelled during setup".to_string());
        }

        // Count files and notify progress
        let file_count = self
            .count_files_in_directory(
                &params.path,
                operation_id,
                &params.include_patterns,
                &params.exclude_patterns,
            )
            .await?;

        // Check if file count exceeds the configured limit
        if file_count > config.max_files {
            self.update_operation_status(
                operation_id,
                format!(
                    "Failed: Directory contains {} files, which exceeds the maximum limit of {} files",
                    file_count, config.max_files
                ),
            )
            .await;
            cancel_token.cancel();
            return Err(format!(
                "Failed: Directory contains {} files, which exceeds the maximum limit of {} files",
                file_count, config.max_files
            ));
        }

        // Check cancellation before processing files
        if cancel_token_clone.is_cancelled() {
            self.update_operation_status(
                operation_id,
                "Failed: Operation was cancelled before file processing".to_string(),
            )
            .await;
            cancel_token.cancel();
            return Err("Failed: Operation was cancelled before file processing".to_string());
        }

        // Process files with cancellation checks
        let items = self
            .process_directory_files(
                &params.path,
                file_count,
                operation_id,
                &cancel_token_clone,
                &params.include_patterns,
                &params.exclude_patterns,
            )
            .await?;

        // Check cancellation before creating semantic context
        if cancel_token_clone.is_cancelled() {
            self.update_operation_status(
                operation_id,
                "Failed: Operation was cancelled before file processing".to_string(),
            )
            .await;
            cancel_token.cancel();
            return Err("Failed: Operation was cancelled before semantic context creation".to_string());
        }

        // Create semantic context
        let semantic_context = self
            .create_semantic_context_impl(&context_dir, &items, &**embedder, operation_id, &cancel_token_clone)
            .await?;

        // Final cancellation check
        if cancel_token_clone.is_cancelled() {
            self.update_operation_status(
                operation_id,
                "Failed: Operation was cancelled before saving".to_string(),
            )
            .await;
            cancel_token.cancel();
            return Err("Cancelled: Operation was cancelled before saving".to_string());
        }

        // Save context if persistent
        if params.persistent {
            semantic_context
                .save()
                .map_err(|e| format!("Failed to save context: {}", e))?;
        }

        // Store the context
        self.store_context(
            &context_id,
            &params.name,
            &params.description,
            params.persistent,
            Some(params.path.to_string_lossy().to_string()),
            &params.include_patterns,
            &params.exclude_patterns,
            semantic_context,
            file_count,
        )
        .await?;

        Ok(context_id)
    }

    async fn process_clear(&self, operation_id: Uuid, cancel_token: CancellationToken) {
        debug!("Processing Clear job");

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        self.update_operation_status(operation_id, "Starting clear operation...".to_string())
            .await;

        // Get all contexts to clear
        let contexts = {
            let contexts_guard = self.contexts.read().await;
            contexts_guard.values().cloned().collect::<Vec<_>>()
        };

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        self.update_operation_status(operation_id, format!("Clearing {} contexts...", contexts.len()))
            .await;

        let mut removed = 0;

        for (index, context) in contexts.iter().enumerate() {
            // Check for cancellation before each context removal
            if cancel_token.is_cancelled() {
                self.update_operation_status(
                    operation_id,
                    format!(
                        "Operation was cancelled after clearing {} of {} contexts",
                        removed,
                        contexts.len()
                    ),
                )
                .await;
                self.mark_operation_cancelled(operation_id).await;
                return;
            }

            // Update progress
            self.update_operation_progress(
                operation_id,
                (index + 1) as u64,
                contexts.len() as u64,
                format!(
                    "Clearing context {} of {} ({})...",
                    index + 1,
                    contexts.len(),
                    context.name
                ),
            )
            .await;

            // Remove from contexts map
            {
                let mut contexts_guard = self.contexts.write().await;
                contexts_guard.remove(&context.id);
            }

            // Remove from volatile contexts
            {
                let mut volatile_contexts = self.volatile_contexts.write().await;
                volatile_contexts.remove(&context.id);
            }

            // Delete persistent storage if needed
            if context.persistent {
                let context_dir = self.base_dir.join(&context.id);
                if context_dir.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&context_dir).await {
                        tracing::warn!("Failed to remove context directory {}: {}", context_dir.display(), e);
                    }
                }
            }

            removed += 1;

            // Small delay to allow cancellation checks and prevent overwhelming the system
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Save updated contexts metadata (should be empty now)
        if let Err(e) = self.save_contexts_metadata().await {
            tracing::error!("Failed to save contexts metadata after clear: {}", e);
        }

        // Final check for cancellation
        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
        } else {
            self.update_operation_status(operation_id, format!("Successfully cleared {} contexts", removed))
                .await;
            self.mark_operation_completed(operation_id).await;
        }
    }

    fn create_progress_callback(
        &self,
        operation_id: Uuid,
        cancel_token: CancellationToken,
    ) -> impl Fn(ProgressStatus) + Send + 'static {
        let operations = self.active_operations.clone();

        move |status: ProgressStatus| {
            // Don't update progress if operation is cancelled
            if cancel_token.is_cancelled() {
                return;
            }

            let operations_clone = operations.clone();
            let op_id = operation_id;

            // Use try_lock to avoid blocking - if we can't get the lock, skip this update
            if let Ok(mut ops) = operations_clone.try_write() {
                if let Some(op) = ops.get_mut(&op_id) {
                    // Double-check cancellation before updating progress
                    if op.cancel_token.is_cancelled() {
                        return;
                    }

                    if let Ok(mut progress) = op.progress.try_lock() {
                        match status {
                            ProgressStatus::CountingFiles => {
                                progress.update(0, 0, "Counting files...".to_string());
                            },
                            ProgressStatus::StartingIndexing(total) => {
                                progress.update(0, total as u64, format!("Starting indexing ({} files)", total));
                            },
                            ProgressStatus::Indexing(current, total) => {
                                progress.update(
                                    current as u64,
                                    total as u64,
                                    format!("Indexing files ({}/{})", current, total),
                                );
                            },
                            ProgressStatus::DownloadingModel(current, total) => {
                                progress.update(
                                    current,
                                    total,
                                    format!("Downloading model ({} / {} bytes)", current, total),
                                );
                            },
                            ProgressStatus::CreatingSemanticContext => {
                                progress.update(0, 0, "Creating semantic context...".to_string());
                            },
                            ProgressStatus::GeneratingEmbeddings(current, total) => {
                                progress.update(
                                    current as u64,
                                    total as u64,
                                    format!("Generating embeddings ({}/{})", current, total),
                                );
                            },
                            ProgressStatus::BuildingIndex => {
                                progress.update(0, 0, "Building vector index...".to_string());
                            },
                            ProgressStatus::Finalizing => {
                                progress.update(0, 0, "Finalizing index...".to_string());
                            },
                            ProgressStatus::Complete => {
                                let total = progress.total;
                                progress.update(total, total, "Indexing complete!".to_string());
                            },
                        };
                    }
                }
            };
        }
    }

    async fn update_operation_status(&self, operation_id: Uuid, message: String) {
        if let Ok(mut operations) = self.active_operations.try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = message;
                }
            }
        }
    }

    async fn update_operation_progress(&self, operation_id: Uuid, current: u64, total: u64, message: String) {
        if let Ok(mut operations) = self.active_operations.try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.update(current, total, message);
                }
            }
        }
    }

    async fn mark_operation_completed(&self, operation_id: Uuid) {
        if let Ok(mut operations) = self.active_operations.try_write() {
            operations.remove(&operation_id);
        }
        debug!("Operation {} completed", operation_id);
    }

    async fn mark_operation_failed(&self, operation_id: Uuid, error: String) {
        if let Ok(mut operations) = self.active_operations.try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = error.clone();
                }
            }
            // Don't remove failed operations - let them be cleaned up by the 30-second timer
            // so users can see what failed
        }
        tracing::error!("Operation {} failed: {}", operation_id, error);
    }

    async fn mark_operation_cancelled(&self, operation_id: Uuid) {
        if let Ok(mut operations) = self.active_operations.try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = "Operation cancelled by user".to_string();
                    progress.current = 0;
                    progress.total = 0;
                }
            }
            // Don't remove immediately - let it show as cancelled for a while
        }
        debug!("Operation {} cancelled", operation_id);
    }

    #[allow(clippy::too_many_arguments)]
    async fn store_context(
        &self,
        context_id: &str,
        name: &str,
        description: &str,
        persistent: bool,
        source_path: Option<String>,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
        semantic_context: SemanticContext,
        item_count: usize,
    ) -> std::result::Result<(), String> {
        // Create the context metadata
        let context = KnowledgeContext::new(
            context_id.to_string(),
            name,
            description,
            persistent,
            source_path,
            (
                include_patterns.as_deref().unwrap_or(&[]).to_vec(),
                exclude_patterns.as_deref().unwrap_or(&[]).to_vec(),
            ),
            item_count,
        );

        // Store in contexts map
        {
            let mut contexts = self.contexts.write().await;
            contexts.insert(context_id.to_string(), context);
        }

        // Store the semantic context in volatile contexts
        {
            let mut volatile_contexts = self.volatile_contexts.write().await;
            volatile_contexts.insert(context_id.to_string(), Arc::new(Mutex::new(semantic_context)));
        }

        // Save contexts metadata if persistent
        if persistent {
            self.save_contexts_metadata().await?;
        }

        Ok(())
    }

    // Helper methods for file processing (async instance methods)
    async fn count_files_in_directory(
        &self,
        dir_path: &Path,
        operation_id: Uuid,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
    ) -> std::result::Result<usize, String> {
        self.update_operation_status(operation_id, "Counting files...".to_string())
            .await;

        // Use tokio::task::spawn_blocking to make the synchronous walkdir operation non-blocking
        let dir_path = dir_path.to_path_buf();
        let active_operations = self.active_operations.clone();

        // Create pattern filter if patterns are provided
        let pattern_filter = Self::create_pattern_filter(include_patterns, exclude_patterns)?;

        let count_result = tokio::task::spawn_blocking(move || {
            let mut count = 0;
            let mut checked = 0;

            for _entry in walkdir::WalkDir::new(&dir_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    // Skip hidden files
                    !e.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|s| s.starts_with('.'))
                })
                .filter(|e| {
                    // Apply pattern filter if present
                    pattern_filter
                        .as_ref()
                        .is_none_or(|filter| filter.should_include(e.path()))
                })
            {
                count += 1;
                checked += 1;

                // Check for cancellation every 100 files
                if checked % 100 == 0 {
                    // Check if operation was cancelled
                    if let Ok(operations) = active_operations.try_read() {
                        if let Some(handle) = operations.get(&operation_id) {
                            if handle.cancel_token.is_cancelled() {
                                return Err("Operation cancelled during file counting".to_string());
                            }
                            if let Ok(progress) = handle.progress.try_lock() {
                                if progress.message.contains("cancelled") {
                                    return Err("Operation cancelled during file counting".to_string());
                                }
                            }
                        }
                    }
                }

                // Early exit if we already exceed the limit to save time
                if count > 5000 {
                    break;
                }
            }

            Ok(count)
        })
        .await;

        match count_result {
            Ok(Ok(count)) => Ok(count),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(format!("File counting task failed: {}", e)),
        }
    }

    async fn process_directory_files(
        &self,
        dir_path: &Path,
        file_count: usize,
        operation_id: Uuid,
        cancel_token: &CancellationToken,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
    ) -> std::result::Result<Vec<serde_json::Value>, String> {
        use crate::processing::process_file_with_config;

        self.update_operation_status(operation_id, format!("Starting indexing ({} files)", file_count))
            .await;

        // Create pattern filter if patterns are provided
        let pattern_filter = Self::create_pattern_filter(include_patterns, exclude_patterns)?;

        let mut processed_files = 0;
        let mut items = Vec::new();

        for entry in walkdir::WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                // Apply pattern filter if present
                pattern_filter
                    .as_ref()
                    .is_none_or(|filter| filter.should_include(e.path()))
            })
        {
            // Check for cancellation frequently
            if cancel_token.is_cancelled() {
                return Err("Operation was cancelled during file processing".to_string());
            }

            let path = entry.path();

            // Skip hidden files
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with('.'))
            {
                continue;
            }

            // Process the file
            match process_file_with_config(path, Some(self.config.chunk_size), Some(self.config.chunk_overlap)) {
                Ok(mut file_items) => items.append(&mut file_items),
                Err(_) => continue, // Skip files that fail to process
            }

            processed_files += 1;

            // Update progress
            if processed_files % 10 == 0 {
                self.update_operation_progress(
                    operation_id,
                    processed_files as u64,
                    file_count as u64,
                    format!("Indexing files ({}/{})", processed_files, file_count),
                )
                .await;
            }
        }

        Ok(items)
    }

    async fn create_semantic_context_impl(
        &self,
        context_dir: &Path,
        items: &[serde_json::Value],
        embedder: &dyn TextEmbedderTrait,
        operation_id: Uuid,
        cancel_token: &CancellationToken,
    ) -> std::result::Result<SemanticContext, String> {
        self.update_operation_status(operation_id, "Creating semantic context...".to_string())
            .await;

        // Check for cancellation
        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled during semantic context creation".to_string());
        }

        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))
            .map_err(|e| format!("Failed to create semantic context: {}", e))?;

        // Process items to data points with cancellation checks
        let mut data_points = Vec::new();
        let total_items = items.len();

        for (i, item) in items.iter().enumerate() {
            // Check for cancellation frequently
            if cancel_token.is_cancelled() {
                return Err("Operation was cancelled during embedding generation".to_string());
            }

            // Update progress for embedding generation
            if i % 10 == 0 {
                self.update_operation_progress(
                    operation_id,
                    i as u64,
                    total_items as u64,
                    format!("Generating embeddings ({}/{})", i, total_items),
                )
                .await;
            }

            // Create a data point from the item
            let data_point = Self::create_data_point_from_item(item, i, embedder)
                .map_err(|e| format!("Failed to create data point: {}", e))?;
            data_points.push(data_point);
        }

        // Check for cancellation before building index
        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled before building index".to_string());
        }

        self.update_operation_status(operation_id, "Building vector index...".to_string())
            .await;

        // Add the data points to the context
        semantic_context
            .add_data_points(data_points)
            .map_err(|e| format!("Failed to add data points: {}", e))?;

        Ok(semantic_context)
    }

    fn create_data_point_from_item(
        item: &serde_json::Value,
        id: usize,
        embedder: &dyn TextEmbedderTrait,
    ) -> Result<DataPoint> {
        use crate::types::DataPoint;

        // Extract the text from the item
        let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");

        // Generate an embedding for the text
        let vector = embedder.embed(text)?;

        // Convert Value to HashMap
        let payload: HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = item {
            map.clone().into_iter().collect()
        } else {
            let mut map = HashMap::new();
            map.insert("text".to_string(), item.clone());
            map
        };

        Ok(DataPoint { id, payload, vector })
    }

    async fn save_contexts_metadata(&self) -> std::result::Result<(), String> {
        let contexts = self.contexts.read().await;
        let contexts_file = self.base_dir.join("contexts.json");

        // Convert to persistent contexts only
        let persistent_contexts: HashMap<String, KnowledgeContext> = contexts
            .iter()
            .filter(|(_, ctx)| ctx.persistent)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        utils::save_json_to_file(&contexts_file, &persistent_contexts)
            .map_err(|e| format!("Failed to save contexts metadata: {}", e))
    }
}
