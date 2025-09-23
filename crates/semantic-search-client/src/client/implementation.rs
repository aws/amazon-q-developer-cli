use std::collections::HashMap;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
};

use serde_json::Value;

use crate::client::context::SemanticContext;
use crate::client::{
    embedder_factory,
    utils,
};
use crate::config;
use crate::embedding::TextEmbedderTrait;
use crate::error::{
    Result,
    SemanticSearchError,
};
use crate::processing::process_file_with_config;
use crate::types::{
    ContextId,
    ContextMap,
    DataPoint,
    KnowledgeContext,
    ProgressStatus,
    SearchResults,
};

/// Semantic search client for managing semantic memory
///
/// This client provides functionality for creating, managing, and searching
/// through semantic memory contexts. It supports both volatile (in-memory)
/// and persistent (on-disk) contexts.
///
/// # Examples
///
/// ```no_run
/// use semantic_search_client::SemanticSearchClient;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = SemanticSearchClient::new_with_default_dir()?;
/// let context_id = client.add_context_from_text(
///     "This is a test text for semantic memory",
///     "Test Context",
///     "A test context",
///     false,
/// )?;
/// # Ok(())
/// # }
/// ```
pub struct SemanticSearchClient {
    /// Base directory for storing persistent contexts
    base_dir: PathBuf,
    /// Short-term (volatile) memory contexts
    volatile_contexts: ContextMap,
    /// Long-term (persistent) memory contexts
    persistent_contexts: HashMap<ContextId, KnowledgeContext>,
    /// Text embedder for generating embeddings
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    embedder: Box<dyn TextEmbedderTrait>,
    /// Text embedder for generating embeddings (Linux only)
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    embedder: Box<dyn TextEmbedderTrait>,
    /// Configuration for the client
    config: crate::config::SemanticSearchConfig,
}
impl SemanticSearchClient {
    /// Create a new semantic search client
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base directory for storing persistent contexts
    ///
    /// # Returns
    ///
    /// A new SemanticSearchClient instance
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir_path = base_dir.as_ref().to_path_buf();
        let config = crate::config::SemanticSearchConfig {
            base_dir: base_dir_path,
            ..Default::default()
        };
        Self::with_config(base_dir, config)
    }

    /// Create a new semantic search client with custom configuration
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base directory for storing persistent contexts
    /// * `config` - Configuration for the client
    ///
    /// # Returns
    ///
    /// A new SemanticSearchClient instance
    pub fn with_config(base_dir: impl AsRef<Path>, config: crate::config::SemanticSearchConfig) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base_dir)?;

        // Create models directory
        crate::config::ensure_models_dir(&base_dir)?;

        // Initialize the configuration
        if let Err(e) = config::init_config(&base_dir) {
            tracing::error!("Failed to initialize semantic search configuration: {}", e);
            // Continue with default config if initialization fails
        }

        let embedder = embedder_factory::create_embedder(config.embedding_type)?;

        // Load metadata for persistent contexts
        let contexts_file = base_dir.join("contexts.json");
        let persistent_contexts = utils::load_json_from_file(&contexts_file)?;

        // Create the client instance first
        let mut client = Self {
            base_dir,
            volatile_contexts: HashMap::new(),
            persistent_contexts,
            embedder,
            config,
        };

        // Now load all persistent contexts
        let context_ids: Vec<String> = client.persistent_contexts.keys().cloned().collect();
        for id in context_ids {
            if let Err(e) = client.load_persistent_context(&id) {
                tracing::error!("Failed to load persistent context {}: {}", id, e);
            }
        }

        Ok(client)
    }

    /// Get the default base directory for memory bank
    ///
    /// # Returns
    ///
    /// The default base directory path
    pub fn get_default_base_dir() -> PathBuf {
        crate::config::get_default_base_dir()
    }

    /// Get the models directory path
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base directory for memory bank
    ///
    /// # Returns
    ///
    /// The models directory path
    pub fn get_models_dir(base_dir: &Path) -> PathBuf {
        crate::config::get_models_dir(base_dir)
    }

    /// Create a new semantic search client with the default base directory
    ///
    /// # Returns
    ///
    /// A new SemanticSearchClient instance
    pub fn new_with_default_dir() -> Result<Self> {
        let base_dir = Self::get_default_base_dir();
        Self::new(base_dir)
    }

    /// Get the current semantic search configuration
    ///
    /// # Returns
    ///
    /// A reference to the current configuration
    pub fn get_config(&self) -> &'static config::SemanticSearchConfig {
        config::get_config()
    }

    /// Update the semantic search configuration
    ///
    /// # Arguments
    ///
    /// * `new_config` - The new configuration to use
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn update_config(&self, new_config: config::SemanticSearchConfig) -> std::io::Result<()> {
        config::update_config(&self.base_dir, new_config)
    }

    /// Validate inputs
    fn validate_input(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Add a context from a path (file or directory)
    ///
    /// # Arguments
    ///
    /// * `path` - Path to a file or directory
    /// * `name` - Name for the context
    /// * `description` - Description of the context
    /// * `persistent` - Whether to make this context persistent
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// The ID of the created context
    pub fn add_context_from_path<F>(
        &mut self,
        path: impl AsRef<Path>,
        name: &str,
        description: &str,
        persistent: bool,
        progress_callback: Option<F>,
    ) -> Result<String>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        let path = path.as_ref();

        // Validate inputs
        Self::validate_input(name)?;

        if !path.exists() {
            return Err(SemanticSearchError::InvalidPath(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        if path.is_dir() {
            // Handle directory
            self.add_context_from_directory(path, name, description, persistent, progress_callback)
        } else if path.is_file() {
            // Handle file
            self.add_context_from_file(path, name, description, persistent, progress_callback)
        } else {
            Err(SemanticSearchError::InvalidPath(format!(
                "Path is not a file or directory: {}",
                path.display()
            )))
        }
    }

    /// Add a context from a file
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the file
    /// * `name` - Name for the context
    /// * `description` - Description of the context
    /// * `persistent` - Whether to make this context persistent
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// The ID of the created context
    fn add_context_from_file<F>(
        &mut self,
        file_path: impl AsRef<Path>,
        name: &str,
        description: &str,
        persistent: bool,
        progress_callback: Option<F>,
    ) -> Result<String>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        let file_path = file_path.as_ref();

        // Notify progress: Starting
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::CountingFiles);
        }

        // Generate a unique ID for this context
        let id = utils::generate_context_id();

        // Create the context directory
        let context_dir = self.create_context_directory(&id, persistent)?;

        // Notify progress: Starting indexing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::StartingIndexing(1));
        }

        // Process the file
        let items = process_file_with_config(file_path, Some(self.config.chunk_size), Some(self.config.chunk_overlap))?;

        // Notify progress: Indexing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Indexing(1, 1));
        }

        // Create a semantic context from the items
        let semantic_context = self.create_semantic_context(&context_dir, &items, &progress_callback)?;

        // Notify progress: Finalizing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Finalizing);
        }

        // Save and store the context
        self.save_and_store_context(
            &id,
            name,
            description,
            persistent,
            Some(file_path.to_string_lossy().to_string()),
            semantic_context,
        )?;

        // Notify progress: Complete
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Complete);
        }

        Ok(id)
    }

    /// Add a context from a directory
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory
    /// * `name` - Name for the context
    /// * `description` - Description of the context
    /// * `persistent` - Whether to make this context persistent
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// The ID of the created context
    pub fn add_context_from_directory<F>(
        &mut self,
        dir_path: impl AsRef<Path>,
        name: &str,
        description: &str,
        persistent: bool,
        progress_callback: Option<F>,
    ) -> Result<ContextId>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        let dir_path = dir_path.as_ref();

        // Generate a unique ID for this context
        let id = utils::generate_context_id();

        // Create context directory
        let context_dir = self.create_context_directory(&id, persistent)?;

        // Count files and notify progress
        let file_count = Self::count_files_in_directory(dir_path, &progress_callback)?;

        // Check if file count exceeds the configured limit
        if file_count > self.config.max_files {
            return Err(SemanticSearchError::InvalidArgument(format!(
                "Directory contains {} files, which exceeds the maximum limit of {} files. Please choose a smaller directory or increase the max_files limit in the configuration.",
                file_count, self.config.max_files
            )));
        }

        // Process files
        let items = Self::process_directory_files(dir_path, file_count, &progress_callback, &self.config)?;

        // Create and populate semantic context
        let semantic_context = self.create_semantic_context(&context_dir, &items, &progress_callback)?;

        // Save and store context
        self.save_and_store_context(
            &id,
            name,
            description,
            persistent,
            Some(dir_path.to_string_lossy().to_string()),
            semantic_context,
        )?;

        Ok(id)
    }

    /// Create a context directory
    fn create_context_directory(&self, id: &str, persistent: bool) -> Result<PathBuf> {
        utils::create_context_directory(&self.base_dir, id, persistent)
    }

    /// Count files in a directory
    fn count_files_in_directory<F>(dir_path: &Path, progress_callback: &Option<F>) -> Result<usize>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        utils::count_files_in_directory(dir_path, progress_callback)
    }

    /// Process files in a directory
    fn process_directory_files<F>(
        dir_path: &Path,
        file_count: usize,
        progress_callback: &Option<F>,
        config: &crate::config::SemanticSearchConfig,
    ) -> Result<Vec<Value>>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        // Notify progress: Starting indexing
        if let Some(callback) = progress_callback {
            callback(ProgressStatus::StartingIndexing(file_count));
        }

        // Process all files in the directory with progress updates
        let mut processed_files = 0;
        let mut items = Vec::new();

        for entry in walkdir::WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
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
            match process_file_with_config(path, Some(config.chunk_size), Some(config.chunk_overlap)) {
                Ok(mut file_items) => items.append(&mut file_items),
                Err(_) => continue, // Skip files that fail to process
            }

            processed_files += 1;

            // Update progress
            if let Some(callback) = progress_callback {
                callback(ProgressStatus::Indexing(processed_files, file_count));
            }
        }

        Ok(items)
    }

    /// Create a semantic context from items
    fn create_semantic_context<F>(
        &self,
        context_dir: &Path,
        items: &[Value],
        progress_callback: &Option<F>,
    ) -> Result<SemanticContext>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        // Notify progress: Creating semantic context
        if let Some(callback) = progress_callback {
            callback(ProgressStatus::CreatingSemanticContext);
        }

        // Create a new semantic context
        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Process items to data points
        let data_points = self.process_items_to_data_points(items, progress_callback)?;

        // Notify progress: Building index
        if let Some(callback) = progress_callback {
            callback(ProgressStatus::BuildingIndex);
        }

        // Add the data points to the context
        semantic_context.add_data_points(data_points)?;

        Ok(semantic_context)
    }

    fn process_items_to_data_points<F>(&self, items: &[Value], progress_callback: &Option<F>) -> Result<Vec<DataPoint>>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        let mut data_points = Vec::new();
        let total_items = items.len();

        // Process items with progress updates for embedding generation
        for (i, item) in items.iter().enumerate() {
            // Update progress for embedding generation
            if let Some(callback) = progress_callback {
                if i % 10 == 0 {
                    callback(ProgressStatus::GeneratingEmbeddings(i, total_items));
                }
            }

            // Create a data point from the item
            let data_point = self.create_data_point_from_item(item, i)?;
            data_points.push(data_point);
        }

        Ok(data_points)
    }

    /// Save and store context
    fn save_and_store_context(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        persistent: bool,
        source_path: Option<String>,
        semantic_context: SemanticContext,
    ) -> Result<()> {
        // Notify progress: Finalizing (90% progress point)
        let item_count = semantic_context.get_data_points().len();

        // Save to disk if persistent
        if persistent {
            semantic_context.save()?;
        }

        // Create the context metadata
        let context = KnowledgeContext::new(
            id.to_string(),
            name,
            description,
            persistent,
            source_path,
            (vec![], vec![]),
            item_count,
            self.config.embedding_type,
        );

        // Store the context
        if persistent {
            self.persistent_contexts.insert(id.to_string(), context);
            self.save_contexts_metadata()?;
        }

        // Store the semantic context
        self.volatile_contexts
            .insert(id.to_string(), Arc::new(Mutex::new(semantic_context)));

        Ok(())
    }

    /// Create a data point from text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to create a data point from
    /// * `id` - The ID for the data point
    ///
    /// # Returns
    ///
    /// A new DataPoint
    fn create_data_point_from_text(&self, text: &str, id: usize) -> Result<DataPoint> {
        // Generate an embedding for the text
        let vector = self.embedder.embed(text)?;

        // Create a data point
        let mut payload = HashMap::new();
        payload.insert("text".to_string(), Value::String(text.to_string()));

        Ok(DataPoint { id, payload, vector })
    }

    /// Create a data point from a JSON item
    ///
    /// # Arguments
    ///
    /// * `item` - The JSON item to create a data point from
    /// * `id` - The ID for the data point
    ///
    /// # Returns
    ///
    /// A new DataPoint
    fn create_data_point_from_item(&self, item: &Value, id: usize) -> Result<DataPoint> {
        // Extract the text from the item
        let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");

        // Generate an embedding for the text
        let vector = self.embedder.embed(text)?;

        // Convert Value to HashMap
        let payload: HashMap<String, Value> = if let Value::Object(map) = item {
            map.clone().into_iter().collect()
        } else {
            let mut map = HashMap::new();
            map.insert("text".to_string(), item.clone());
            map
        };

        Ok(DataPoint { id, payload, vector })
    }

    /// Add a context from text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to add
    /// * `context_name` - Name for the context
    /// * `context_description` - Description of the context
    /// * `is_persistent` - Whether to make this context persistent
    ///
    /// # Returns
    ///
    /// The ID of the created context
    pub fn add_context_from_text(
        &mut self,
        text: &str,
        context_name: &str,
        context_description: &str,
        is_persistent: bool,
    ) -> Result<String> {
        // Validate inputs
        if text.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Text content cannot be empty".to_string(),
            ));
        }

        if context_name.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context name cannot be empty".to_string(),
            ));
        }

        // Generate a unique ID for this context
        let context_id = utils::generate_context_id();

        // Create the context directory
        let context_dir = self.create_context_directory(&context_id, is_persistent)?;

        // Create a new semantic context
        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Create a data point from the text
        let data_point = self.create_data_point_from_text(text, 0)?;

        // Add the data point to the context
        semantic_context.add_data_points(vec![data_point])?;

        // Save to disk if persistent
        if is_persistent {
            semantic_context.save()?;
        }

        // Save and store the context
        self.save_and_store_context(
            &context_id,
            context_name,
            context_description,
            is_persistent,
            None,
            semantic_context,
        )?;

        Ok(context_id)
    }

    /// Get all contexts
    ///
    /// # Returns
    ///
    /// A vector of all contexts (both volatile and persistent)
    pub fn get_all_contexts(&self) -> Vec<KnowledgeContext> {
        let mut contexts = Vec::new();

        // Add persistent contexts
        for context in self.persistent_contexts.values() {
            contexts.push(context.clone());
        }

        // Add volatile contexts that aren't already in persistent contexts
        for id in self.volatile_contexts.keys() {
            if !self.persistent_contexts.contains_key(id) {
                // Create a temporary context object for volatile contexts
                let context = KnowledgeContext::new(
                    id.clone(),
                    "Volatile Context",
                    "Temporary memory context",
                    false,
                    None,
                    (vec![], vec![]),
                    0,
                    self.config.embedding_type, // Use client default
                );
                contexts.push(context);
            }
        }

        contexts
    }

    /// Search across all contexts
    ///
    /// # Arguments
    ///
    /// * `query_text` - Search query
    /// * `result_limit` - Maximum number of results to return per context (if None, uses
    ///   default_results from config)
    ///
    /// # Returns
    ///
    /// A vector of (context_id, results) pairs
    pub fn search_all(&self, query_text: &str, result_limit: Option<usize>) -> Result<Vec<(ContextId, SearchResults)>> {
        // Validate inputs
        if query_text.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Query text cannot be empty".to_string(),
            ));
        }

        // Use the configured default_results if limit is None
        let effective_limit = result_limit.unwrap_or_else(|| config::get_config().default_results);

        // Generate an embedding for the query
        let query_vector = self.embedder.embed(query_text)?;

        let mut all_results = Vec::new();

        // Search in all volatile contexts
        for (context_id, context) in &self.volatile_contexts {
            let context_guard = context.lock().map_err(|e| {
                SemanticSearchError::OperationFailed(format!("Failed to acquire lock on context: {}", e))
            })?;

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

        // Sort contexts by best match
        all_results.sort_by(|(_, a), (_, b)| {
            if a.is_empty() {
                return std::cmp::Ordering::Greater;
            }
            if b.is_empty() {
                return std::cmp::Ordering::Less;
            }
            a[0].distance
                .partial_cmp(&b[0].distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(all_results)
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
    pub fn search_context(
        &self,
        context_id: &str,
        query_text: &str,
        result_limit: Option<usize>,
    ) -> Result<SearchResults> {
        // Validate inputs
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

        // Use the configured default_results if limit is None
        let effective_limit = result_limit.unwrap_or_else(|| config::get_config().default_results);

        // Generate an embedding for the query
        let query_vector = self.embedder.embed(query_text)?;

        let context = self
            .volatile_contexts
            .get(context_id)
            .ok_or_else(|| SemanticSearchError::ContextNotFound(context_id.to_string()))?;

        let context_guard = context
            .lock()
            .map_err(|e| SemanticSearchError::OperationFailed(format!("Failed to acquire lock on context: {}", e)))?;

        context_guard.search(&query_vector, effective_limit)
    }

    /// Get all contexts
    ///
    /// # Returns
    ///
    /// A vector of memory contexts
    pub fn get_contexts(&self) -> Vec<KnowledgeContext> {
        self.persistent_contexts.values().cloned().collect()
    }

    /// Make a context persistent
    ///
    /// # Arguments
    ///
    /// * `context_id` - ID of the context to make persistent
    /// * `context_name` - Name for the persistent context
    /// * `context_description` - Description of the persistent context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn make_persistent(&mut self, context_id: &str, context_name: &str, context_description: &str) -> Result<()> {
        // Validate inputs
        if context_id.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context ID cannot be empty".to_string(),
            ));
        }

        if context_name.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context name cannot be empty".to_string(),
            ));
        }

        // Check if the context exists
        let context = self
            .volatile_contexts
            .get(context_id)
            .ok_or_else(|| SemanticSearchError::ContextNotFound(context_id.to_string()))?;

        // Create the persistent context directory
        let persistent_dir = self.base_dir.join(context_id);
        fs::create_dir_all(&persistent_dir)?;

        // Get the context data
        let context_guard = context
            .lock()
            .map_err(|e| SemanticSearchError::OperationFailed(format!("Failed to acquire lock on context: {}", e)))?;

        // Save the data to the persistent directory
        let data_path = persistent_dir.join("data.json");
        utils::save_json_to_file(&data_path, context_guard.get_data_points())?;

        // Create the context metadata
        let context_meta = KnowledgeContext::new(
            context_id.to_string(),
            context_name,
            context_description,
            true,
            None,
            (vec![], vec![]),
            context_guard.get_data_points().len(),
            self.config.embedding_type, // Use client default
        );

        // Store the context metadata
        self.persistent_contexts.insert(context_id.to_string(), context_meta);
        self.save_contexts_metadata()?;

        Ok(())
    }

    /// Remove a context by ID
    ///
    /// # Arguments
    ///
    /// * `context_id` - ID of the context to remove
    /// * `delete_persistent_storage` - Whether to delete persistent storage for this context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn remove_context_by_id(&mut self, context_id: &str, delete_persistent_storage: bool) -> Result<()> {
        // Validate inputs
        if context_id.is_empty() {
            return Err(SemanticSearchError::InvalidArgument(
                "Context ID cannot be empty".to_string(),
            ));
        }

        // Check if the context exists before attempting removal
        let context_exists =
            self.volatile_contexts.contains_key(context_id) || self.persistent_contexts.contains_key(context_id);

        if !context_exists {
            return Err(SemanticSearchError::ContextNotFound(context_id.to_string()));
        }

        // Remove from volatile contexts
        self.volatile_contexts.remove(context_id);

        // Remove from persistent contexts if needed
        if delete_persistent_storage {
            if self.persistent_contexts.remove(context_id).is_some() {
                self.save_contexts_metadata()?;
            }

            // Delete the persistent directory
            let persistent_dir = self.base_dir.join(context_id);
            if persistent_dir.exists() {
                fs::remove_dir_all(persistent_dir)?;
            }
        }

        Ok(())
    }

    /// Remove a context by name
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the context to remove
    /// * `delete_persistent` - Whether to delete persistent storage for this context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn remove_context_by_name(&mut self, name: &str, delete_persistent: bool) -> Result<()> {
        // Find the context ID by name
        let context_id = self
            .persistent_contexts
            .iter()
            .find(|(_, ctx)| ctx.name == name)
            .map(|(id, _)| id.clone());

        if let Some(id) = context_id {
            self.remove_context_by_id(&id, delete_persistent)
        } else {
            Err(SemanticSearchError::ContextNotFound(format!(
                "No context found with name: {}",
                name
            )))
        }
    }

    /// Remove a context by path
    ///
    /// # Arguments
    ///
    /// * `path` - Path associated with the context to remove
    /// * `delete_persistent` - Whether to delete persistent storage for this context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn remove_context_by_path(&mut self, path: &str, delete_persistent: bool) -> Result<()> {
        // Find the context ID by path
        let context_id = self
            .persistent_contexts
            .iter()
            .find(|(_, ctx)| ctx.source_path.as_ref().is_some_and(|p| p == path))
            .map(|(id, _)| id.clone());

        if let Some(id) = context_id {
            self.remove_context_by_id(&id, delete_persistent)
        } else {
            Err(SemanticSearchError::ContextNotFound(format!(
                "No context found with path: {}",
                path
            )))
        }
    }

    /// Remove a context (legacy method for backward compatibility)
    ///
    /// # Arguments
    ///
    /// * `context_id_or_name` - ID or name of the context to remove
    /// * `delete_persistent` - Whether to delete persistent storage for this context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn remove_context(&mut self, context_id_or_name: &str, delete_persistent: bool) -> Result<()> {
        // Try to remove by ID first
        if self.persistent_contexts.contains_key(context_id_or_name)
            || self.volatile_contexts.contains_key(context_id_or_name)
        {
            return self.remove_context_by_id(context_id_or_name, delete_persistent);
        }

        // If not found by ID, try by name
        self.remove_context_by_name(context_id_or_name, delete_persistent)
    }

    /// Load a persistent context
    ///
    /// # Arguments
    ///
    /// * `context_id` - ID of the context to load
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn load_persistent_context(&mut self, context_id: &str) -> Result<()> {
        // Check if the context exists in persistent contexts
        if !self.persistent_contexts.contains_key(context_id) {
            return Err(SemanticSearchError::ContextNotFound(context_id.to_string()));
        }

        // Check if the context is already loaded
        if self.volatile_contexts.contains_key(context_id) {
            return Ok(());
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
        let semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Store the semantic context
        self.volatile_contexts
            .insert(context_id.to_string(), Arc::new(Mutex::new(semantic_context)));

        Ok(())
    }

    /// Save contexts metadata to disk
    fn save_contexts_metadata(&self) -> Result<()> {
        let contexts_file = self.base_dir.join("contexts.json");
        utils::save_json_to_file(&contexts_file, &self.persistent_contexts)
    }
}
