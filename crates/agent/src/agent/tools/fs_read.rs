use std::path::PathBuf;

use futures::StreamExt;
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
use crate::util::path::canonicalize_path_sys;
use crate::util::providers::SystemProvider;

const MAX_READ_SIZE: u32 = 250 * 1024;

const FS_READ_TOOL_DESCRIPTION: &str = r#"
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
- Can read from any position in a file using the offset parameter
- Handles large files by limiting the number of lines read

LIMITATIONS:
- Maximum file size is 250KB
- Cannot display binary files or images

TIPS:
- Read multiple files in one go if you know you want to read more than one file
- Dont use limit and offset for small files
"#;

// TODO - migrate from JsonSchema, it's not very configurable and prone to breaking changes in the
// generated structure.
const FS_READ_SCHEMA: &str = "";

impl BuiltInToolTrait for FsRead {
    fn name() -> BuiltInToolName {
        BuiltInToolName::FsRead
    }

    fn description() -> std::borrow::Cow<'static, str> {
        FS_READ_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        FS_READ_SCHEMA.into()
    }
}

/// A tool for reading files
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsRead {
    pub ops: Vec<FsReadOp>,
}

impl FsRead {
    pub fn tool_schema() -> serde_json::Value {
        let schema = schema_for!(Self);
        serde_json::to_value(schema).expect("creating tool schema should not fail")
    }

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        let mut errors = Vec::new();
        for op in &self.ops {
            let path = PathBuf::from(canonicalize_path_sys(&op.path, provider).map_err(|e| e.to_string())?);
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

    pub async fn execute<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let mut results = Vec::new();
        let mut errors = Vec::new();
        for op in &self.ops {
            match op.execute(provider).await {
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
pub struct FsReadOp {
    /// Path to the file
    pub path: String,
    /// Number of lines to read
    pub limit: Option<u32>,
    /// Line offset from the start of the file to start reading from
    pub offset: Option<u32>,
}

impl FsReadOp {
    async fn execute<P: SystemProvider>(&self, provider: &P) -> Result<ToolExecutionOutputItem, ToolExecutionError> {
        let path = PathBuf::from(
            canonicalize_path_sys(&self.path, provider).map_err(|e| ToolExecutionError::Custom(e.to_string()))?,
        );

        // TODO: add line numbers
        let file_lines = LinesStream::new(
            BufReader::new(
                fs::File::open(&path)
                    .await
                    .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?,
            )
            .lines(),
        );
        let mut file_lines = file_lines
            .enumerate()
            .skip(self.offset.unwrap_or_default() as usize)
            .take(self.limit.unwrap_or(u32::MAX) as usize);

        let mut is_truncated = false;
        let mut content = Vec::new();
        while let Some((i, line)) = file_lines.next().await {
            match line {
                Ok(l) => {
                    if content.len() as u32 > MAX_READ_SIZE {
                        is_truncated = true;
                        break;
                    }
                    content.push(l);
                },
                Err(err) => {
                    return Err(ToolExecutionError::io(format!("Failed to read line {}", i + 1,), err));
                },
            }
        }

        let mut content = content.join("\n");
        if is_truncated {
            content.push_str("...truncated");
        }
        Ok(ToolExecutionOutputItem::Text(content))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadContext {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestDir;
    use crate::util::test::TestProvider;

    #[tokio::test]
    async fn test_fs_read_single_file() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new().with_file(("test.txt", "line1\nline2\nline3")).await;

        let tool = FsRead {
            ops: vec![FsReadOp {
                path: test_dir.join("test.txt").to_string_lossy().to_string(),
                limit: None,
                offset: None,
            }],
        };

        assert!(tool.validate(&test_provider).await.is_ok());
        let result = tool.execute(&test_provider).await.unwrap();
        assert_eq!(result.items.len(), 1);
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert_eq!(content, "line1\nline2\nline3");
        }
    }

    #[tokio::test]
    async fn test_fs_read_with_offset_and_limit() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new()
            .with_file(("test.txt", "line1\nline2\nline3\nline4\nline5"))
            .await;

        let tool = FsRead {
            ops: vec![FsReadOp {
                path: test_dir.join("test.txt").to_string_lossy().to_string(),
                limit: Some(2),
                offset: Some(1),
            }],
        };

        let result = tool.execute(&test_provider).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert_eq!(content, "line2\nline3");
        }
    }

    #[tokio::test]
    async fn test_fs_read_multiple_files() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new()
            .with_file(("file1.txt", "content1"))
            .await
            .with_file(("file2.txt", "content2"))
            .await;

        let tool = FsRead {
            ops: vec![
                FsReadOp {
                    path: test_dir.join("file1.txt").to_string_lossy().to_string(),
                    limit: None,
                    offset: None,
                },
                FsReadOp {
                    path: test_dir.join("file2.txt").to_string_lossy().to_string(),
                    limit: None,
                    offset: None,
                },
            ],
        };

        let result = tool.execute(&test_provider).await.unwrap();
        assert_eq!(result.items.len(), 2);
    }

    #[tokio::test]
    async fn test_fs_read_validate_nonexistent_file() {
        let test_provider = TestProvider::new();
        let tool = FsRead {
            ops: vec![FsReadOp {
                path: "/nonexistent/file.txt".to_string(),
                limit: None,
                offset: None,
            }],
        };

        assert!(tool.validate(&test_provider).await.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_validate_directory_path() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new();

        let tool = FsRead {
            ops: vec![FsReadOp {
                path: test_dir.join("").to_string_lossy().to_string(),
                limit: None,
                offset: None,
            }],
        };

        assert!(tool.validate(&test_provider).await.is_err());
    }
}
