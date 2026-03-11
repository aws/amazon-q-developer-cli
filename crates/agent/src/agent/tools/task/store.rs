use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

use super::{
    Task,
    TaskPatch,
    TaskStatus,
    TaskSummary,
};

/// File-backed task store. Each task is stored as an individual JSON file.
/// The next ID counter is held in memory and recovered from disk on creation.
#[derive(Debug)]
pub struct TaskStore {
    pub(crate) dir: PathBuf,
    next_id: AtomicU64,
}

impl TaskStore {
    pub fn new(session_id: &str) -> Self {
        let dir = task_store_dir(session_id);
        let next_id = Self::recover_next_id(&dir);
        Self {
            dir,
            next_id: AtomicU64::new(next_id),
        }
    }

    #[cfg(test)]
    fn with_dir(dir: PathBuf) -> Self {
        let next_id = Self::recover_next_id(&dir);
        Self {
            dir,
            next_id: AtomicU64::new(next_id),
        }
    }

    /// Scan task directory for the highest existing ID.
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

    /// Allocate the next task ID.
    pub(crate) fn allocate_id(&self) -> String {
        self.next_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    /// Ensure the storage directory exists.
    pub(crate) fn ensure_dir(&self) -> Result<(), String> {
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

    /// Write a task to disk with an exclusive file lock.
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
        // Lock released on drop
        Ok(())
    }

    /// Remove a task's JSON file from disk.
    fn delete_task_file(&self, id: &str) -> Result<(), String> {
        let path = self.task_path(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete task file: {e}"))?;
        }
        Ok(())
    }

    /// Get a task by ID.
    ///
    /// The returned task's `depends_on` is filtered at read-time to exclude
    /// completed and deleted dependencies, so callers see only open blockers.
    /// The on-disk JSON retains the original declared dependencies.
    pub fn get(&self, id: &str) -> Result<Task, String> {
        let mut task = self.read_task(id)?;
        task.depends_on.retain(|dep_id| {
            self.read_task(dep_id)
                .is_ok_and(|t| t.status != TaskStatus::Completed && t.status != TaskStatus::Deleted)
        });
        Ok(task)
    }

    /// Update a task. Only provided fields are changed.
    pub fn update(&self, id: &str, patch: TaskPatch) -> Result<Task, String> {
        let mut task = self.read_task(id)?;

        if let Some(s) = patch.status {
            task.status = s;
        }
        if let Some(s) = patch.subject {
            task.subject = s;
        }
        if let Some(d) = patch.description {
            task.description = d;
        }
        if let Some(o) = patch.owner {
            task.owner = Some(o);
        }

        if let Some(dep_ids) = patch.add_depends_on {
            for dep_id in dep_ids {
                if !task.depends_on.contains(&dep_id) {
                    task.depends_on.push(dep_id);
                }
            }
        }

        // Merge metadata (null values delete keys)
        if let Some(meta) = patch.metadata {
            for (key, value) in meta {
                if value.is_null() {
                    task.metadata.remove(&key);
                } else {
                    task.metadata.insert(key, value);
                }
            }
        }

        // Delete file from disk if status is deleted, otherwise write
        if task.status == TaskStatus::Deleted {
            self.delete_task_file(&task.id)?;
        } else {
            self.write_task(&task)?;
        }
        Ok(task)
    }

    /// List all non-deleted tasks as summaries.
    ///
    /// Deleted tasks are excluded entirely. Each summary's `depends_on` is
    /// filtered at read-time to only include open (non-completed, non-deleted)
    /// dependencies.
    pub fn list(&self) -> Result<Vec<TaskSummary>, String> {
        let tasks = self.read_all_tasks()?;
        let status_map: HashMap<&str, &TaskStatus> = tasks.iter().map(|t| (t.id.as_str(), &t.status)).collect();

        let mut summaries: Vec<TaskSummary> = tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Deleted)
            .map(|t| {
                // Filter depends_on to only include open (non-completed, non-deleted) tasks
                let open_depends_on: Vec<String> = t
                    .depends_on
                    .iter()
                    .filter(|bid| {
                        status_map
                            .get(bid.as_str())
                            .is_some_and(|s| **s != TaskStatus::Completed && **s != TaskStatus::Deleted)
                    })
                    .cloned()
                    .collect();
                t.to_summary(open_depends_on)
            })
            .collect();

        summaries.sort_by(|a, b| a.id.parse::<u64>().unwrap_or(0).cmp(&b.id.parse::<u64>().unwrap_or(0)));

        Ok(summaries)
    }

