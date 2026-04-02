use std::borrow::Cow;
use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use super::store::TaskStore;
use super::{
    ProjectMetadata,
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

const TASK_TOOL_DESCRIPTION: &str = r#"A tool for creating a task list and keeping track of tasks. This tool should be requested EVERY time the user gives you a task that will take multiple steps. A task list should be made BEFORE executing any steps. Steps should be marked off AS YOU COMPLETE THEM. DO NOT display your own tasks or task list AT ANY POINT; this is done for you. Complete the tasks in the same order that you provide them. If the user tells you to skip a step, DO NOT mark it as completed."#;

const TASK_TOOL_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "enum": ["create", "complete", "add", "remove", "list"],
            "description": "The command to run. Allowed options are `create`, `complete`, `add`, `remove`, and `list`. Call `list` without arguments to see a list of all current tasks."
        },
        "tasks": {
            "description": "Required parameter of `create` command containing the list of DISTINCT tasks to be added to the task list.",
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "task_description": {
                        "type": "string",
                        "description": "The main task description"
                    },
                    "details": {
                        "type": "string",
                        "description": "Optional detailed information about the task"
                    }
                },
                "required": ["task_description"]
            }
        },
        "task_list_description": {
            "description": "Required parameter of `create` command containing a BRIEF summary of the task list being created. The summary should be detailed enough to refer to without knowing the problem context beforehand.",
            "type": "string"
        },
        "completed_task_ids": {
            "description": "Required parameter of `complete` command containing the IDs of EVERY completed task. Each task should be marked as completed IMMEDIATELY after it is finished.",
            "type": "array",
            "items": { "type": "string" }
        },
        "context_update": {
            "description": "Required parameter of `complete` command containing important task context. Use this command to track important information about the task AND information about files you have read.",
            "type": "string"
        },
        "modified_files": {
            "description": "Optional parameter of `complete` command containing a list of paths of files that were modified during the task. This is useful for tracking file changes that are important to the task.",
            "type": "array",
            "items": { "type": "string" }
        },
        "new_tasks": {
            "description": "Required parameter of `add` command containing a list of new tasks to be added to the task list.",
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "task_description": {
                        "type": "string",
                        "description": "The main task description"
                    },
                    "details": {
                        "type": "string",
                        "description": "Optional detailed information about the task"
                    }
                },
                "required": ["task_description"]
            }
        },
        "new_description": {
            "description": "Optional parameter of `add` and `remove` containing a new task list description. Use this when the updated set of tasks significantly change the goal or overall procedure of the task list.",
            "type": "string"
        },
        "remove_task_ids": {
            "description": "Required parameter of `remove` command containing the IDs of tasks to remove.",
            "type": "array",
            "items": { "type": "string" }
        }
    },
    "required": ["command"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInput {
    pub task_description: String,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum TaskTool {
    Create {
        tasks: Vec<TaskInput>,
        task_list_description: String,
    },
    Complete {
        completed_task_ids: Vec<String>,
        context_update: String,
        #[serde(default)]
        modified_files: Option<Vec<String>>,
    },
    Add {
        new_tasks: Vec<TaskInput>,
        #[serde(default)]
        new_description: Option<String>,
    },
    Remove {
        remove_task_ids: Vec<String>,
        #[serde(default)]
        new_description: Option<String>,
    },
    List {},
}

impl BuiltInToolTrait for TaskTool {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Task
    }

    fn description() -> Cow<'static, str> {
        TASK_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TASK_TOOL_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["todo_list", "todo"])
    }
}

impl TaskTool {
    pub fn execute(&self, store: &TaskStore) -> ToolExecutionResult {
        match self {
            Self::Create {
                tasks,
                task_list_description,
            } => Self::execute_create(store, tasks, task_list_description),
            Self::Complete {
                completed_task_ids,
                context_update,
                modified_files,
            } => Self::execute_complete(store, completed_task_ids, context_update, modified_files.as_deref()),
            Self::Add {
                new_tasks,
                new_description,
            } => Self::execute_add(store, new_tasks, new_description.as_deref()),
            Self::Remove {
                remove_task_ids,
                new_description,
            } => Self::execute_remove(store, remove_task_ids, new_description.as_deref()),
            Self::List {} => Self::format_full_state(store),
        }
    }

