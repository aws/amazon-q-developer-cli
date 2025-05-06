use std::collections::HashMap;
use std::fs::{
    self,
    File,
};
use std::io::{
    BufReader,
    BufWriter,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
};

use serde_json::Value;
use uuid::Uuid;

use crate::embedding::TextEmbedder;
use crate::error::{
    MemoryBankError,
    Result,
};
use crate::index::VectorIndex;
use crate::processing::process_file;
use crate::types::{
    DataPoint,
    MemoryContext,
    ProgressStatus,
    SearchResult,
};

/// Memory bank client for managing semantic memory
pub struct MemoryBankClient {
    /// Base directory for storing persistent contexts
    base_dir: PathBuf,
    /// Short-term (volatile) memory contexts
    volatile_contexts: HashMap<String, Arc<Mutex<SemanticContext>>>,
    /// Long-term (persistent) memory contexts
    persistent_contexts: HashMap<String, MemoryContext>,
    /// Text embedder for generating embeddings
    embedder: TextEmbedder,
}

/// A semantic context containing data points and a vector index
struct SemanticContext {
    /// The data points stored in the index
    data_points: Vec<DataPoint>,
    /// The vector index for fast approximate nearest neighbor search
    index: Option<VectorIndex>,
    /// Path to save/load the data points
    data_path: PathBuf,
}

impl SemanticContext {
    /// Create a new semantic context
    fn new(data_path: PathBuf) -> Result<Self> {
        // Create the directory if it doesn't exist
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a new instance
        let mut context = Self {
            data_points: Vec::new(),
            index: None,
            data_path: data_path.clone(),
        };

        // Load data points if the file exists
        if data_path.exists() {
            let file = File::open(&data_path)?;
            let reader = BufReader::new(file);
            context.data_points = serde_json::from_reader(reader)?;
        }

        // If we have data points, rebuild the index
        if !context.data_points.is_empty() {
            context.rebuild_index()?;
        }

        Ok(context)
    }

    /// Save data points to disk
    fn save(&self) -> Result<()> {
        // Save the data points as JSON
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.data_points)?;

        Ok(())
    }

    /// Rebuild the index from the current data points
    fn rebuild_index(&mut self) -> Result<()> {
        // Create a new index with the current data points
        let index = VectorIndex::new(self.data_points.len().max(100));

        // Add all data points to the index
        for (i, point) in self.data_points.iter().enumerate() {
            index.insert(&point.vector, i);
        }

        // Set the new index
        self.index = Some(index);

        Ok(())
    }

    /// Add data points to the context
    fn add_data_points(&mut self, data_points: Vec<DataPoint>) -> Result<usize> {
        // Store the count before extending the data points
        let count = data_points.len();

        if count == 0 {
            return Ok(0);
        }

        // Add the new points to our data store
        let start_idx = self.data_points.len();
        self.data_points.extend(data_points);
        let end_idx = self.data_points.len();

        // Update the index
        self.update_index_by_range(start_idx, end_idx)?;

        Ok(count)
    }

    /// Update the index with data points in a specific range
    fn update_index_by_range(&mut self, start_idx: usize, end_idx: usize) -> Result<()> {
        // If we don't have an index yet, or if the index is small and we're adding many points,
        // it might be more efficient to rebuild from scratch
        if self.index.is_none() || (self.data_points.len() < 1000 && (end_idx - start_idx) > self.data_points.len() / 2)
        {
            return self.rebuild_index();
        }

        // Get the existing index
        let index = self.index.as_ref().unwrap();

        // Add only the points in the specified range to the index
        for i in start_idx..end_idx {
            index.insert(&self.data_points[i].vector, i);
        }

        Ok(())
    }

    /// Search for similar items to the given vector
    fn search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let index = match &self.index {
            Some(idx) => idx,
            None => return Ok(Vec::new()), // Return empty results if no index
        };

        // Search for the nearest neighbors
        let results = index.search(query_vector, limit, 100);

        // Convert the results to our SearchResult type
        let search_results = results
            .into_iter()
            .map(|(id, distance)| {
                let point = self.data_points[id].clone();
                SearchResult::new(point, distance)
            })
            .collect();

        Ok(search_results)
    }
}

