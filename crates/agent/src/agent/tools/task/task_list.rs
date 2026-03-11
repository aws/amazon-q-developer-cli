use std::borrow::Cow;

use serde::{
    Deserialize,
    Serialize,
};

use super::TaskStatus;
use super::store::TaskStore;
use crate::agent::tools::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};

const TASK_LIST_DESCRIPTION: &str = r#"
Use this tool to list all tasks in the task list.

## When to Use This Tool

- To see what tasks are available to work on (status: 'pending', no owner, not blocked)
- To check overall progress on the project
- To find tasks that are blocked and need dependencies resolved
- After completing a task, to check for newly unblocked work or claim the next available task
- **Prefer working on tasks in ID order** (lowest ID first) when multiple tasks are available,
  as earlier tasks often set up context for later ones

## Output

Returns a summary of each task:
- **id**: Task identifier (use with task_get, task_update)
- **subject**: Brief description of the task
- **status**: 'pending', 'in_progress', or 'completed'
- **owner**: Agent ID if assigned, empty if available
- **depends_on**: List of open task IDs that must be resolved first
  (tasks with depends_on cannot be claimed until dependencies resolve)

Use task_get with a specific task ID to view full details including description.
"#;

const TASK_LIST_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {},
    "required": []
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskList {}

impl BuiltInToolTrait for TaskList {
    fn name() -> BuiltInToolName {
        BuiltInToolName::TaskList
    }

    fn description() -> Cow<'static, str> {
        TASK_LIST_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TASK_LIST_SCHEMA.into()
    }
}

impl TaskList {
    pub fn execute(&self, store: &TaskStore) -> ToolExecutionResult {
        let summaries = store.list().map_err(ToolExecutionError::Custom)?;

        if !summaries.is_empty() && summaries.iter().all(|s| s.status == TaskStatus::Completed) {
            store.cleanup_if_all_completed().map_err(ToolExecutionError::Custom)?;
            let response = serde_json::json!([]);
            return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]));
        }

        let response = serde_json::to_value(&summaries)
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to serialize: {e}")))?;

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]))
    }
}
