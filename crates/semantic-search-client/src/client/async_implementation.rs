use std::path::{
    Path,
    PathBuf,
};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::background::BackgroundWorker;
// Use the new modular structure
use super::context::ContextManager;
use super::model::ModelDownloader;
use super::operation::OperationManager;
use crate::client::embedder_factory;
use crate::config::{
    self,
    SemanticSearchConfig,
};
use crate::embedding::TextEmbedderTrait;
use crate::error::{
    Result,
    SemanticSearchError,
};
use crate::types::*;

/// Async Semantic Search Client with proper cancellation support
pub struct AsyncSemanticSearchClient {
    base_dir: PathBuf,
    embedder: Box<dyn TextEmbedderTrait>,
    config: SemanticSearchConfig,
    job_tx: mpsc::UnboundedSender<IndexingJob>,
    context_manager: ContextManager,
    operation_manager: OperationManager,
}

impl AsyncSemanticSearchClient {
    /// Creates a new AsyncSemanticSearchClient with custom configuration.
    ///
    /// This method initializes the client with a specified base directory and configuration,
    /// setting up all necessary components including context manager, operation manager,
    /// and background worker for asynchronous operations.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - The base directory where persistent contexts will be stored
    /// * `config` - Custom configuration for the semantic search client
    ///
    /// # Returns
    ///
    /// Returns a `Result<Self>` containing the initialized client or an error if initialization
    /// fails.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The base directory cannot be created or accessed
    /// - The embedder cannot be initialized
    /// - Required models cannot be downloaded
    /// - Context manager initialization fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use semantic_search_client::{
    ///     AsyncSemanticSearchClient,
    ///     SemanticSearchConfig,
    /// };
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = SemanticSearchConfig::default();
    /// let client = AsyncSemanticSearchClient::with_config("/path/to/data", config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_config(base_dir: impl AsRef<Path>, config: SemanticSearchConfig) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        tokio::fs::create_dir_all(&base_dir).await?;

        config::ensure_models_dir(&base_dir)?;
        ModelDownloader::ensure_models_downloaded(&config.embedding_type).await?;

        let embedder = embedder_factory::create_embedder(config.embedding_type)?;
        let context_manager = ContextManager::new(&base_dir).await?;
        let operation_manager = OperationManager::new();

        let (job_tx, job_rx) = mpsc::unbounded_channel();

        let worker = BackgroundWorker::new(
            job_rx,
            context_manager.clone(),
            operation_manager.clone(),
            config.clone(),
            base_dir.clone(),
        )
        .await?;

        tokio::spawn(worker.run());

        let client = Self {
            base_dir,
            embedder,
            config,
            job_tx,
            context_manager,
            operation_manager,
        };

