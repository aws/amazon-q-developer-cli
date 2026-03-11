use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

pub mod store;
pub mod task_create;
pub mod task_get;
pub mod task_list;
pub mod task_update;

pub use task_create::TaskCreate;
pub use task_get::TaskGet;
pub use task_list::TaskList;
pub use task_update::TaskUpdateTool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Deleted,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Deleted => write!(f, "deleted"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Lighter representation returned by TaskList.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub subject: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

/// Fields that can be updated on a task. All fields are optional; only provided
/// fields are applied.
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub add_depends_on: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Task {
    pub fn to_summary(&self, open_depends_on: Vec<String>) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            subject: self.subject.clone(),
            status: self.status.clone(),
            owner: self.owner.clone(),
            depends_on: open_depends_on,
        }
    }
}
