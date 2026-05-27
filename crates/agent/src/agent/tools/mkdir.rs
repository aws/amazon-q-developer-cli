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

pub const MKDIR_TOOL_DESCRIPTION: &str = r#"
A tool for creating directories.

WHEN TO USE THIS TOOL:
- Use when you need to create a directory

HOW TO USE:
- Provide the path for the directory to be created
- Parent directories will be created if they don't already exist
"#;

const MKDIR_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "path": {
            "description": "Path to the directory",
            "type": "string"
        }
    },
    "required": [
        "path"
    ]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mkdir {
    path: String,
}

impl Mkdir {
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

    fn make_mkdir(path: &str) -> Mkdir {
        Mkdir { path: path.into() }
    }

    #[tokio::test]
    async fn test_validate_empty_path() {
        let m = make_mkdir("");
        let err = m.validate().await.unwrap_err();
        assert!(err.contains("Path must not be empty"));
    }

    #[tokio::test]
    async fn test_validate_path_exists_as_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let m = make_mkdir(tmp.path().to_str().unwrap());
        let err = m.validate().await.unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn test_validate_path_exists_as_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("file.txt");
        tokio::fs::write(&file_path, "x").await.unwrap();
        let m = make_mkdir(file_path.to_str().unwrap());
        let err = m.validate().await.unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn test_validate_ok_for_new_path() {
        let tmp = tempfile::tempdir().unwrap();
        let new_path = tmp.path().join("new_dir");
        let m = make_mkdir(new_path.to_str().unwrap());
        assert!(m.validate().await.is_ok());
    }

    #[tokio::test]
    async fn test_execute_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let new_path = tmp.path().join("newdir");
        let m = make_mkdir(new_path.to_str().unwrap());
        let result = m.execute().await;
        assert!(result.is_ok());
        assert!(new_path.exists());
        assert!(new_path.is_dir());
    }

    #[tokio::test]
    async fn test_execute_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        let m = make_mkdir(nested.to_str().unwrap());
        let result = m.execute().await;
        assert!(result.is_ok());
        assert!(nested.exists());
    }

    #[test]
    fn test_canonical_path_works() {
        let m = make_mkdir("/tmp/test");
        assert!(m.canonical_path().is_ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let m = make_mkdir("/tmp/x");
        let json = serde_json::to_string(&m).unwrap();
        let parsed: Mkdir = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "/tmp/x");
    }

    #[test]
    fn test_mkdir_description_constant() {
        assert!(MKDIR_TOOL_DESCRIPTION.contains("creating directories"));
        assert!(MKDIR_SCHEMA.contains("path"));
    }
}