        client.context_manager.load_persistent_contexts().await?;
        Ok(client)
    }

    /// Creates a new AsyncSemanticSearchClient with default configuration.
    ///
    /// This is a convenience method that creates a client with default settings
    /// and the specified base directory for storing persistent contexts.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - The base directory where persistent contexts will be stored
    ///
    /// # Returns
    ///
    /// Returns a `Result<Self>` containing the initialized client or an error if initialization
    /// fails.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The base directory cannot be created or accessed
    /// - Default configuration cannot be applied
    /// - Client initialization fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new("/path/to/data").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir_path = base_dir.as_ref().to_path_buf();
        let config = SemanticSearchConfig {
            base_dir: base_dir_path,
            ..Default::default()
        };
        Self::with_config(base_dir, config).await
    }

    /// Creates a new AsyncSemanticSearchClient using the default base directory.
    ///
    /// This convenience method creates a client using the system's default directory
    /// for storing semantic search data, typically in the user's home directory.
    ///
    /// # Returns
    ///
    /// Returns a `Result<Self>` containing the initialized client or an error if initialization
    /// fails.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The default directory cannot be determined or created
    /// - Client initialization fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new_with_default_dir() -> Result<Self> {
        let base_dir = Self::get_default_base_dir();
        Self::new(base_dir).await
    }

    /// Returns the default base directory for storing semantic search data.
    ///
    /// This method returns the platform-specific default directory where
    /// the semantic search client stores its persistent data and contexts.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the default base directory path.
    ///
    /// # Platform Behavior
    ///
    /// - **macOS**: `~/Library/Application Support/semantic-search`
    /// - **Linux**: `~/.local/share/semantic-search`
    /// - **Windows**: `%APPDATA%\semantic-search`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// let default_dir = AsyncSemanticSearchClient::get_default_base_dir();
    /// println!("Default directory: {}", default_dir.display());
    /// ```
    pub fn get_default_base_dir() -> PathBuf {
        config::get_default_base_dir()
    }

    /// Adds a new context to the knowledge base asynchronously.
    ///
    /// This method initiates the process of indexing a directory or file and adding it
    /// as a searchable context. The operation runs in the background and can be cancelled
    /// using the returned cancellation token.
    ///
    /// # Arguments
    ///
    /// * `request` - An `AddContextRequest` containing the path, name, description, and other
    ///   configuration options for the context to be added
    ///
    /// # Returns
    ///
    /// Returns a `Result<(Uuid, CancellationToken)>` where:
    /// - `Uuid` is the unique operation ID that can be used to track progress
    /// - `CancellationToken` can be used to cancel the operation
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The specified path does not exist or is not accessible
    /// - The path is already being indexed by another operation
    /// - The background worker cannot be started
    /// - Required models are not available and cannot be downloaded
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    ///
    /// use semantic_search_client::{
    ///     AddContextRequest,
    ///     AsyncSemanticSearchClient,
    /// };
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    ///
    /// let request = AddContextRequest {
    ///     path: PathBuf::from("/path/to/documents"),
    ///     name: "My Documents".to_string(),
    ///     description: "Personal document collection".to_string(),
    ///     persistent: true,
    ///     include_patterns: Some(vec!["*.txt".to_string(), "*.md".to_string()]),
    ///     exclude_patterns: Some(vec!["*.tmp".to_string()]),
    ///     embedding_type: None, // Use default
    /// };
    ///
    /// let (operation_id, cancel_token) = client.add_context(request).await?;
    /// println!("Started indexing operation: {}", operation_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_context(&self, request: AddContextRequest) -> Result<(Uuid, CancellationToken)> {
        let canonical_path = request.path.canonicalize().map_err(|_e| {
            SemanticSearchError::InvalidPath(format!(
                "Path does not exist or is not accessible: {}",
                request.path.display()
            ))
        })?;

        self.context_manager
            .check_path_exists(&canonical_path, &self.operation_manager)
            .await?;

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

        self.operation_manager
            .register_operation(
                operation_id,
                OperationType::Indexing {
                    name: request.name.clone(),
                    path: canonical_path.to_string_lossy().to_string(),
                },
                cancel_token.clone(),
            )
            .await;

        let job = IndexingJob::AddDirectory {
            id: operation_id,
            cancel: cancel_token.clone(),
            path: canonical_path,
            name: request.name.clone(),
            description: request.description.clone(),
            persistent: request.persistent,
            include_patterns: request.include_patterns.clone(),
            exclude_patterns: request.exclude_patterns.clone(),
            embedding_type: request.embedding_type,
        };

        self.job_tx
            .send(job)
            .map_err(|_send_error| SemanticSearchError::OperationFailed("Background worker unavailable".to_string()))?;

        Ok((operation_id, cancel_token))
    }

    /// Retrieves all available contexts in the knowledge base.
    ///
    /// This method returns a list of all contexts (both persistent and volatile)
    /// that are currently available for searching, including their metadata
    /// such as names, descriptions, and indexing statistics.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<KnowledgeContext>` containing all available contexts.
    /// The vector will be empty if no contexts have been added.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    /// let contexts = client.get_contexts().await;
    ///
    /// for context in contexts {
    ///     println!("Context: {} - {} items", context.name, context.item_count);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_contexts(&self) -> Vec<KnowledgeContext> {
        self.context_manager.get_contexts().await
    }

    /// Performs a semantic search across all available contexts.
    ///
    /// This method searches through all indexed contexts using the provided query text,
    /// returning the most relevant results ranked by semantic similarity or BM25 score
    /// depending on the context type.
    ///
    /// # Arguments
    ///
    /// * `query_text` - The search query string
    /// * `result_limit` - Optional limit on the number of results per context. If `None`, uses the
    ///   default limit from configuration
    ///
    /// # Returns
    ///
    /// Returns a `Result<Vec<(ContextId, SearchResults)>>` where each tuple contains:
    /// - `ContextId` - The unique identifier of the context
    /// - `SearchResults` - The search results from that context, ranked by relevance
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The embedder fails to generate embeddings for the query
    /// - One or more contexts cannot be searched due to corruption or access issues
    /// - The search operation times out
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    /// let results = client
    ///     .search_all("machine learning algorithms", Some(10))
    ///     .await?;
    ///
    /// for (context_id, search_results) in results {
    ///     println!(
    ///         "Results from context {}: {} matches",
    ///         context_id,
    ///         search_results.results.len()
    ///     );
    ///     for result in search_results.results.iter().take(3) {
    ///         println!(
    ///             "  Score: {:.3} - {}",
    ///             result.score,
    ///             result.text.chars().take(100).collect::<String>()
    ///         );
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
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
        self.context_manager
            .search_all(query_text, effective_limit, &*self.embedder)
            .await
    }

    /// Search in a specific context
    ///
    /// # Arguments
    ///
    /// * `context_id` - ID of the context to search in
    /// * `query_text` - Search query
    /// * `result_limit` - Maximum number of results to return (if None, uses default_results from
    ///   config)
    ///
    /// # Returns
    ///
    /// A vector of search results
    pub async fn search_context(
        &self,
        context_id: &str,
        query_text: &str,
        result_limit: Option<usize>,
    ) -> Result<SearchResults> {
        if context_id.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context ID cannot be empty".to_string(),
            ));
        }

        if query_text.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Query text cannot be empty".to_string(),
            ));
        }

        let effective_limit = result_limit.unwrap_or(self.config.default_results);

        self.context_manager
            .search_context(context_id, query_text, effective_limit, &*self.embedder)
            .await?
            .ok_or_else(|| SemanticSearchError::ContextNotFound(context_id.to_string()))
    }

    /// Cancels a running background operation.
    ///
    /// This method attempts to cancel an operation identified by its UUID.
    /// The operation will be gracefully stopped and any partial work will be cleaned up.
    ///
    /// # Arguments
    ///
    /// * `operation_id` - The unique identifier of the operation to cancel
    ///
    /// # Returns
    ///
    /// Returns a `Result<String>` containing a status message about the cancellation.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use semantic_search_client::AsyncSemanticSearchClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    /// // ... start an operation and get operation_id
    /// let status = client.cancel_operation(operation_id).await?;
    /// println!("Cancellation status: {}", status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cancel_operation(&self, operation_id: Uuid) -> Result<String> {
        self.operation_manager.cancel_operation(operation_id).await
    }

    /// Cancel the most recent operation
    pub async fn cancel_most_recent_operation(&self) -> Result<String> {
        self.operation_manager.cancel_most_recent_operation().await
    }

    /// Cancel all active operations
    pub async fn cancel_all_operations(&self) -> Result<String> {
        self.operation_manager.cancel_all_operations().await
    }

    /// Finds an operation by its short identifier.
    ///
    /// This method allows finding operations using a shortened version of their UUID,
    /// which is useful for user interfaces where full UUIDs are impractical.
    ///
    /// # Arguments
    ///
    /// * `short_id` - The first 8 characters of the operation UUID
    ///
    /// # Returns
    ///
    /// Returns `Some(Uuid)` if a matching operation is found, `None` otherwise.
    pub async fn find_operation_by_short_id(&self, short_id: &str) -> Option<Uuid> {
        self.operation_manager.find_operation_by_short_id(short_id).await
    }

    /// Lists all currently active operation identifiers.
    ///
    /// This method returns the UUIDs of all operations that are currently
    /// running or queued for execution.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<String>` containing the string representations of all active operation UUIDs.
    pub async fn list_operation_ids(&self) -> Vec<String> {
        self.operation_manager.list_operation_ids().await
    }

    /// Retrieves comprehensive system status information.
    ///
    /// This method returns detailed information about the current state of the system,
    /// including active operations, context statistics, and resource usage.
    ///
    /// # Returns
    ///
    /// Returns a `Result<SystemStatus>` containing detailed system information.
    pub async fn get_status_data(&self) -> Result<SystemStatus> {
        self.operation_manager.get_status_data(&self.context_manager).await
    }

    /// Clears all contexts from the knowledge base asynchronously.
    ///
    /// This method initiates a background operation to remove all contexts
    /// (both persistent and volatile) from the knowledge base.
    ///
    /// # Returns
    ///
    /// Returns a `Result<(Uuid, CancellationToken)>` for tracking the clear operation.
    pub async fn clear_all(&self) -> Result<(Uuid, CancellationToken)> {
        let operation_id = Uuid::new_v4();
        let cancel_token = CancellationToken::new();

        self.operation_manager
            .register_operation(operation_id, OperationType::Clearing, cancel_token.clone())
            .await;

        let job = IndexingJob::Clear {
            id: operation_id,
            cancel: cancel_token.clone(),
        };

        self.job_tx
            .send(job)
            .map_err(|_send_error| SemanticSearchError::OperationFailed("Background worker unavailable".to_string()))?;

        Ok((operation_id, cancel_token))
    }

    /// Clears all contexts from the knowledge base asynchronously.
    ///
    /// This method initiates a background operation to remove all contexts
    /// (both persistent and volatile) from the knowledge base.
    ///
    /// # Returns
    ///
    /// Returns a `Result<(Uuid, CancellationToken)>` for tracking the clear operation. immediately
    pub async fn clear_all_immediate(&self) -> Result<usize> {
        self.context_manager.clear_all_immediate(&self.base_dir).await
    }

    /// Removes a specific context from the knowledge base.
    ///
    /// This method removes a context identified by its unique ID, cleaning up
    /// both the in-memory representation and any persistent storage.
    ///
    /// # Arguments
    ///
    /// * `context_id` - The unique identifier of the context to remove
    pub async fn remove_context_by_id(&self, context_id: &str) -> Result<()> {
        self.context_manager
            .remove_context_by_id(context_id, &self.base_dir)
            .await
    }

    /// Retrieves a context by its source path.
    ///
    /// This method finds a context that was created from the specified file or directory path.
    ///
    /// # Arguments
    ///
    /// * `path` - The file or directory path used when the context was created
    ///
    /// # Returns
    ///
    /// Returns `Some(KnowledgeContext)` if found, `None` otherwise.
    pub async fn get_context_by_path(&self, path: &str) -> Option<KnowledgeContext> {
        self.context_manager.get_context_by_path(path).await
    }

    /// Retrieves a context by its display name.
    ///
    /// This method finds a context using the human-readable name that was
    /// assigned when the context was created.
    ///
    /// # Arguments
    ///
    /// * `name` - The display name of the context
    ///
    /// # Returns
    ///
    /// Returns `Some(KnowledgeContext)` if found, `None` otherwise.
    pub async fn get_context_by_name(&self, name: &str) -> Option<KnowledgeContext> {
        self.context_manager.get_context_by_name(name).await
    }

    /// Lists the source paths of all available contexts.
    ///
    /// This method returns a list of strings showing the mapping between
    /// context names and their source paths for debugging and informational purposes.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<String>` with entries in the format "name -> path".
    pub async fn list_context_paths(&self) -> Vec<String> {
        self.context_manager.list_context_paths().await
    }
}
