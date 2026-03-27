use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

pub(crate) mod project_store;
pub mod store;
pub mod task_tool;

pub use task_tool::TaskTool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Completed,
    Deleted,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Deleted => write!(f, "deleted"),
        }
    }
}

/// Project-level metadata (the overall goal / progress context for the task set).
/// Stored separately in `project_metadata.json` so it is independent of any individual task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Lighter representation returned by list command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub subject: String,
    pub status: TaskStatus,
}

impl Task {
    pub fn to_summary(&self) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            subject: self.subject.clone(),
            status: self.status.clone(),
        }
    }
}