    fn execute_create(store: &TaskStore, tasks: &[TaskInput], task_list_description: &str) -> ToolExecutionResult {
        if tasks.is_empty() {
            return Err(ToolExecutionError::Custom("No tasks were provided".to_string()));
        }
        if tasks.iter().any(|t| t.task_description.trim().is_empty()) {
            return Err(ToolExecutionError::Custom("Tasks cannot be empty".to_string()));
        }
        if task_list_description.trim().is_empty() {
            return Err(ToolExecutionError::Custom(
                "No task description was provided".to_string(),
            ));
        }

        store.cleanup_all().map_err(ToolExecutionError::Custom)?;

        for input in tasks {
            let task = Task {
                id: store.allocate_id(),
                subject: input.task_description.clone(),
                description: input.details.clone().unwrap_or_default(),
                status: TaskStatus::Pending,
                metadata: HashMap::new(),
            };
            store.write_task(&task).map_err(ToolExecutionError::Custom)?;
        }

        store
            .project
            .write(&ProjectMetadata {
                description: task_list_description.to_string(),
                ..Default::default()
            })
            .map_err(ToolExecutionError::Custom)?;

        Self::format_full_state(store)
    }

    fn execute_complete(
        store: &TaskStore,
        completed_task_ids: &[String],
        context_update: &str,
        modified_files: Option<&[String]>,
    ) -> ToolExecutionResult {
        if completed_task_ids.is_empty() {
            return Err(ToolExecutionError::Custom(
                "At least one completed task ID must be provided".to_string(),
            ));
        }
        if context_update.trim().is_empty() {
            return Err(ToolExecutionError::Custom("No context update was provided".to_string()));
        }

        for id in completed_task_ids {
            store
                .update_status(id, TaskStatus::Completed)
                .map_err(ToolExecutionError::Custom)?;
        }

        store
            .project
            .append(context_update, modified_files)
            .map_err(ToolExecutionError::Custom)?;

        store.cleanup_if_all_completed().map_err(ToolExecutionError::Custom)?;

        Self::format_full_state(store)
    }

    fn execute_add(store: &TaskStore, new_tasks: &[TaskInput], new_description: Option<&str>) -> ToolExecutionResult {
        if new_tasks.is_empty() {
            return Err(ToolExecutionError::Custom("No tasks were provided".to_string()));
        }
        if new_tasks.iter().any(|t| t.task_description.trim().is_empty()) {
            return Err(ToolExecutionError::Custom("New tasks cannot be empty".to_string()));
        }
        if new_description.is_some_and(|d| d.trim().is_empty()) {
            return Err(ToolExecutionError::Custom(
                "New description cannot be empty".to_string(),
            ));
        }

        for input in new_tasks {
            let task = Task {
                id: store.allocate_id(),
                subject: input.task_description.clone(),
                description: input.details.clone().unwrap_or_default(),
                status: TaskStatus::Pending,
                metadata: HashMap::new(),
            };
            store.write_task(&task).map_err(ToolExecutionError::Custom)?;
        }

        if let Some(desc) = new_description {
            let mut meta = store.project.read();
            meta.description = desc.to_string();
            store.project.write(&meta).map_err(ToolExecutionError::Custom)?;
        }

        Self::format_full_state(store)
    }

    fn execute_remove(
        store: &TaskStore,
        remove_task_ids: &[String],
        new_description: Option<&str>,
    ) -> ToolExecutionResult {
        if remove_task_ids.is_empty() {
            return Err(ToolExecutionError::Custom(
                "At least one task ID must be provided".to_string(),
            ));
        }

        for id in remove_task_ids {
            store
                .update_status(id, TaskStatus::Deleted)
                .map_err(ToolExecutionError::Custom)?;
        }

        if let Some(desc) = new_description {
            let mut meta = store.project.read();
            meta.description = desc.to_string();
            store.project.write(&meta).map_err(ToolExecutionError::Custom)?;
        }

        Self::format_full_state(store)
    }

