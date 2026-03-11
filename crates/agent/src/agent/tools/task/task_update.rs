use std::borrow::Cow;
use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use super::store::TaskStore;
use super::{
    TaskPatch,
    TaskStatus,
};
use crate::agent::tools::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};

const TASK_UPDATE_DESCRIPTION: &str = r#"
Use this tool to update a task in the task list.

## When to Use This Tool

**Mark tasks as resolved:**
- When you have completed the work described in a task
- When a task is no longer needed or has been superseded
- IMPORTANT: Always mark your assigned tasks as resolved when you finish them
- After resolving, call task_list to find your next task

- ONLY mark a task as completed when you have FULLY accomplished it
- If you encounter errors, blockers, or cannot finish, keep the task as in_progress
- When blocked, create a new task describing what needs to be resolved
- Never mark a task as completed if:
  - Tests are failing
  - Implementation is partial
  - You encountered unresolved errors
  - You couldn't find necessary files or dependencies

**Delete tasks:**
- When a task is no longer relevant or was created in error
- Setting status to `deleted` permanently removes the task

**Update task details:**
- When requirements change or become clearer
- When establishing dependencies between tasks

## Fields You Can Update

- **status**: The task status (see Status Workflow below)
- **subject**: Change the task title (imperative form, e.g., "Run tests")
- **description**: Change the task description
- **owner**: Change the task owner (agent name)
- **metadata**: Merge metadata keys into the task (set a key to null to delete it)
- **add_depends_on**: Declare which tasks must complete before this one can start

## Status Workflow

Status progresses: `pending` → `in_progress` → `completed`

Use `deleted` to permanently remove a task.

## Staleness

Make sure to read a task's latest state using `task_get` before updating it.

## Examples

Mark task as in progress when starting work:
{"task_id": "1", "status": "in_progress"}

Mark task as completed after finishing work:
{"task_id": "1", "status": "completed"}

Set up task dependencies:
{"task_id": "2", "add_depends_on": ["1"]}
"#;

const TASK_UPDATE_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "task_id": {
            "type": "string",
            "description": "The ID of the task to update"
        },
        "status": {
            "type": "string",
            "enum": ["pending", "in_progress", "completed", "deleted"],
            "description": "New status for the task"
        },
        "subject": {
            "type": "string",
            "description": "New subject for the task"
        },
        "description": {
            "type": "string",
            "description": "New description for the task"
        },
        "owner": {
            "type": "string",
            "description": "New owner for the task (agent name)"
        },
        "add_depends_on": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Task IDs that must complete before this task can start (appended to existing list)"
        },
        "metadata": {
            "type": "object",
            "description": "Metadata keys to merge into the task. Set a key to null to delete it.",
            "additionalProperties": true
        }
    },
    "required": ["task_id"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdateTool {
    pub task_id: String,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub add_depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl BuiltInToolTrait for TaskUpdateTool {
    fn name() -> BuiltInToolName {
        BuiltInToolName::TaskUpdate
    }

    fn description() -> Cow<'static, str> {
        TASK_UPDATE_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TASK_UPDATE_SCHEMA.into()
    }
}

impl TaskUpdateTool {
    pub fn execute(&self, store: &TaskStore) -> ToolExecutionResult {
        let patch = TaskPatch {
            status: self.status.clone(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            owner: self.owner.clone(),
            add_depends_on: self.add_depends_on.clone(),
            metadata: self.metadata.clone(),
        };
        let task = store.update(&self.task_id, patch).map_err(ToolExecutionError::Custom)?;

        let mut response =
            serde_json::to_value(&task).map_err(|e| ToolExecutionError::Custom(format!("Failed to serialize: {e}")))?;

        // Warn if task was set to in_progress while it has open blockers
        if task.status == TaskStatus::InProgress && !task.depends_on.is_empty() {
            let open: Vec<&String> = task
                .depends_on
                .iter()
                .filter(|bid| {
                    store
                        .get(bid)
                        .is_ok_and(|t| t.status != TaskStatus::Completed && t.status != TaskStatus::Deleted)
                })
                .collect();
            if !open.is_empty() {
                let ids: Vec<String> = open.iter().map(|id| format!("#{id}")).collect();
                response["warning"] = serde_json::Value::String(format!(
                    "Task depends on {} which are not yet completed",
                    ids.join(", ")
                ));
            }
        }

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]))
    }
}
