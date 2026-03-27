use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

use super::project_store::ProjectStore;
use super::{
    Task,
    TaskStatus,
    TaskSummary,
};

/// File-backed task store. Each task is stored as an individual JSON file.
/// The next ID counter is held in memory and recovered from disk on creation.
#[derive(Debug)]
pub struct TaskStore {
    pub(crate) dir: PathBuf,
    pub(crate) project: ProjectStore,
    next_id: AtomicU64,
}

impl TaskStore {
    pub fn new(session_id: &str) -> Self {
        Self::from_dir(task_store_dir(session_id))
    }

    /// Format task list as context for injection into conversation.
    ///
    /// Matches V1's `format_todo_as_context` output as closely as possible so the
    /// LLM sees an identical context shape.
    pub fn format_context(&self) -> Result<Option<String>, String> {
        let summaries = self.list()?;
        if summaries.is_empty() {
            return Ok(None);
        }

        let completed_count = summaries.iter().filter(|s| s.status == TaskStatus::Completed).count();
        let total_count = summaries.len();
        let plan_meta = self.project.read();

        let mut output = String::new();
        output.push_str("Active Task List for current session:\n\n");
        if !plan_meta.description.is_empty() {
            output.push_str(&format!("Description: {}\n", plan_meta.description));
        }
        output.push_str(&format!(
            "Progress: {completed_count}/{total_count} tasks completed\n\n"
        ));

        output.push_str("Tasks:\n");
        let mut found_next = false;
        for s in &summaries {
            if s.status == TaskStatus::Deleted {
                continue;
            }
            let checkbox = if s.status == TaskStatus::Completed {
                "[✓]"
            } else {
                "[ ]"
            };
            let next_marker = if s.status != TaskStatus::Completed && !found_next {
                found_next = true;
                " (NEXT)"
            } else {
                ""
            };
            output.push_str(&format!("{} #{}. {}{}\n", checkbox, s.id, s.subject, next_marker));
        }

        if !plan_meta.context.is_empty() {
            output.push_str("\nRecent Context:\n");
            for ctx in plan_meta.context.iter().rev().take(3).rev() {
                output.push_str(&format!("- {ctx}\n"));
            }
        }

        if !plan_meta.modified_files.is_empty() {
            output.push_str("\nModified Files:\n");
            for file in &plan_meta.modified_files {
                output.push_str(&format!("- {file}\n"));
            }
        }

        Ok(Some(output))
    }

    #[cfg(test)]
    pub(crate) fn with_dir(dir: PathBuf) -> Self {
        Self::from_dir(dir)
    }

    pub(crate) fn allocate_id(&self) -> String {
        self.next_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    pub(crate) fn write_task(&self, task: &Task) -> Result<(), String> {
        use std::io::Write;
        self.ensure_dir()?;
        let path = self.task_path(&task.id);
        let data = serde_json::to_string_pretty(task).map_err(|e| format!("Failed to serialize task: {e}"))?;
        let file = std::fs::File::create(&path).map_err(|e| format!("Failed to create task file: {e}"))?;
        fs4::fs_std::FileExt::lock_exclusive(&file).map_err(|e| format!("Failed to lock task file: {e}"))?;
        let mut writer = std::io::BufWriter::new(&file);
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write task: {e}"))?;
        drop(writer);
        Ok(())
    }

    pub(crate) fn update_status(&self, id: &str, status: TaskStatus) -> Result<Task, String> {
        let mut task = self.read_task(id)?;
        task.status = status;
        if task.status == TaskStatus::Deleted {
            self.delete_task_file(&task.id)?;
        } else {
            self.write_task(&task)?;
        }
        Ok(task)
    }

    pub(crate) fn list(&self) -> Result<Vec<TaskSummary>, String> {
        let tasks = self.read_all_tasks()?;
        let mut summaries: Vec<TaskSummary> = tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Deleted)
            .map(|t| t.to_summary())
            .collect();
        summaries.sort_by(|a, b| a.id.parse::<u64>().unwrap_or(0).cmp(&b.id.parse::<u64>().unwrap_or(0)));
        Ok(summaries)
    }

    /// Delete all task files and project metadata unconditionally, resetting to a clean state.
    pub(crate) fn cleanup_all(&self) -> Result<(), String> {
        let tasks = self.read_all_tasks()?;
        for task in &tasks {
            self.delete_task_file(&task.id)?;
        }
        self.project.delete()?;
        self.next_id.store(1, Ordering::Relaxed);
        Ok(())
    }

    /// Delete all task files if every existing task is completed.
    pub(crate) fn cleanup_if_all_completed(&self) -> Result<(), String> {
        let tasks = self.read_all_tasks()?;
        if tasks.is_empty() || !tasks.iter().all(|t| t.status == TaskStatus::Completed) {
            return Ok(());
        }
        self.cleanup_all()
    }

    fn from_dir(dir: PathBuf) -> Self {
        let next_id = Self::recover_next_id(&dir);
        let project = ProjectStore::new(dir.clone());
        Self {
            dir,
            project,
            next_id: AtomicU64::new(next_id),
        }
    }

    fn recover_next_id(dir: &Path) -> u64 {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .map_or(1, |id| id + 1)
    }

    fn ensure_dir(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("Failed to create task dir: {e}"))
    }

    fn task_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn read_task(&self, id: &str) -> Result<Task, String> {
        let path = self.task_path(id);
        let data = std::fs::read_to_string(&path).map_err(|e| format!("Task {id} not found: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse task {id}: {e}"))
    }

    fn delete_task_file(&self, id: &str) -> Result<(), String> {
        let path = self.task_path(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete task file: {e}"))?;
        }
        Ok(())
    }

    fn read_all_tasks(&self) -> Result<Vec<Task>, String> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut tasks = Vec::new();
        let entries = std::fs::read_dir(&self.dir).map_err(|e| format!("Failed to read task dir: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == "project_metadata.json") {
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "json") {
                let data = std::fs::read_to_string(&path).map_err(|e| format!("Failed to read task file: {e}"))?;
                match serde_json::from_str::<Task>(&data) {
                    Ok(task) => tasks.push(task),
                    Err(e) => tracing::warn!("Skipping malformed task file {:?}: {}", path, e),
                }
            }
        }
        Ok(tasks)
    }
}

/// Build the task store directory path co-located with the session.
/// Path: ~/.kiro/sessions/cli/{session_id}/tasks/
/// Respects `KIRO_TEST_SESSIONS_DIR` for testing.
pub fn task_store_dir(session_id: &str) -> PathBuf {
    let base = if let Ok(test_dir) = std::env::var("KIRO_TEST_SESSIONS_DIR") {
        PathBuf::from(test_dir)
    } else {
        dirs::home_dir()
            .expect("HOME directory not found")
            .join(".kiro")
            .join("sessions")
            .join("cli")
    };
    base.join(session_id).join("tasks")
}
