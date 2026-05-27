#![allow(dead_code)]

use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

use super::{
    ToolExecutionError,
    ToolExecutionResult,
};
use crate::agent::util::path::canonicalize_path;

pub const RM_TOOL_DESCRIPTION: &str = r#"
A tool for removing files and directories.

WHEN TO USE THIS TOOL:
- Use when you need to remove files or directories

HOW TO USE:
- Provide the path for the directory to be created
- Parent directories will be created if they don't already exist

TIPS:
- Use the ls tool
"#;

const RM_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "path": {
            "description": "Path to the file or directory",
            "type": "string"
        }
    },
    "required": [
        "path"
    ]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rm {
    path: String,
}

impl Rm {
    fn canonical_path(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from(canonicalize_path(&self.path).map_err(|e| e.to_string())?))
    }

    pub async fn validate(&self) -> Result<(), String> {
        if self.path.is_empty() {
            return Err("Path must not be empty".to_string());
        }

        let path = self.canonical_path()?;
        if path.exists() {
            let Ok(file_md) = tokio::fs::symlink_metadata(&path).await else {
                return Err(format!("A file at {} already exists", self.path));
            };
            if file_md.is_dir() {
                return Err(format!("A directory at {} already exists", self.path));
            } else {
                return Err(format!("A file at {} already exists", self.path));
            }
        }

        Ok(())
    }

    pub async fn execute(&self) -> ToolExecutionResult {
        let path = self.canonical_path()?;
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to create directory {}", path.to_string_lossy()), e))?;
        Ok(Default::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rm(path: &str) -> Rm {
        Rm { path: path.into() }
    }

    #[tokio::test]
    async fn test_validate_empty_path() {
        let r = make_rm("");
        let err = r.validate().await.unwrap_err();
        assert!(err.contains("Path must not be empty"));
    }

    #[tokio::test]
    async fn test_validate_path_exists_as_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let r = make_rm(tmp.path().to_str().unwrap());
        let err = r.validate().await.unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn test_validate_path_exists_as_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("file.txt");
        tokio::fs::write(&file_path, "x").await.unwrap();
        let r = make_rm(file_path.to_str().unwrap());
        let err = r.validate().await.unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn test_validate_ok_for_new_path() {
        let tmp = tempfile::tempdir().unwrap();
        let new_path = tmp.path().join("nonexistent");
        let r = make_rm(new_path.to_str().unwrap());
        assert!(r.validate().await.is_ok());
    }

    #[tokio::test]
    async fn test_execute_creates_dir() {
        // Note: execute currently creates a directory (looks like a stub/bug,
        // but we test the actual behavior)
        let tmp = tempfile::tempdir().unwrap();
        let new_path = tmp.path().join("newdir");
        let r = make_rm(new_path.to_str().unwrap());
        let result = r.execute().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_canonical_path_works() {
        let r = make_rm("/tmp/test");
        assert!(r.canonical_path().is_ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let r = make_rm("/tmp/x");
        let json = serde_json::to_string(&r).unwrap();
        let parsed: Rm = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "/tmp/x");
    }

    #[test]
    fn test_rm_description_constant() {
        assert!(RM_TOOL_DESCRIPTION.contains("removing files"));
        assert!(RM_SCHEMA.contains("path"));
    }
}
