use std::path::PathBuf;

use futures::StreamExt;
use rand::seq::IndexedRandom;
use schemars::{
    JsonSchema,
    schema_for,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs;
use tokio::io::{
    AsyncBufReadExt,
    BufReader,
};
use tokio_stream::wrappers::LinesStream;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::util::path::canonicalize_path;

const MAX_READ_SIZE: u32 = 250 * 1024;

const FILE_READ_TOOL_DESCRIPTION: &str = r#"
A tool for viewing file contents.

WHEN TO USE THIS TOOL:
- Use when you need to read the contents of a specific file
- Helpful for examining source code, configuration files, or log files
- Perfect for looking at text-based file formats

HOW TO USE:
- Provide the path to the file you want to view
- Optionally specify an offset to start reading from a specific line
- Optionally specify a limit to control how many lines are read
- Do not use this for directories, use the ls tool instead

FEATURES:
- Displays file contents with line numbers for easy reference
- Can read from any position in a file using the offset parameter
- Handles large files by limiting the number of lines read

LIMITATIONS:
- Maximum file size is 250KB
- Cannot display binary files or images
- Images can be identified but not displayed

TIPS:
- Use with Glob tool to first find files you want to view
- For code exploration, first use Grep to find relevant files, then View to examine them
- When viewing large files, use the offset parameter to read specific sections
"#;

// TODO - migrate from JsonSchema, it's not very configurable and prone to breaking changes in the
// generated structure.
const FILE_READ_SCHEMA: &str = "";

impl BuiltInToolTrait for FileRead {
    const DESCRIPTION: &str = FILE_READ_TOOL_DESCRIPTION;
    const INPUT_SCHEMA: &str = FILE_READ_SCHEMA;
    const NAME: BuiltInToolName = BuiltInToolName::FileRead;
}

/// A tool for reading files
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileRead {
    pub ops: Vec<FileReadOp>,
}

impl FileRead {
    pub fn tool_schema() -> serde_json::Value {
        let schema = schema_for!(Self);
        serde_json::to_value(schema).expect("creating tool schema should not fail")
    }

    pub async fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        for op in &self.ops {
            let path = PathBuf::from(canonicalize_path(&op.path).map_err(|e| e.to_string())?);
            if !path.exists() {
                errors.push(format!("'{}' does not exist", path.to_string_lossy()));
                continue;
            }
            let file_md = tokio::fs::symlink_metadata(&path).await;
            let Ok(file_md) = file_md else {
                errors.push(format!(
                    "Failed to check file metadata for '{}'",
                    path.to_string_lossy()
                ));
                continue;
            };
            if !file_md.is_file() {
                errors.push(format!("'{}' is not a file", path.to_string_lossy()));
            }
        }
        if !errors.is_empty() {
            Err(errors.join("\n"))
        } else {
            Ok(())
        }
    }

    pub async fn execute(&self) -> ToolExecutionResult {
        let mut results = Vec::new();
        let mut errors = Vec::new();
        for op in &self.ops {
            match op.execute().await {
                Ok(res) => results.push(res),
                Err(err) => errors.push((op.clone(), err)),
            }
        }
        if !errors.is_empty() {
            let err_msg = errors
                .into_iter()
                .map(|(op, err)| format!("Operation for '{}' failed: {}", op.path, err))
                .collect::<Vec<_>>()
                .join(",");
            Err(ToolExecutionError::Custom(err_msg))
        } else {
            Ok(ToolExecutionOutput::new(results))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileReadOp {
    /// Path to the file
    pub path: String,
    /// Number of lines to read
    pub limit: Option<u32>,
    /// Line offset from the start of the file to start reading from
    pub offset: Option<u32>,
}

impl FileReadOp {
    async fn execute(&self) -> Result<ToolExecutionOutputItem, ToolExecutionError> {
        let path = PathBuf::from(canonicalize_path(&self.path).map_err(|e| ToolExecutionError::Custom(e.to_string()))?);

        // TODO: add image reading
        // add line numbers
        // add extra truncated context
        let file_lines = LinesStream::new(
            BufReader::new(
                fs::File::open(&path)
                    .await
                    .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?,
            )
            .lines(),
        );
        let mut file_lines = file_lines.enumerate().skip(self.offset.unwrap_or_default() as usize);

        let mut content = Vec::new();
        while let Some((i, line)) = file_lines.next().await {
            match line {
                Ok(l) => {
                    if content.len() as u32 > MAX_READ_SIZE {
                        break;
                    }
                    content.push(l);
                },
                Err(err) => {
                    return Err(ToolExecutionError::io(format!("Failed to read line {}", i + 1,), err));
                },
            }
        }

        let content = content.join("\n");
        Ok(ToolExecutionOutputItem::Text(content))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadContext {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_tool_schema() {
        let schema = FileRead::tool_schema();
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    }
}
