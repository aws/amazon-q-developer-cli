use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::client::SemanticContext;

/// Type alias for context ID
pub type ContextId = String;

/// Type alias for search results
pub type SearchResults = Vec<SearchResult>;

/// Type alias for context map
pub type ContextMap = HashMap<ContextId, Arc<Mutex<SemanticContext>>>;

/// A memory context containing semantic information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryContext {
    /// Unique identifier for the context
    pub id: String,

    /// Human-readable name for the context
    pub name: String,

    /// Description of the context
    pub description: String,

    /// When the context was created
    pub created_at: DateTime<Utc>,

    /// When the context was last updated
    pub updated_at: DateTime<Utc>,

    /// Whether this context is persistent (saved to disk)
    pub persistent: bool,

    /// Original source path if created from a directory
    pub source_path: Option<String>,

    /// Number of items in the context
    pub item_count: usize,
}

impl MemoryContext {
    /// Create a new memory context
    pub fn new(
        id: String,
        name: &str,
        description: &str,
        persistent: bool,
        source_path: Option<String>,
        item_count: usize,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name: name.to_string(),
            description: description.to_string(),
            created_at: now,
            updated_at: now,
            source_path,
            persistent,
            item_count,
        }
    }
}

/// A data point in the semantic index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Unique identifier for the data point
    pub id: usize,

    /// Metadata associated with the data point
    pub payload: HashMap<String, serde_json::Value>,

    /// Vector representation of the data point
    pub vector: Vec<f32>,
}

/// A search result from the semantic index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The data point that matched
    pub point: DataPoint,

    /// Distance/similarity score (lower is better)
    pub distance: f32,
}

impl SearchResult {
    /// Create a new search result
    pub fn new(point: DataPoint, distance: f32) -> Self {
        Self { point, distance }
    }

    /// Get the text content of this result
    pub fn text(&self) -> Option<&str> {
        self.point.payload.get("text").and_then(|v| v.as_str())
    }
}

/// File type for processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Plain text file
    Text,
    /// Markdown file
    Markdown,
    /// JSON file
    Json,
    /// Source code file (programming languages)
    Code,
    /// Unknown file type
    Unknown,
}

/// Progress status for indexing operations
#[derive(Debug, Clone)]
pub enum ProgressStatus {
    /// Counting files in the directory
    CountingFiles,
    /// Starting the indexing process with total file count
    StartingIndexing(usize),
    /// Indexing in progress with current file and total count
    Indexing(usize, usize),
    /// Creating semantic context (50% progress point)
    CreatingSemanticContext,
    /// Generating embeddings for items (50-80% progress range)
    GeneratingEmbeddings(usize, usize),
    /// Building vector index (80% progress point)
    BuildingIndex,
    /// Finalizing the index (90% progress point)
    Finalizing,
    /// Indexing complete (100% progress point)
    Complete,
}
