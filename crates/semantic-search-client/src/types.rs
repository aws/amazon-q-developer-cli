use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::SystemTime;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::client::SemanticContext;

/// Type alias for context ID
pub type ContextId = String;

/// Type alias for search results
pub type SearchResults = Vec<SearchResult>;

/// Type alias for context map
pub type ContextMap = HashMap<ContextId, Arc<Mutex<SemanticContext>>>;

/// A memory context containing semantic information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeContext {
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

impl KnowledgeContext {
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

/// Handle for tracking active operations
#[derive(Debug)]
pub struct OperationHandle {
    pub(crate) operation_type: OperationType,
    pub(crate) started_at: SystemTime,
    pub(crate) progress: Arc<tokio::sync::Mutex<ProgressInfo>>,
    pub(crate) cancel_token: CancellationToken,
    /// Task handle for proper cancellation
    pub(crate) task_handle: Option<tokio::task::AbortHandle>,
}

/// Type of operation being performed
#[derive(Debug, Clone)]
pub enum OperationType {
    /// Indexing operation with name and path
    Indexing {
        /// Display name for the operation
        name: String,
        /// Path being indexed
        path: String,
    },
    /// Clearing all contexts
    Clearing,
}

impl OperationType {
    /// Get display name for the operation
    pub fn display_name(&self) -> String {
        match self {
            OperationType::Indexing { name, .. } => format!("Indexing '{}'", name),
            OperationType::Clearing => "Clearing all".to_string(),
        }
    }
}

/// Status information for a single operation (data contract for UI)
#[derive(Debug, Clone)]
pub struct OperationStatus {
    /// Full operation ID
    pub id: String,
    /// Short operation ID (first 8 characters)
    pub short_id: String,
    /// Type of operation being performed
    pub operation_type: OperationType,
    /// When the operation started
    pub started_at: SystemTime,
    /// Current progress count
    pub current: u64,
    /// Total items to process
    pub total: u64,
    /// Current status message
    pub message: String,
    /// Whether the operation was cancelled
    pub is_cancelled: bool,
    /// Whether the operation failed
    pub is_failed: bool,
    /// Whether the operation is waiting
    pub is_waiting: bool,
    /// Estimated time to completion
    pub eta: Option<std::time::Duration>,
}

/// Overall status information (data contract for UI)
#[derive(Debug, Clone)]
pub struct SystemStatus {
    /// Total number of contexts
    pub total_contexts: usize,
    /// Number of persistent contexts
    pub persistent_contexts: usize,
    /// Number of volatile contexts
    pub volatile_contexts: usize,
    /// List of current operations
    pub operations: Vec<OperationStatus>,
    /// Number of active operations
    pub active_count: usize,
    /// Number of waiting operations
    pub waiting_count: usize,
    /// Maximum concurrent operations allowed
    pub max_concurrent: usize,
}

/// Progress information for operations
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// Current progress count
    pub current: u64,
    /// Total items to process
    pub total: u64,
    /// Current status message
    pub message: String,
    /// When progress tracking started
    pub progress_started_at: Option<SystemTime>,
}

impl Default for ProgressInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressInfo {
    /// Create a new progress info instance
    pub fn new() -> Self {
        Self {
            current: 0,
            total: 0,
            message: "Initializing...".to_string(),
            progress_started_at: None,
        }
    }

    /// Update progress information
    pub fn update(&mut self, current: u64, total: u64, message: String) {
        // Start tracking progress time when we first get meaningful progress
        if self.progress_started_at.is_none() && current > 0 && total > 0 {
            self.progress_started_at = Some(SystemTime::now());
        }

        self.current = current;
        self.total = total;
        self.message = message;
    }

    /// Calculate ETA based on current progress rate
    pub fn calculate_eta(&self) -> Option<std::time::Duration> {
        if let Some(started_at) = self.progress_started_at {
            if self.current > 0 && self.total > self.current {
                if let Ok(elapsed) = started_at.elapsed() {
                    let progress_rate = self.current as f64 / elapsed.as_secs_f64();
                    if progress_rate > 0.0 {
                        let remaining_items = self.total - self.current;
                        let eta_seconds = remaining_items as f64 / progress_rate;
                        return Some(std::time::Duration::from_secs_f64(eta_seconds));
                    }
                }
            }
        }
        None
    }
}

/// Background indexing job (internal implementation detail)
#[derive(Debug)]
pub(crate) enum IndexingJob {
    AddDirectory {
        id: Uuid,
        cancel: CancellationToken,
        path: PathBuf,
        name: String,
        description: String,
        persistent: bool,
    },
    Clear {
        id: Uuid,
        cancel: CancellationToken,
    },
}

#[cfg(test)]
mod progress_tests {
    use std::thread;

    use super::*;

    #[test]
    fn test_eta_calculation() {
        let mut progress = ProgressInfo::new();

        // No ETA initially
        assert!(progress.calculate_eta().is_none());

        // Set initial progress
        progress.update(0, 100, "Starting".to_string());
        assert!(progress.calculate_eta().is_none());

        // Simulate some progress after a small delay
        thread::sleep(std::time::Duration::from_millis(10));
        progress.update(25, 100, "25% complete".to_string());

        // Should have an ETA now
        let eta = progress.calculate_eta();
        assert!(eta.is_some());

        // ETA should be reasonable (not zero, not too large)
        if let Some(eta_duration) = eta {
            assert!(eta_duration.as_secs() < 3600); // Less than an hour
        }
    }

    #[test]
    fn test_eta_edge_cases() {
        let mut progress = ProgressInfo::new();

        // Complete progress should have no ETA
        progress.update(100, 100, "Complete".to_string());
        assert!(progress.calculate_eta().is_none());

        // Zero total should have no ETA
        progress.update(50, 0, "Invalid".to_string());
        assert!(progress.calculate_eta().is_none());
    }
}