impl MemoryBankClient {
    /// Create a new memory bank client
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base directory for storing persistent contexts
    ///
    /// # Returns
    ///
    /// A new MemoryBankClient instance
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base_dir)?;

        // Initialize the embedding model
        let embedder = TextEmbedder::new()?;

        // Load metadata for persistent contexts
        let contexts_file = base_dir.join("contexts.json");
        let persistent_contexts = if contexts_file.exists() {
            let file = File::open(&contexts_file)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Create the client instance first
        let mut client = Self {
            base_dir,
            volatile_contexts: HashMap::new(),
            persistent_contexts,
            embedder,
        };

        // Now load all persistent contexts
        let context_ids: Vec<String> = client.persistent_contexts.keys().cloned().collect();
        for id in context_ids {
            if let Err(e) = client.load_persistent_context(&id) {
                eprintln!("Failed to load persistent context {}: {}", id, e);
            }
        }

        Ok(client)
    }

    /// Create a new memory bank client with the default base directory
    ///
    /// # Returns
    ///
    /// A new MemoryBankClient instance
    pub fn new_with_default_dir() -> Result<Self> {
        let base_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".memory_bank");

        Self::new(base_dir)
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

        if path.is_dir() {
            // Handle directory
            self.add_context_from_directory(path, name, description, persistent, progress_callback)
        } else if path.is_file() {
            // Handle file
            self.add_context_from_file(path, name, description, persistent, progress_callback)
        } else {
            Err(MemoryBankError::InvalidPath(format!(
                "Path does not exist or is not a file or directory: {}",
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
        let id = Uuid::new_v4().to_string();

        // Create the context directory
        let context_dir = if persistent {
            let context_dir = self.base_dir.join(&id);
            fs::create_dir_all(&context_dir)?;
            context_dir
        } else {
            // For volatile contexts, use a temporary directory
            let temp_dir = std::env::temp_dir().join("memory_bank").join(&id);
            fs::create_dir_all(&temp_dir)?;
            temp_dir
        };

        // Notify progress: Starting indexing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::StartingIndexing(1));
        }

        // Process the file
        let items = process_file(file_path)?;

        // Notify progress: Indexing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Indexing(1, 1));
        }

        // Notify progress: Creating semantic context
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::CreatingSemanticContext);
        }

        // Create a new semantic context
        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Add the items to the context
        let mut data_points = Vec::new();
        let total_items = items.len();

        // Process items with progress updates for embedding generation
        for (i, item) in items.iter().enumerate() {
            // Extract the text from the item
            let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Update progress for embedding generation
            if let Some(ref callback) = progress_callback {
                if i % 10 == 0 {
                    callback(ProgressStatus::GeneratingEmbeddings(i, total_items));
                }
            }

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

            // Create a data point
            data_points.push(DataPoint { id: i, payload, vector });
        }

        // Notify progress: Building index
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::BuildingIndex);
        }

        // Add the data points to the context
        let item_count = semantic_context.add_data_points(data_points)?;

        // Notify progress: Finalizing
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Finalizing);
        }

        // Save to disk if persistent
        if persistent {
            semantic_context.save()?;
        }

        // Create the context metadata
        let context = MemoryContext::new(
            id.clone(),
            name,
            description,
            persistent,
            Some(file_path.to_string_lossy().to_string()),
            item_count,
        );

        // Store the context
        if persistent {
            self.persistent_contexts.insert(id.clone(), context);
            self.save_contexts_metadata()?;
        }

        // Store the semantic context
        self.volatile_contexts
            .insert(id.clone(), Arc::new(Mutex::new(semantic_context)));

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
    ) -> Result<String>
    where
        F: Fn(ProgressStatus) + Send + 'static,
    {
        let dir_path = dir_path.as_ref();

        // Generate a unique ID for this context
        let id = Uuid::new_v4().to_string();

        // Create the context directory
        let context_dir = if persistent {
            let context_dir = self.base_dir.join(&id);
            fs::create_dir_all(&context_dir)?;
            context_dir
        } else {
            // For volatile contexts, use a temporary directory
            let temp_dir = std::env::temp_dir().join("memory_bank").join(&id);
            fs::create_dir_all(&temp_dir)?;
            temp_dir
        };

        // Notify progress: Getting file count
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::CountingFiles);
        }

        // Count files first to provide progress information
        let mut file_count = 0;
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

            file_count += 1;
        }

        // Notify progress: Starting indexing
        if let Some(ref callback) = progress_callback {
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
            match process_file(path) {
                Ok(mut file_items) => items.append(&mut file_items),
                Err(_) => continue, // Skip files that fail to process
            }

            processed_files += 1;

            // Update progress
            if let Some(ref callback) = progress_callback {
                callback(ProgressStatus::Indexing(processed_files, file_count));
            }
        }

        // Notify progress: Creating semantic context (50% progress point)
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::CreatingSemanticContext);
        }

        // Create a new semantic context
        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Add the items to the context
        let mut data_points = Vec::new();
        let total_items = items.len();

        // Process items with progress updates for embedding generation
        for (i, item) in items.iter().enumerate() {
            // Extract the text from the item
            let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Update progress for embedding generation (50% to 80% progress range)
            if let Some(ref callback) = progress_callback {
                if i % 10 == 0 {
                    // Calculate progress percentage between 50-80%
                    callback(ProgressStatus::GeneratingEmbeddings(i, total_items));
                }
            }

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

            // Create a data point
            data_points.push(DataPoint { id: i, payload, vector });
        }

        // Notify progress: Building index (80% progress point)
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::BuildingIndex);
        }

        // Add the data points to the context
        let item_count = semantic_context.add_data_points(data_points)?;

        // Notify progress: Finalizing (90% progress point)
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Finalizing);
        }

        // Save to disk if persistent
        if persistent {
            semantic_context.save()?;
        }

        // Create the context metadata
        let context = MemoryContext::new(
            id.clone(),
            name,
            description,
            persistent,
            Some(dir_path.to_string_lossy().to_string()),
            item_count,
        );

        // Store the context
        if persistent {
            self.persistent_contexts.insert(id.clone(), context);
            self.save_contexts_metadata()?;
        }

        // Store the semantic context
        self.volatile_contexts
            .insert(id.clone(), Arc::new(Mutex::new(semantic_context)));

        // Notify progress: Complete (100% progress point)
        if let Some(ref callback) = progress_callback {
            callback(ProgressStatus::Complete);
        }

        Ok(id)
    }

    /// Add a context from text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to add
    /// * `name` - Name for the context
    /// * `description` - Description of the context
    /// * `persistent` - Whether to make this context persistent
    ///
    /// # Returns
    ///
    /// The ID of the created context
    pub fn add_context_from_text(
        &mut self,
        text: &str,
        name: &str,
        description: &str,
        persistent: bool,
    ) -> Result<String> {
        // Generate a unique ID for this context
        let id = Uuid::new_v4().to_string();

        // Create the context directory
        let context_dir = if persistent {
            let context_dir = self.base_dir.join(&id);
            fs::create_dir_all(&context_dir)?;
            context_dir
        } else {
            // For volatile contexts, use a temporary directory
            let temp_dir = std::env::temp_dir().join("memory_bank").join(&id);
            fs::create_dir_all(&temp_dir)?;
            temp_dir
        };

        // Create a new semantic context
        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))?;

        // Generate an embedding for the text
        let vector = self.embedder.embed(text)?;

        // Create a data point
        let mut payload = HashMap::new();
        payload.insert("text".to_string(), Value::String(text.to_string()));

        let data_point = DataPoint { id: 0, payload, vector };

        // Add the data point to the context
        semantic_context.add_data_points(vec![data_point])?;

        // Save to disk if persistent
        if persistent {
            semantic_context.save()?;
        }

        // Create the context metadata
        let context = MemoryContext::new(id.clone(), name, description, persistent, None, 0);

        // Store the context
        if persistent {
            self.persistent_contexts.insert(id.clone(), context);
            self.save_contexts_metadata()?;
        }

        // Store the semantic context
        self.volatile_contexts
            .insert(id.clone(), Arc::new(Mutex::new(semantic_context)));

        Ok(id)
    }

    /// Get all contexts
    ///
    /// # Returns
    ///
    /// A vector of all contexts (both volatile and persistent)
    pub fn get_all_contexts(&self) -> Vec<MemoryContext> {
        let mut contexts = Vec::new();

        // Add persistent contexts
        for context in self.persistent_contexts.values() {
            contexts.push(context.clone());
        }

        // Add volatile contexts that aren't already in persistent contexts
        for id in self.volatile_contexts.keys() {
            if !self.persistent_contexts.contains_key(id) {
                // Create a temporary context object for volatile contexts
                let context = MemoryContext::new(
                    id.clone(),
                    "Volatile Context",
                    "Temporary memory context",
                    false,
                    None,
                    0,
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
    /// * `query` - Search query
    /// * `limit` - Maximum number of results to return per context
    ///
    /// # Returns
    ///
    /// A vector of (context_id, results) pairs
    pub fn search_all(&self, query: &str, limit: usize) -> Result<Vec<(String, Vec<SearchResult>)>> {
        // Generate an embedding for the query
        let query_vector = self.embedder.embed(query)?;

        let mut all_results = Vec::new();

        // Search in all volatile contexts
        for (id, context) in &self.volatile_contexts {
            let context_guard = context
                .lock()
                .map_err(|e| MemoryBankError::OperationFailed(format!("Failed to acquire lock on context: {}", e)))?;

            match context_guard.search(&query_vector, limit) {
                Ok(results) => {
                    if !results.is_empty() {
                        all_results.push((id.clone(), results));
                    }
                },
                Err(_) => continue, // Skip contexts that fail to search
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
    /// * `query` - Search query
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// A vector of search results
    pub fn search_context(&self, context_id: &str, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Generate an embedding for the query
        let query_vector = self.embedder.embed(query)?;

        let context = self
            .volatile_contexts
            .get(context_id)
            .ok_or_else(|| MemoryBankError::ContextNotFound(context_id.to_string()))?;

        let context_guard = context
            .lock()
            .map_err(|e| MemoryBankError::OperationFailed(format!("Failed to acquire lock on context: {}", e)))?;

        context_guard.search(&query_vector, limit)
    }

    /// Get all contexts
    ///
    /// # Returns
    ///
    /// A vector of memory contexts
    pub fn get_contexts(&self) -> Vec<MemoryContext> {
        self.persistent_contexts.values().cloned().collect()
    }

    /// Make a context persistent
    ///
    /// # Arguments
    ///
    /// * `context_id` - ID of the context to make persistent
    /// * `name` - Name for the persistent context
    /// * `description` - Description of the persistent context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn make_persistent(&mut self, context_id: &str, name: &str, description: &str) -> Result<()> {
        // Check if the context exists
        let context = self
            .volatile_contexts
            .get(context_id)
            .ok_or_else(|| MemoryBankError::ContextNotFound(context_id.to_string()))?;

        // Create the persistent context directory
        let persistent_dir = self.base_dir.join(context_id);
        fs::create_dir_all(&persistent_dir)?;

        // Get the context data
        let context_guard = context
            .lock()
            .map_err(|e| MemoryBankError::OperationFailed(format!("Failed to acquire lock on context: {}", e)))?;

        // Save the data to the persistent directory
        let data_path = persistent_dir.join("data.json");
        let file = File::create(&data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &context_guard.data_points)?;

        // Create the context metadata
        let context_meta = MemoryContext::new(context_id.to_string(), name, description, true, None, 0);

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
    /// * `delete_persistent` - Whether to delete persistent storage for this context
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn remove_context_by_id(&mut self, context_id: &str, delete_persistent: bool) -> Result<()> {
        // Remove from volatile contexts
        self.volatile_contexts.remove(context_id);

        // Remove from persistent contexts if needed
        if delete_persistent {
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
            Err(MemoryBankError::ContextNotFound(format!(
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
            Err(MemoryBankError::ContextNotFound(format!(
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
            return Err(MemoryBankError::ContextNotFound(context_id.to_string()));
        }

        // Check if the context is already loaded
        if self.volatile_contexts.contains_key(context_id) {
            return Ok(());
        }

        // Create the context directory path
        let context_dir = self.base_dir.join(context_id);
        if !context_dir.exists() {
            return Err(MemoryBankError::InvalidPath(format!(
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
        let file = File::create(&contexts_file)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.persistent_contexts)?;

        Ok(())
    }
}
