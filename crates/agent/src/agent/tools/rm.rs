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
