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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Pending.to_string(), "pending");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Deleted.to_string(), "deleted");
    }

    #[test]
    fn test_task_status_serde() {
        let status = TaskStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""pending""#);
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_task_status_eq() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_ne!(TaskStatus::Pending, TaskStatus::Completed);
    }

    #[test]
    fn test_task_to_summary() {
        let t = Task {
            id: "1".to_string(),
            subject: "Do thing".to_string(),
            description: "Detailed description".to_string(),
            status: TaskStatus::Pending,
            metadata: HashMap::new(),
        };
        let s = t.to_summary();
        assert_eq!(s.id, "1");
        assert_eq!(s.subject, "Do thing");
        assert_eq!(s.status, TaskStatus::Pending);
    }

    #[test]
    fn test_project_metadata_default() {
        let m = ProjectMetadata::default();
        assert!(m.description.is_empty());
        assert!(m.context.is_empty());
        assert!(m.modified_files.is_empty());
    }

    #[test]
    fn test_project_metadata_serde_minimal() {
        let m = ProjectMetadata::default();
        let json = serde_json::to_string(&m).unwrap();
        // Empty vecs are skipped, only description shown
        let parsed: ProjectMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, m.description);
    }

    #[test]
    fn test_project_metadata_serde_full() {
        let m = ProjectMetadata {
            description: "Build app".to_string(),
            context: vec!["c1".to_string(), "c2".to_string()],
            modified_files: vec!["a.rs".to_string()],
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: ProjectMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, m.description);
        assert_eq!(parsed.context, m.context);
        assert_eq!(parsed.modified_files, m.modified_files);
    }

    #[test]
    fn test_task_summary_serde() {
        let s = TaskSummary {
            id: "x".to_string(),
            subject: "Subject".to_string(),
            status: TaskStatus::Completed,
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: TaskSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, s.id);
        assert_eq!(parsed.subject, s.subject);
        assert_eq!(parsed.status, s.status);
    }

    #[test]
    fn test_task_serde() {
        let t = Task {
            id: "1".to_string(),
            subject: "X".to_string(),
            description: "desc".to_string(),
            status: TaskStatus::Pending,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, t.id);
        assert_eq!(parsed.subject, t.subject);
    }
}
