use std::borrow::Cow;

use serde::{
    Deserialize,
    Serialize,
};

use super::store::TaskStore;
use crate::agent::tools::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};

const TASK_GET_DESCRIPTION: &str = r#"
Use this tool to retrieve a task by its ID from the task list.

## When to Use This Tool

- When you need the full description and context before starting work on a task
- To understand task dependencies
- After being assigned a task, to get complete requirements

## Output

Returns full task details:
- **subject**: Task title
- **description**: Detailed requirements and context
- **status**: 'pending', 'in_progress', or 'completed'
- **depends_on**: Task IDs that must complete before this one can start

## Tips

- After fetching a task, verify its depends_on list is empty before beginning work.
- Use task_list to see all tasks in summary form.
"#;

const TASK_GET_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "task_id": {
            "type": "string",
            "description": "The ID of the task to retrieve"
        }
    },
    "required": ["task_id"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGet {
    pub task_id: String,
}

impl BuiltInToolTrait for TaskGet {
    fn name() -> BuiltInToolName {
        BuiltInToolName::TaskGet
    }

    fn description() -> Cow<'static, str> {
        TASK_GET_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TASK_GET_SCHEMA.into()
    }
}

impl TaskGet {
    pub fn execute(&self, store: &TaskStore) -> ToolExecutionResult {
        let task = store.get(&self.task_id).map_err(ToolExecutionError::Custom)?;

        let response =
            serde_json::to_value(&task).map_err(|e| ToolExecutionError::Custom(format!("Failed to serialize: {e}")))?;

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]))
    }
}