    /// Delete all task files if every existing task is completed.
    /// Called before creating a new task to start fresh.
    pub(crate) fn cleanup_if_all_completed(&self) -> Result<(), String> {
        let tasks = self.read_all_tasks()?;
        if tasks.is_empty() || !tasks.iter().all(|t| t.status == TaskStatus::Completed) {
            return Ok(());
        }
        for task in &tasks {
            self.delete_task_file(&task.id)?;
        }
        Ok(())
    }

    /// Read all tasks from disk.
    fn read_all_tasks(&self) -> Result<Vec<Task>, String> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let mut tasks = Vec::new();
        let entries = std::fs::read_dir(&self.dir).map_err(|e| format!("Failed to read task dir: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
            let path = entry.path();
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

    /// Format task list as context for injection into conversation.
    pub fn format_context(&self) -> Result<Option<String>, String> {
        let summaries = self.list()?;
        if summaries.is_empty() {
            return Ok(None);
        }

        let completed = summaries.iter().filter(|s| s.status == TaskStatus::Completed).count();
        let total = summaries.len();

        let mut output = format!("Active Task List ({completed}/{total} completed):\n");

        for s in &summaries {
            let icon = match s.status {
                TaskStatus::Completed => "✓",
                TaskStatus::InProgress => "■",
                TaskStatus::Pending => "□",
                TaskStatus::Deleted => continue,
            };

            let mut line = format!("{icon} #{}: {}", s.id, s.subject);

            if let Some(owner) = &s.owner {
                line.push_str(&format!(" (owner: {owner})"));
            }

            if !s.depends_on.is_empty() {
                let blockers: Vec<String> = s.depends_on.iter().map(|b| format!("#{b}")).collect();
                line.push_str(&format!(" — depends on {}", blockers.join(", ")));
            }

            output.push_str(&line);
            output.push('\n');
        }

        Ok(Some(output))
    }
}

/// Build the task store directory path co-located with the session.
/// Path: ~/.kiro/sessions/cli/{session_id}/tasks/
pub fn task_store_dir(session_id: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".kiro")
        .join("sessions")
        .join("cli")
        .join(session_id)
        .join("tasks")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, TaskStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    /// Test helper: create a task with just subject and description.
    impl TaskStore {
        fn create(
            &self,
            subject: String,
            description: String,
            metadata: Option<HashMap<String, serde_json::Value>>,
        ) -> Result<Task, String> {
            let task = Task {
                id: self.allocate_id(),
                subject,
                description,
                status: TaskStatus::Pending,
                owner: None,
                depends_on: Vec::new(),
                metadata: metadata.unwrap_or_default(),
            };
            self.write_task(&task)?;
            Ok(task)
        }
    }

    #[test]
    fn test_create_and_get() {
        let (_dir, store) = temp_store();
        let task = store
            .create("Test task".to_string(), "A description".to_string(), None)
            .unwrap();
        assert_eq!(task.id, "1");
        assert_eq!(task.status, TaskStatus::Pending);

        let fetched = store.get("1").unwrap();
        assert_eq!(fetched.subject, "Test task");
    }

    #[test]
    fn test_auto_increment_ids() {
        let (_dir, store) = temp_store();
        let t1 = store.create("First".to_string(), "desc".to_string(), None).unwrap();
        let t2 = store.create("Second".to_string(), "desc".to_string(), None).unwrap();
        assert_eq!(t1.id, "1");
        assert_eq!(t2.id, "2");
    }

    #[test]
    fn test_update_status() {
        let (_dir, store) = temp_store();
        store.create("Task".to_string(), "desc".to_string(), None).unwrap();
        let updated = store
            .update("1", TaskPatch {
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_bidirectional_deps() {
        let (_dir, store) = temp_store();
        store.create("Task A".to_string(), "desc".to_string(), None).unwrap();
        store.create("Task B".to_string(), "desc".to_string(), None).unwrap();

        // Task B depends on Task A
        store
            .update("2", TaskPatch {
                add_depends_on: Some(vec!["1".to_string()]),
                ..Default::default()
            })
            .unwrap();

        let task_b = store.get("2").unwrap();
        assert!(task_b.depends_on.contains(&"1".to_string()));
    }

    #[test]
    fn test_list_filters_deleted() {
        let (_dir, store) = temp_store();
        store.create("Active".to_string(), "desc".to_string(), None).unwrap();
        store.create("Deleted".to_string(), "desc".to_string(), None).unwrap();
        store
            .update("2", TaskPatch {
                status: Some(TaskStatus::Deleted),
                ..Default::default()
            })
            .unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "1");
    }

    #[test]
    fn test_list_filters_completed_blockers() {
        let (_dir, store) = temp_store();
        store.create("Blocker".to_string(), "desc".to_string(), None).unwrap();
        store.create("Blocked".to_string(), "desc".to_string(), None).unwrap();
        store
            .update("2", TaskPatch {
                add_depends_on: Some(vec!["1".to_string()]),
                ..Default::default()
            })
            .unwrap();

        // Before completing blocker
        let list = store.list().unwrap();
        assert_eq!(list[1].depends_on, vec!["1".to_string()]);

        // After completing blocker
        store
            .update("1", TaskPatch {
                status: Some(TaskStatus::Completed),
                ..Default::default()
            })
            .unwrap();
        let list = store.list().unwrap();
        assert!(list[1].depends_on.is_empty());
    }

    #[test]
    fn test_metadata_merge() {
        let (_dir, store) = temp_store();
        store.create("Task".to_string(), "desc".to_string(), None).unwrap();

        let mut meta = HashMap::new();
        meta.insert("key1".to_string(), serde_json::json!("value1"));
        meta.insert("key2".to_string(), serde_json::json!(42));
        store
            .update("1", TaskPatch {
                metadata: Some(meta),
                ..Default::default()
            })
            .unwrap();

        // Delete key1, add key3
        let mut meta2 = HashMap::new();
        meta2.insert("key1".to_string(), serde_json::Value::Null);
        meta2.insert("key3".to_string(), serde_json::json!("new"));
        let task = store
            .update("1", TaskPatch {
                metadata: Some(meta2),
                ..Default::default()
            })
            .unwrap();

        assert!(!task.metadata.contains_key("key1"));
        assert_eq!(task.metadata["key2"], serde_json::json!(42));
        assert_eq!(task.metadata["key3"], serde_json::json!("new"));
    }

    #[test]
    fn test_format_context() {
        let (_dir, store) = temp_store();
        store.create("Setup".to_string(), "desc".to_string(), None).unwrap();
        store.create("Build".to_string(), "desc".to_string(), None).unwrap();
        store
            .update("1", TaskPatch {
                status: Some(TaskStatus::Completed),
                ..Default::default()
            })
            .unwrap();

        let ctx = store.format_context().unwrap().unwrap();
        assert!(ctx.contains("1/2 completed"));
        assert!(ctx.contains("✓ #1: Setup"));
        assert!(ctx.contains("□ #2: Build"));
    }

    #[test]
    fn test_format_context_empty() {
        let (_dir, store) = temp_store();
        assert!(store.format_context().unwrap().is_none());
    }
}