    /// Return full task state after every command, echoing current plan state back to the LLM.
    fn format_full_state(store: &TaskStore) -> ToolExecutionResult {
        let summaries = store.list().map_err(ToolExecutionError::Custom)?;

        let list_meta = store.project.read();

        let tasks_json: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "task_description": s.subject,
                    "completed": s.status == TaskStatus::Completed,
                })
            })
            .collect();

        let response = serde_json::json!({
            "tasks": tasks_json,
            "description": list_meta.description,
            "context": list_meta.context,
            "modified_files": list_meta.modified_files,
        });

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(response)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::task::store::TaskStore;

    fn temp_store() -> (tempfile::TempDir, TaskStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    // -----------------------------------------------------------------------
    // JSON-driven test runner
    // -----------------------------------------------------------------------

    /// Snapshot of on-disk state, read once per step.
    struct DiskState {
        task_count: usize,
        total_file_count: usize,
        /// Each task as (id, subject, is_completed)
        tasks: Vec<(String, String, bool)>,
        metadata_description: String,
        metadata_context: Vec<String>,
        metadata_modified_files: Vec<String>,
    }

    fn read_disk(store: &TaskStore) -> DiskState {
        let mut tasks = Vec::new();
        let mut total_file_count = 0;
        let mut metadata_description = String::new();
        let mut metadata_context = Vec::new();
        let mut metadata_modified_files = Vec::new();
        if store.dir.exists() {
            for entry in std::fs::read_dir(&store.dir).unwrap().filter_map(|e| e.ok()) {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.ends_with(".json") {
                    continue;
                }
                total_file_count += 1;
                let data = std::fs::read_to_string(entry.path()).unwrap();
                let v: serde_json::Value = serde_json::from_str(&data).unwrap();
                if name == "project_metadata.json" {
                    metadata_description = v["description"].as_str().unwrap_or("").to_string();
                    metadata_context = v
                        .get("context")
                        .and_then(|a| a.as_array())
                        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
                        .unwrap_or_default();
                    metadata_modified_files = v
                        .get("modified_files")
                        .and_then(|a| a.as_array())
                        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
                        .unwrap_or_default();
                    continue;
                }
                tasks.push((
                    v["id"].as_str().unwrap().to_string(),
                    v["subject"].as_str().unwrap().to_string(),
                    v["status"].as_str().unwrap() == "completed",
                ));
            }
        }
        // format_context is the function under test — no way to avoid calling it
        DiskState {
            task_count: tasks.len(),
            total_file_count,
            tasks,
            metadata_description,
            metadata_context,
            metadata_modified_files,
        }
    }

    #[derive(Deserialize)]
    struct JsonTestCase {
        name: String,
        #[allow(dead_code)]
        description: String,
        steps: Vec<JsonStep>,
    }

    #[derive(Deserialize)]
    struct JsonStep {
        command: String,
        input: serde_json::Value,
        expect: JsonExpect,
    }

    #[derive(Deserialize)]
    struct JsonExpect {
        ok: bool,
        #[serde(default)]
        error_contains: Option<String>,
        #[serde(default)]
        task_count: Option<usize>,
        #[serde(default)]
        tasks: Option<Vec<serde_json::Value>>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        context: Option<Vec<String>>,
        #[serde(default)]
        modified_files: Option<Vec<String>>,
        /// Assert exact `store.format_context()` output (array of lines, joined with \n)
        /// Empty array means format_context should return None.
        #[serde(default)]
        format_context: Option<Vec<String>>,
    }

    fn build_tool(command: &str, input: &serde_json::Value) -> Result<TaskTool, String> {
        let mut obj = input.clone();
        obj.as_object_mut()
            .unwrap()
            .insert("command".to_string(), serde_json::json!(command));
        serde_json::from_value(obj).map_err(|e| e.to_string())
    }

    #[test]
    fn test_from_json_data() {
        let json_data = include_str!("test_data/task_tool_tests.json");
        let cases: Vec<JsonTestCase> = serde_json::from_str(json_data).expect("Failed to parse task_tool_tests.json");

        for tc in &cases {
            let (_dir, store) = temp_store();

            for (i, step) in tc.steps.iter().enumerate() {
                let ctx = format!("test '{}' step {}", tc.name, i);
                let tool = match build_tool(&step.command, &step.input) {
                    Ok(t) => t,
                    Err(e) => {
                        assert!(!step.expect.ok, "{ctx}: unexpected deserialization error: {e}");
                        if let Some(needle) = &step.expect.error_contains {
                            assert!(e.contains(needle), "{ctx}: deser error '{e}' should contain '{needle}'");
                        }
                        continue;
                    },
                };
                let result = tool.execute(&store);

                if !step.expect.ok {
                    let err = result.expect_err(&format!("{ctx}: expected Err, got Ok"));
                    if let Some(needle) = &step.expect.error_contains {
                        let msg = err.to_string();
                        assert!(msg.contains(needle), "{ctx}: error '{msg}' should contain '{needle}'");
                    }
                    continue;
                }

                let output = result.unwrap_or_else(|e| panic!("{ctx}: expected Ok, got Err({e})"));
                let json = match output.items.first().unwrap() {
                    ToolExecutionOutputItem::Json(v) => v.clone(),
                    _ => panic!("{ctx}: expected JSON output"),
                };
                let disk = read_disk(&store);

                // task_count: response and disk must agree
                if let Some(expected) = step.expect.task_count {
                    let resp_count = json["tasks"].as_array().map(|a| a.len()).unwrap_or(0);
                    assert_eq!(resp_count, expected, "{ctx}: response task_count");
                    assert_eq!(disk.task_count, expected, "{ctx}: disk task file count");
                    if expected == 0 {
                        assert_eq!(
                            disk.total_file_count, 0,
                            "{ctx}: disk should be fully empty after cleanup"
                        );
                    }
                }

                // tasks: check each expected task against both response and disk
                if let Some(expected_tasks) = &step.expect.tasks {
                    let resp_tasks = json["tasks"].as_array().unwrap();
                    for et in expected_tasks {
                        let id = et["id"].as_str().unwrap();
                        let resp = resp_tasks
                            .iter()
                            .find(|t| t["id"].as_str() == Some(id))
                            .unwrap_or_else(|| panic!("{ctx}: task id={id} not in response"));
                        let on_disk = disk.tasks.iter().find(|t| t.0 == id);

                        if let Some(desc) = et.get("task_description").and_then(|v| v.as_str()) {
                            assert_eq!(
                                resp["task_description"].as_str().unwrap(),
                                desc,
                                "{ctx}: response task {id} description"
                            );
                            if let Some(d) = on_disk {
                                assert_eq!(d.1, desc, "{ctx}: disk task {id} subject");
                            }
                        }
                        if let Some(completed) = et.get("completed").and_then(|v| v.as_bool()) {
                            assert_eq!(
                                resp["completed"].as_bool().unwrap(),
                                completed,
                                "{ctx}: response task {id} completed"
                            );
                            if let Some(d) = on_disk {
                                assert_eq!(d.2, completed, "{ctx}: disk task {id} completed");
                            }
                        }
                    }
                }

                // description: response and disk metadata
                if let Some(expected) = &step.expect.description {
                    assert_eq!(
                        json["description"].as_str().unwrap(),
                        expected,
                        "{ctx}: response description"
                    );
                    assert_eq!(&disk.metadata_description, expected, "{ctx}: disk metadata description");
                }

                // context: response and disk metadata
                if let Some(expected) = &step.expect.context {
                    let resp: Vec<String> = json["context"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect();
                    assert_eq!(&resp, expected, "{ctx}: response context");
                    assert_eq!(&disk.metadata_context, expected, "{ctx}: disk metadata context");
                }

                // modified_files: response and disk metadata
                if let Some(expected) = &step.expect.modified_files {
                    let resp: Vec<String> = json["modified_files"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect();
                    assert_eq!(&resp, expected, "{ctx}: response modified_files");
                    assert_eq!(
                        &disk.metadata_modified_files, expected,
                        "{ctx}: disk metadata modified_files"
                    );
                }

                // format_context assertions (calls store directly — not disk state)
                if let Some(lines) = &step.expect.format_context {
                    let fmt_ctx = store
                        .format_context()
                        .unwrap_or_else(|e| panic!("{ctx}: format_context error: {e}"));
                    if lines.is_empty() {
                        assert!(fmt_ctx.is_none(), "{ctx}: format_context should be None");
                    } else {
                        let expected: String = lines.concat();
                        let actual: String = fmt_ctx
                            .as_deref()
                            .unwrap_or_else(|| panic!("{ctx}: format_context is None but expected content"))
                            .replace('\n', "");
                        assert_eq!(actual, expected, "{ctx}: format_context mismatch");
                    }
                }
            }
        }
    }
}
