use std::borrow::Cow;
use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use super::store::TaskStore;
use super::{
    Task,
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

const TASK_CREATE_DESCRIPTION: &str = r#"
Use this tool to create a structured task list for your current coding session. This helps you
track progress, organize complex tasks, and demonstrate thoroughness to the user.
It also helps the user understand the progress of the task and overall progress of their requests.

## When to Use This Tool

Use this tool proactively in these scenarios:

- Complex multi-step tasks - When a task requires 3 or more distinct steps or actions
- Non-trivial and complex tasks - Tasks that require careful planning or multiple operations
- User explicitly requests todo list - When the user directly asks you to use the todo list
- User provides multiple tasks - When users provide a list of things to be done (numbered or comma-separated)
- After receiving new instructions - Immediately capture user requirements as tasks
- When you start working on a task - Mark it as in_progress BEFORE beginning work
- After completing a task - Mark it as completed and add any new follow-up tasks discovered during implementation

## When NOT to Use This Tool

Skip using this tool when:
- There is only a single, straightforward task
- The task is trivial and tracking it provides no organizational benefit
- The task can be completed in less than 3 trivial steps
- The task is purely conversational or informational

NOTE that you should not use this tool if there is only one trivial task to do. In this case
you are better off just doing the task directly.

## Task Fields

- **subject**: A brief, actionable title in imperative form (e.g., "Fix authentication bug in login flow")
- **description**: Detailed description of what needs to be done, including context and acceptance criteria

All tasks are created with status `pending`.

## Tips

- Create tasks with clear, specific subjects that describe the outcome
- Include enough detail in the description for another agent to understand and complete the task
- After creating tasks, use task_update with add_depends_on to declare dependencies if needed
- Check task_list first to avoid creating duplicate tasks
- Create tasks in logical execution order (task #1 should be done first)
"#;

const TASK_CREATE_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "subject": {
            "type": "string",
            "description": "Brief imperative title for the task (e.g., 'Fix authentication bug in login flow')"
        },
        "description": {
            "type": "string",
            "description": "Detailed description of what needs to be done, including context and acceptance criteria"
        },
        "metadata": {
            "type": "object",
            "description": "Arbitrary metadata to attach to the task",
            "additionalProperties": true
        }
    },
    "required": ["subject", "description"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreate {
    pub subject: String,
    pub description: String,
    #[serde(default)]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl BuiltInToolTrait for TaskCreate {
    fn name() -> BuiltInToolName {
        BuiltInToolName::TaskCreate
    }

    fn description() -> Cow<'static, str> {
        TASK_CREATE_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TASK_CREATE_SCHEMA.into()
    }
}

impl TaskCreate {
    pub fn execute(&self, store: &TaskStore) -> ToolExecutionResult {
        store.cleanup_if_all_completed().map_err(ToolExecutionError::Custom)?;

        let task = Task {
            id: store.allocate_id(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            status: TaskStatus::Pending,
            owner: None,
            depends_on: Vec::new(),
            metadata: self.metadata.clone().unwrap_or_default(),
        };
        store.write_task(&task).map_err(ToolExecutionError::Custom)?;

        let response = serde_json::json!({
            "id": task.id,
            "subject": task.subject,
            "status": task.status,
        });

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]))
    }
}
