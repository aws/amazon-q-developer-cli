use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::{queue, style, style::Color};
use eyre::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{InvokeOutput, OutputKind};

use crate::os::Os;

// Type alias to reduce complexity
type TaskState = Arc<Mutex<HashMap<String, Vec<TaskItem>>>>;

// Global state that persists across tool invocations within a session
pub static TASK_STATE: std::sync::LazyLock<TaskState> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskPriority {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "low")]
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub created_at: String, // Unix timestamp as string
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum TaskTool {
    #[serde(rename = "list")]
    List {
        #[serde(default)]
        filter: Option<TaskStatus>,
    },
    #[serde(rename = "add")]
    Add {
        task: String,
        #[serde(default = "default_priority")]
        priority: TaskPriority,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "add_multiple")]
    AddMultiple {
        tasks: Vec<String>,
        #[serde(default = "default_priority")]
        priority: TaskPriority,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "replace")]
    Replace {
        tasks: Vec<String>,
        #[serde(default = "default_priority")]
        priority: TaskPriority,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "complete")]
    Complete {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "complete_multiple")]
    CompleteMultiple {
        ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "remove")]
    Remove {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "remove_multiple")]
    RemoveMultiple {
        ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    #[serde(rename = "clear")]
    Clear {
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

fn default_priority() -> TaskPriority {
    TaskPriority::Medium
}
impl TaskTool {
    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        match self {
            TaskTool::Add { task, .. } => {
                if task.trim().is_empty() {
                    bail!("Task description cannot be empty");
                }
                if task.len() > 500 {
                    bail!("Task description must be 500 characters or less");
                }
            },
            TaskTool::AddMultiple { tasks, .. } | TaskTool::Replace { tasks, .. } => {
                if tasks.is_empty() {
                    bail!("At least one task must be provided");
                }
                for task in tasks {
                    if task.trim().is_empty() {
                        bail!("Task descriptions cannot be empty");
                    }
                    if task.len() > 500 {
                        bail!("Task descriptions must be 500 characters or less");
                    }
                }
            },
            TaskTool::Complete { id, .. } | TaskTool::Remove { id, .. } => {
                if id.trim().is_empty() {
                    bail!("Task ID must be provided");
                }
            },
            TaskTool::CompleteMultiple { ids, .. } | TaskTool::RemoveMultiple { ids, .. } => {
                if ids.is_empty() {
                    bail!("At least one task ID must be provided");
                }
                for id in ids {
                    if id.trim().is_empty() {
                        bail!("Task IDs cannot be empty");
                    }
                }
            },
            TaskTool::List { .. } | TaskTool::Clear { .. } => {
                // No validation needed
            },
        }
        Ok(())
    }



    pub async fn invoke(&self, os: &Os, stdout: &mut impl Write) -> Result<InvokeOutput> {
        // Use current working directory as session identifier since conversations are tied to directories
        let conversation_id = os
            .env
            .current_dir()
            .map_or_else(|_| "default".to_string(), |p| p.to_string_lossy().to_string());
        let mut state = TASK_STATE.lock().await;

        let result = match self {
            TaskTool::List { filter } => self.handle_list(&mut state, &conversation_id, filter, stdout).await?,
            TaskTool::Add {
                task,
                priority,
                summary,
            } => {
                let msg = if let Some(summary) = summary {
                    format!("Adding task: {}", summary)
                } else {
                    format!("Adding task: {}", task)
                };
                super::queue_function_result(&msg, stdout, false, true)?;
                self.handle_add(&mut state, &conversation_id, task, priority, stdout)
                    .await?
            },
            TaskTool::AddMultiple {
                tasks,
                priority,
                summary,
            } => {
                let msg = if let Some(summary) = summary {
                    format!("Adding {} tasks: {}", tasks.len(), summary)
                } else {
                    format!("Adding {} tasks", tasks.len())
                };
                super::queue_function_result(&msg, stdout, false, true)?;
                self.handle_add_multiple(&mut state, &conversation_id, tasks, priority, stdout)
                    .await?
            },
            TaskTool::Replace {
                tasks,
                priority,
                summary,
            } => {
                let msg = if let Some(summary) = summary {
                    format!("Replacing task list with {} tasks: {}", tasks.len(), summary)
                } else {
                    format!("Replacing task list with {} tasks", tasks.len())
                };
                super::queue_function_result(&msg, stdout, false, true)?;
                self.handle_replace(&mut state, &conversation_id, tasks, priority, stdout)
                    .await?
            },
            TaskTool::Complete { id, summary } => {
                if let Some(summary) = summary {
                    super::queue_function_result(&format!("Completing task: {}", summary), stdout, false, true)?;
                }
                self.handle_complete(&mut state, &conversation_id, id, stdout).await?
            },
            TaskTool::CompleteMultiple { ids, summary } => {
                let msg = if let Some(summary) = summary {
                    format!("Completing {} tasks: {}", ids.len(), summary)
                } else {
                    format!("Completing {} tasks", ids.len())
                };
                super::queue_function_result(&msg, stdout, false, true)?;
                self.handle_complete_multiple(&mut state, &conversation_id, ids, stdout)
                    .await?
            },
            TaskTool::Remove { id, summary } => {
                if let Some(summary) = summary {
                    super::queue_function_result(&format!("Removing task: {}", summary), stdout, false, true)?;
                }
                self.handle_remove(&mut state, &conversation_id, id, stdout).await?
            },
            TaskTool::RemoveMultiple { ids, summary } => {
                let msg = if let Some(summary) = summary {
                    format!("Removing {} tasks: {}", ids.len(), summary)
                } else {
                    format!("Removing {} tasks", ids.len())
                };
                super::queue_function_result(&msg, stdout, false, true)?;
                self.handle_remove_multiple(&mut state, &conversation_id, ids, stdout)
                    .await?
            },
            TaskTool::Clear { summary } => {
                if let Some(summary) = summary {
                    super::queue_function_result(&format!("Clearing tasks: {}", summary), stdout, false, true)?;
                }
                self.handle_clear(&mut state, &conversation_id, stdout).await?
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }
    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        match self {
            TaskTool::List { filter } => {
                queue!(output, style::Print("Listing task items"))?;
                if let Some(filter) = filter {
                    queue!(
                        output,
                        style::Print(" ("),
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("{:?}", filter)),
                        style::ResetColor,
                        style::Print(" only)")
                    )?;
                }
            },
            TaskTool::Add { task, priority, .. } => {
                queue!(
                    output,
                    style::Print("Adding task: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(task),
                    style::ResetColor,
                    style::Print(" ["),
                    style::SetForegroundColor(Color::Blue),
                    style::Print(format!("{:?}", priority)),
                    style::ResetColor,
                    style::Print("]")
                )?;
            },
            TaskTool::AddMultiple { tasks, priority, .. } => {
                queue!(
                    output,
                    style::Print("Adding "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("{}", tasks.len())),
                    style::ResetColor,
                    style::Print(" tasks ["),
                    style::SetForegroundColor(Color::Blue),
                    style::Print(format!("{:?}", priority)),
                    style::ResetColor,
                    style::Print("]")
                )?;
            },
            TaskTool::Replace { tasks, priority, .. } => {
                queue!(
                    output,
                    style::Print("Replacing task list with "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("{}", tasks.len())),
                    style::ResetColor,
                    style::Print(" tasks ["),
                    style::SetForegroundColor(Color::Blue),
                    style::Print(format!("{:?}", priority)),
                    style::ResetColor,
                    style::Print("]")
                )?;
            },
            TaskTool::Complete { id, .. } => {
                queue!(
                    output,
                    style::Print("Completing task: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(id),
                    style::ResetColor
                )?;
            },
            TaskTool::CompleteMultiple { ids, .. } => {
                queue!(
                    output,
                    style::Print("Completing "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("{}", ids.len())),
                    style::ResetColor,
                    style::Print(" tasks")
                )?;
            },
            TaskTool::Remove { id, .. } => {
                queue!(
                    output,
                    style::Print("Removing task: "),
                    style::SetForegroundColor(Color::Red),
                    style::Print(id),
                    style::ResetColor
                )?;
            },
            TaskTool::RemoveMultiple { ids, .. } => {
                queue!(
                    output,
                    style::Print("Removing "),
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("{}", ids.len())),
                    style::ResetColor,
                    style::Print(" tasks")
                )?;
            },
            TaskTool::Clear { .. } => {
                queue!(
                    output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Clearing all tasks"),
                    style::ResetColor
                )?;
            },
        }
        Ok(())
    }
    // Implementation methods for each operation
    async fn handle_list(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        filter: &Option<TaskStatus>,
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        if tasks.is_empty() {
            super::queue_function_result("No task items found", updates, false, true)?;
            return Ok("No task items in the current session".to_string());
        }

        let mut filtered_tasks: Vec<&TaskItem> = match filter {
            Some(status) => tasks.iter().filter(|item| &item.status == status).collect(),
            None => tasks.iter().collect(),
        };

        if filtered_tasks.is_empty() {
            let msg = match filter {
                Some(status) => format!("No {:?} task items found", status),
                None => "No task items found".to_string(),
            };
            super::queue_function_result(&msg, updates, false, true)?;
            return Ok(msg);
        }

        // Sort by priority: High → Medium → Low
        filtered_tasks.sort_by(|a, b| match (&a.priority, &b.priority) {
            (TaskPriority::High, TaskPriority::High) => std::cmp::Ordering::Equal,
            (TaskPriority::High, _) => std::cmp::Ordering::Less,
            (_, TaskPriority::High) => std::cmp::Ordering::Greater,
            (TaskPriority::Medium, TaskPriority::Medium) => std::cmp::Ordering::Equal,
            (TaskPriority::Medium, TaskPriority::Low) => std::cmp::Ordering::Less,
            (TaskPriority::Low, TaskPriority::Medium) => std::cmp::Ordering::Greater,
            (TaskPriority::Low, TaskPriority::Low) => std::cmp::Ordering::Equal,
        });

        let mut result = format!("Task List ({} items):\n", filtered_tasks.len());
        for (index, item) in filtered_tasks.iter().enumerate() {
            let status_icon = match item.status {
                TaskStatus::Pending => "○",
                TaskStatus::Completed => "✓",
            };

            result.push_str(&format!("{}. {} {}\n", index + 1, status_icon, item.description));
        }

        super::queue_function_result(&result, updates, false, false)?;
        Ok(result)
    }

    async fn handle_add(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        task: &str,
        priority: &TaskPriority,
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        let new_item = TaskItem {
            id: generate_short_id(),
            description: task.to_string(),
            status: TaskStatus::Pending,
            priority: priority.clone(),
            created_at: generate_timestamp(),
        };

        tasks.push(new_item.clone());

        let msg = format!(
            "Added task: {} (Priority: {:?})",
            new_item.description, new_item.priority
        );

        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }

    async fn handle_add_multiple(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        tasks: &[String],
        priority: &TaskPriority,
        updates: &mut impl Write,
    ) -> Result<String> {
        let task_list = state.entry(conversation_id.to_string()).or_default();

        let mut added_items = Vec::new();
        for task in tasks {
            let new_item = TaskItem {
                id: generate_short_id(),
                description: task.clone(),
                status: TaskStatus::Pending,
                priority: priority.clone(),
                created_at: generate_timestamp(),
            };
            task_list.push(new_item.clone());
            added_items.push(new_item);
        }

        let mut msg = format!("Added {} tasks:\n", added_items.len());
        for item in &added_items {
            msg.push_str(&format!("  {} (Priority: {:?})\n", item.description, item.priority));
        }

        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }

    async fn handle_replace(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        tasks: &[String],
        priority: &TaskPriority,
        updates: &mut impl Write,
    ) -> Result<String> {
        let task_list = state.entry(conversation_id.to_string()).or_default();
        
        // Clear existing tasks
        let old_count = task_list.len();
        task_list.clear();

        // Add new tasks
        let mut new_items = Vec::new();
        for task in tasks {
            let new_item = TaskItem {
                id: generate_short_id(),
                description: task.clone(),
                status: TaskStatus::Pending,
                priority: priority.clone(),
                created_at: generate_timestamp(),
            };
            task_list.push(new_item.clone());
            new_items.push(new_item);
        }

        let mut msg = format!("Replaced {} tasks with {} new tasks:\n", old_count, new_items.len());
        for item in &new_items {
            msg.push_str(&format!("  {} (Priority: {:?})\n", item.description, item.priority));
        }

        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }
    async fn handle_complete(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        id: &str,
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        // Parse the ID as a 1-based index
        let index = match id.parse::<usize>() {
            Ok(i) if i > 0 && i <= tasks.len() => i - 1, // Convert to 0-based index
            _ => {
                let msg = format!("Task with ID '{}' not found", id);
                super::queue_function_result(&msg, updates, true, false)?;
                bail!(msg);
            },
        };

        let item = &mut tasks[index];
        if item.status == TaskStatus::Completed {
            let msg = format!("Task is already completed: {}", item.description);
            super::queue_function_result(&msg, updates, false, true)?;
            return Ok(msg);
        }

        item.status = TaskStatus::Completed;
        let msg = format!("✓ Completed task: {}", item.description);
        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }

    async fn handle_complete_multiple(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        ids: &[String],
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        let mut completed_items = Vec::new();
        let mut not_found_ids = Vec::new();
        let mut already_completed_items = Vec::new();

        for id in ids {
            // Parse the ID as a 1-based index
            match id.parse::<usize>() {
                Ok(i) if i > 0 && i <= tasks.len() => {
                    let index = i - 1; // Convert to 0-based index
                    let item = &mut tasks[index];
                    if item.status == TaskStatus::Completed {
                        already_completed_items.push(item.description.clone());
                    } else {
                        item.status = TaskStatus::Completed;
                        completed_items.push(item.description.clone());
                    }
                },
                _ => {
                    not_found_ids.push(id.clone());
                },
            }
        }

        let mut result_msg = String::new();

        if !completed_items.is_empty() {
            result_msg.push_str(&format!("✓ Completed {} tasks:\n", completed_items.len()));
            for description in &completed_items {
                result_msg.push_str(&format!("  {}\n", description));
            }
        }

        if !already_completed_items.is_empty() {
            result_msg.push_str(&format!(
                "Already completed ({} tasks):\n",
                already_completed_items.len()
            ));
            for description in &already_completed_items {
                result_msg.push_str(&format!("  {}\n", description));
            }
        }

        if !not_found_ids.is_empty() {
            result_msg.push_str(&format!(
                "Not found ({} task IDs): {}\n",
                not_found_ids.len(),
                not_found_ids.join(", ")
            ));
        }

        let success = !completed_items.is_empty();
        super::queue_function_result(&result_msg, updates, !success, false)?;

        if not_found_ids.is_empty() {
            Ok(result_msg)
        } else {
            bail!("Some task items were not found: {}", not_found_ids.join(", "));
        }
    }

    async fn handle_remove(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        id: &str,
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        // Parse the ID as a 1-based index
        let index = match id.parse::<usize>() {
            Ok(i) if i > 0 && i <= tasks.len() => i - 1, // Convert to 0-based index
            _ => {
                let msg = format!("Task with ID '{}' not found", id);
                super::queue_function_result(&msg, updates, true, false)?;
                bail!(msg);
            },
        };

        let removed_item = tasks.remove(index);
        let msg = format!("Removed task: {}", removed_item.description);
        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }

    async fn handle_remove_multiple(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        ids: &[String],
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();

        // Parse and validate all IDs first, collect indices in descending order
        let mut indices_to_remove = Vec::new();
        let mut not_found_ids = Vec::new();
        let mut removed_descriptions = Vec::new();

        for id in ids {
            match id.parse::<usize>() {
                Ok(i) if i > 0 && i <= tasks.len() => {
                    indices_to_remove.push((i - 1, tasks[i - 1].description.clone())); // Convert to 0-based index
                },
                _ => {
                    not_found_ids.push(id.clone());
                },
            }
        }

        if !not_found_ids.is_empty() {
            let msg = format!("Task IDs not found: {}", not_found_ids.join(", "));
            super::queue_function_result(&msg, updates, true, false)?;
            bail!(msg);
        }

        // Sort indices in descending order to avoid index shifting issues when removing
        indices_to_remove.sort_by(|a, b| b.0.cmp(&a.0));
        
        // Remove tasks from highest index to lowest
        for (index, description) in indices_to_remove {
            tasks.remove(index);
            removed_descriptions.push(description);
        }

        // Reverse to show in original order
        removed_descriptions.reverse();

        let mut msg = format!("Removed {} tasks:\n", removed_descriptions.len());
        for description in &removed_descriptions {
            msg.push_str(&format!("  {}\n", description));
        }

        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }

    /// Auto-clear tasks when context switches (e.g., user moves to new topic)
    /// This should be called by the LLM when it detects a significant context change
    #[cfg(test)]
    pub async fn auto_clear_on_context_switch(&self, os: &Os, updates: &mut impl Write) -> Result<bool> {
        let conversation_id = os
            .env
            .current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "default".to_string());
        let mut state = TASK_STATE.lock().await;
        let tasks = state.entry(conversation_id.to_string()).or_default();

        if !tasks.is_empty() {
            let count = tasks.len();
            tasks.clear();
            let msg = format!("Auto-cleared {} tasks due to context switch", count);
            super::queue_function_result(&msg, updates, false, true)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if there are any pending tasks that might need clearing
    #[cfg(test)]
    pub async fn has_pending_tasks(&self, os: &Os) -> Result<bool> {
        let conversation_id = os
            .env
            .current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "default".to_string());
        let state = TASK_STATE.lock().await;
        if let Some(tasks) = state.get(&conversation_id) {
            Ok(tasks.iter().any(|task| task.status == TaskStatus::Pending))
        } else {
            Ok(false)
        }
    }

    async fn handle_clear(
        &self,
        state: &mut HashMap<String, Vec<TaskItem>>,
        conversation_id: &str,
        updates: &mut impl Write,
    ) -> Result<String> {
        let tasks = state.entry(conversation_id.to_string()).or_default();
        let count = tasks.len();
        tasks.clear();

        let msg = format!("Cleared {} tasks", count);
        super::queue_function_result(&msg, updates, false, false)?;
        Ok(msg)
    }
}

fn generate_short_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn generate_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_add_validation() {
        let os = Os::new().await.unwrap();

        // Valid task
        let mut valid_task = TaskTool::Add {
            task: "Test task".to_string(),
            priority: TaskPriority::Medium,
            summary: None,
        };
        assert!(valid_task.validate(&os).await.is_ok());

        // Empty task
        let mut empty_task = TaskTool::Add {
            task: "".to_string(),
            priority: TaskPriority::Low,
            summary: None,
        };
        let result = empty_task.validate(&os).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_task_operations() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Test add
        let add_task = TaskTool::Add {
            task: "Test task".to_string(),
            priority: TaskPriority::High,
            summary: None,
        };
        let result = add_task.invoke(&os, &mut output).await.unwrap();
        assert!(result.as_str().contains("Added task"));

        // Test list
        let list_task = TaskTool::List { filter: None };
        let result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(result.as_str().contains("Task List"));
        assert!(result.as_str().contains("Test task"));
    }

    #[test]
    fn test_task_deserialization() {
        // Test add command
        let add_json = serde_json::json!({
            "command": "add",
            "task": "Test task",
            "priority": "high"
        });
        let add_task = serde_json::from_value::<TaskTool>(add_json).unwrap();
        assert!(matches!(add_task, TaskTool::Add { .. }));

        // Test list command
        let list_json = serde_json::json!({
            "command": "list",
            "filter": "pending"
        });
        let list_task = serde_json::from_value::<TaskTool>(list_json).unwrap();
        assert!(matches!(list_task, TaskTool::List { .. }));
    }

    #[tokio::test]
    async fn test_complete_workflow() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first to ensure test isolation
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add a task
        let add_task = TaskTool::Add {
            task: "Complete this task".to_string(),
            priority: TaskPriority::High,
            summary: None,
        };
        let add_result = add_task.invoke(&os, &mut output).await.unwrap();
        assert!(add_result.as_str().contains("Added task"));

        // List tasks to get the ID
        let list_task = TaskTool::List { filter: None };
        let list_result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("Complete this task"));

        // Complete the task using 1-based index (first task = "1")
        let complete_task = TaskTool::Complete {
            id: "1".to_string(),
            summary: None,
        };
        let complete_result = complete_task.invoke(&os, &mut output).await.unwrap();
        assert!(complete_result.as_str().contains("Completed task"));

        // Verify task is completed
        let final_list = TaskTool::List { filter: None };
        let final_result = final_list.invoke(&os, &mut output).await.unwrap();
        assert!(final_result.as_str().contains("✓"));
    }

    #[tokio::test]
    async fn test_state_isolation() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Add two different tasks
        let add_task1 = TaskTool::Add {
            task: "First unique task".to_string(),
            priority: TaskPriority::Medium,
            summary: None,
        };
        add_task1.invoke(&os, &mut output).await.unwrap();

        let add_task2 = TaskTool::Add {
            task: "Second unique task".to_string(),
            priority: TaskPriority::Low,
            summary: None,
        };
        add_task2.invoke(&os, &mut output).await.unwrap();

        // List tasks - should contain both since they're in the same session
        let list_task = TaskTool::List { filter: None };
        let result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(result.as_str().contains("First unique task"));
        assert!(result.as_str().contains("Second unique task"));
    }

    #[tokio::test]
    async fn test_add_multiple() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add multiple tasks at once
        let add_multiple_task = TaskTool::AddMultiple {
            tasks: vec!["Task one".to_string(), "Task two".to_string(), "Task three".to_string()],
            priority: TaskPriority::High,
            summary: Some("Adding test tasks".to_string()),
        };
        let result = add_multiple_task.invoke(&os, &mut output).await.unwrap();
        assert!(result.as_str().contains("Added 3 tasks"));
        assert!(result.as_str().contains("Task one"));
        assert!(result.as_str().contains("Task two"));
        assert!(result.as_str().contains("Task three"));

        // List tasks to verify they were all added
        let list_task = TaskTool::List { filter: None };
        let list_result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("Task List (3 items)"));
    }

    #[test]
    fn test_add_multiple_deserialization() {
        let json = serde_json::json!({
            "command": "add_multiple",
            "tasks": ["First task", "Second task"],
            "priority": "high",
            "summary": "Bulk adding tasks"
        });
        let add_multiple_task = serde_json::from_value::<TaskTool>(json).unwrap();
        assert!(matches!(add_multiple_task, TaskTool::AddMultiple { .. }));
    }

    #[tokio::test]
    async fn test_complete_multiple() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add some tasks to complete
        let add_multiple_task = TaskTool::AddMultiple {
            tasks: vec![
                "Task to complete 1".to_string(),
                "Task to complete 2".to_string(),
                "Task to complete 3".to_string(),
            ],
            priority: TaskPriority::Medium,
            summary: None,
        };
        let _ = add_multiple_task.invoke(&os, &mut output).await;

        // Verify tasks were added by invoking list (but we don't need the result)
        let list_task = TaskTool::List { filter: None };
        let _ = list_task.invoke(&os, &mut output).await.unwrap();

        // Complete multiple tasks using 1-based indices (tasks 1, 2, 3)
        let complete_multiple_task = TaskTool::CompleteMultiple {
            ids: vec!["1".to_string(), "2".to_string(), "3".to_string()],
            summary: Some("Completing test tasks".to_string()),
        };
        let complete_result = complete_multiple_task.invoke(&os, &mut output).await.unwrap();
        assert!(complete_result.as_str().contains("Completed 3 tasks"));
        assert!(complete_result.as_str().contains("Task to complete 1"));
        assert!(complete_result.as_str().contains("Task to complete 2"));
        assert!(complete_result.as_str().contains("Task to complete 3"));

        // Verify tasks are completed by listing completed items
        let completed_list = TaskTool::List {
            filter: Some(TaskStatus::Completed),
        };
        let completed_result = completed_list.invoke(&os, &mut output).await.unwrap();
        assert!(completed_result.as_str().contains("Task List (3 items)"));
        assert!(completed_result.as_str().contains("✓"));
    }

    #[tokio::test]
    async fn test_remove_functionality() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add a task to remove
        let add_task = TaskTool::Add {
            task: "Task to be removed".to_string(),
            priority: TaskPriority::Medium,
            summary: None,
        };
        let _ = add_task.invoke(&os, &mut output).await;

        // Remove the task using 1-based index (first task = "1")
        let remove_task = TaskTool::Remove {
            id: "1".to_string(),
            summary: None,
        };
        let remove_result = remove_task.invoke(&os, &mut output).await.unwrap();
        assert!(remove_result.as_str().contains("Removed task: Task to be removed"));

        // Verify task is removed by listing
        let list_task = TaskTool::List { filter: None };
        let list_result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("No task items in the current session"));
    }

    #[tokio::test]
    async fn test_auto_clear_functionality() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add some tasks
        let add_task = TaskTool::Add {
            task: "Task 1".to_string(),
            priority: TaskPriority::Medium,
            summary: None,
        };
        let _ = add_task.invoke(&os, &mut output).await;

        let add_task2 = TaskTool::Add {
            task: "Task 2".to_string(),
            priority: TaskPriority::High,
            summary: None,
        };
        let _ = add_task2.invoke(&os, &mut output).await;

        // Check that we have pending tasks
        let task_tool = TaskTool::List { filter: None };
        let has_pending = task_tool.has_pending_tasks(&os).await.unwrap();
        assert!(has_pending);

        // Auto-clear tasks
        let cleared = task_tool.auto_clear_on_context_switch(&os, &mut output).await.unwrap();
        assert!(cleared);

        // Verify tasks are cleared
        let has_pending_after = task_tool.has_pending_tasks(&os).await.unwrap();
        assert!(!has_pending_after);

        // Verify list is empty
        let list_result = task_tool.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("No task items in the current session"));
    }

    #[test]
    fn test_complete_multiple_deserialization() {
        let json = serde_json::json!({
            "command": "complete_multiple",
            "ids": ["abc123", "def456", "ghi789"],
            "summary": "Bulk completing tasks"
        });
        let complete_multiple_task = serde_json::from_value::<TaskTool>(json).unwrap();
        assert!(matches!(complete_multiple_task, TaskTool::CompleteMultiple { .. }));
    }

    #[tokio::test]
    async fn test_replace_functionality() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add some initial tasks
        let add_task = TaskTool::AddMultiple {
            tasks: vec!["Old task 1".to_string(), "Old task 2".to_string()],
            priority: TaskPriority::Medium,
            summary: None,
        };
        let _ = add_task.invoke(&os, &mut output).await;

        // Replace with new tasks
        let replace_task = TaskTool::Replace {
            tasks: vec!["New task A".to_string(), "New task B".to_string(), "New task C".to_string()],
            priority: TaskPriority::High,
            summary: Some("Replacing with new tasks".to_string()),
        };
        let replace_result = replace_task.invoke(&os, &mut output).await.unwrap();
        assert!(replace_result.as_str().contains("Replaced 2 tasks with 3 new tasks"));
        assert!(replace_result.as_str().contains("New task A"));
        assert!(replace_result.as_str().contains("New task B"));
        assert!(replace_result.as_str().contains("New task C"));

        // Verify old tasks are gone and new tasks exist
        let list_task = TaskTool::List { filter: None };
        let list_result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("Task List (3 items)"));
        assert!(list_result.as_str().contains("New task A"));
        assert!(!list_result.as_str().contains("Old task 1"));
    }

    #[tokio::test]
    async fn test_remove_multiple_functionality() {
        use std::io::Cursor;

        let os = Os::new().await.unwrap();
        let mut output = Cursor::new(Vec::new());

        // Clear any existing state first
        let clear_task = TaskTool::Clear { summary: None };
        let _ = clear_task.invoke(&os, &mut output).await;

        // Add some tasks to remove
        let add_task = TaskTool::AddMultiple {
            tasks: vec![
                "Task to remove 1".to_string(),
                "Task to keep".to_string(),
                "Task to remove 2".to_string(),
                "Another task to keep".to_string(),
                "Task to remove 3".to_string(),
            ],
            priority: TaskPriority::Medium,
            summary: None,
        };
        let _ = add_task.invoke(&os, &mut output).await;

        // Remove multiple tasks (1st, 3rd, and 5th tasks)
        let remove_multiple_task = TaskTool::RemoveMultiple {
            ids: vec!["1".to_string(), "3".to_string(), "5".to_string()],
            summary: Some("Removing specific tasks".to_string()),
        };
        let remove_result = remove_multiple_task.invoke(&os, &mut output).await.unwrap();
        assert!(remove_result.as_str().contains("Removed 3 tasks"));
        assert!(remove_result.as_str().contains("Task to remove 1"));
        assert!(remove_result.as_str().contains("Task to remove 2"));
        assert!(remove_result.as_str().contains("Task to remove 3"));

        // Verify only the "keep" tasks remain
        let list_task = TaskTool::List { filter: None };
        let list_result = list_task.invoke(&os, &mut output).await.unwrap();
        assert!(list_result.as_str().contains("Task List (2 items)"));
        assert!(list_result.as_str().contains("Task to keep"));
        assert!(list_result.as_str().contains("Another task to keep"));
        assert!(!list_result.as_str().contains("Task to remove"));
    }

    #[test]
    fn test_replace_deserialization() {
        let json = serde_json::json!({
            "command": "replace",
            "tasks": ["New task 1", "New task 2"],
            "priority": "high",
            "summary": "Replacing all tasks"
        });
        let replace_task = serde_json::from_value::<TaskTool>(json).unwrap();
        assert!(matches!(replace_task, TaskTool::Replace { .. }));
    }

    #[test]
    fn test_remove_multiple_deserialization() {
        let json = serde_json::json!({
            "command": "remove_multiple",
            "ids": ["1", "3", "5"],
            "summary": "Removing multiple tasks"
        });
        let remove_multiple_task = serde_json::from_value::<TaskTool>(json).unwrap();
        assert!(matches!(remove_multiple_task, TaskTool::RemoveMultiple { .. }));
    }
}
