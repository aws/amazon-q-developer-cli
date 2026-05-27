use std::path::PathBuf;

use super::ProjectMetadata;

/// File-backed project metadata store. Manages `project_metadata.json`.
#[derive(Debug)]
pub(crate) struct ProjectStore {
    dir: PathBuf,
}

impl ProjectStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn read(&self) -> ProjectMetadata {
        match std::fs::read_to_string(self.path()) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => ProjectMetadata::default(),
        }
    }

    pub(crate) fn write(&self, meta: &ProjectMetadata) -> Result<(), String> {
        use std::io::Write;
        self.ensure_dir()?;
        let data =
            serde_json::to_string_pretty(meta).map_err(|e| format!("Failed to serialize project metadata: {e}"))?;
        let file =
            std::fs::File::create(self.path()).map_err(|e| format!("Failed to create project metadata file: {e}"))?;
        fs4::fs_std::FileExt::lock_exclusive(&file)
            .map_err(|e| format!("Failed to lock project metadata file: {e}"))?;
        let mut writer = std::io::BufWriter::new(&file);
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write project metadata: {e}"))?;
        Ok(())
    }

    /// Append a context entry and modified files with an exclusive lock for the read-modify-write.
    pub(crate) fn append(&self, context_update: &str, modified_files: Option<&[String]>) -> Result<(), String> {
        use std::io::Write;
        self.ensure_dir()?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path())
            .map_err(|e| format!("Failed to open project metadata file: {e}"))?;
        fs4::fs_std::FileExt::lock_exclusive(&file)
            .map_err(|e| format!("Failed to lock project metadata file: {e}"))?;

        let mut meta: ProjectMetadata = std::io::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        meta.context.push(context_update.to_string());
        if let Some(files) = modified_files {
            meta.modified_files.extend_from_slice(files);
        }
        meta.modified_files.sort();
        meta.modified_files.dedup();

        let data =
            serde_json::to_string_pretty(&meta).map_err(|e| format!("Failed to serialize project metadata: {e}"))?;
        file.set_len(0)
            .map_err(|e| format!("Failed to truncate project metadata file: {e}"))?;
        std::io::Seek::seek(&mut &file, std::io::SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek project metadata file: {e}"))?;
        let mut writer = std::io::BufWriter::new(&file);
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write project metadata: {e}"))?;
        Ok(())
    }

    pub(crate) fn delete(&self) -> Result<(), String> {
        let path = self.path();
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete project metadata: {e}"))?;
        }
        Ok(())
    }

    fn path(&self) -> PathBuf {
        self.dir.join("project_metadata.json")
    }

    fn ensure_dir(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("Failed to create task dir: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_store_read_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        let meta = store.read();
        assert!(meta.description.is_empty());
        assert!(meta.context.is_empty());
        assert!(meta.modified_files.is_empty());
    }

    #[test]
    fn test_project_store_write_and_read() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        let meta = ProjectMetadata {
            description: "Build app".to_string(),
            context: vec!["c1".to_string()],
            modified_files: vec!["a.rs".to_string()],
        };
        store.write(&meta).unwrap();
        let read_back = store.read();
        assert_eq!(read_back.description, "Build app");
        assert_eq!(read_back.context, vec!["c1".to_string()]);
        assert_eq!(read_back.modified_files, vec!["a.rs".to_string()]);
    }

    #[test]
    fn test_project_store_append_first() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        store.append("first context", Some(&["a.rs".to_string()])).unwrap();
        let meta = store.read();
        assert_eq!(meta.context, vec!["first context".to_string()]);
        assert_eq!(meta.modified_files, vec!["a.rs".to_string()]);
    }

    #[test]
    fn test_project_store_append_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        store.append("c1", Some(&["a.rs".to_string()])).unwrap();
        store
            .append("c2", Some(&["b.rs".to_string(), "a.rs".to_string()]))
            .unwrap();
        let meta = store.read();
        assert_eq!(meta.context.len(), 2);
        // dedup applied
        assert_eq!(meta.modified_files, vec!["a.rs".to_string(), "b.rs".to_string()]);
    }

    #[test]
    fn test_project_store_append_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        store.append("just context", None).unwrap();
        let meta = store.read();
        assert_eq!(meta.context, vec!["just context".to_string()]);
        assert!(meta.modified_files.is_empty());
    }

    #[test]
    fn test_project_store_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        store.append("c1", None).unwrap();
        store.delete().unwrap();
        let meta = store.read();
        assert!(meta.context.is_empty());
    }

    #[test]
    fn test_project_store_delete_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        // Should not error if file doesn't exist
        store.delete().unwrap();
    }

    #[test]
    fn test_project_store_read_corrupted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("project_metadata.json"), "{invalid json").unwrap();
        let store = ProjectStore::new(tmp.path().to_path_buf());
        let meta = store.read();
        // Should fall back to default
        assert!(meta.description.is_empty());
    }
}
