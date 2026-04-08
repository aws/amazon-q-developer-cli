use std::path::PathBuf;

use futures::StreamExt;
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

use super::MAX_READ_SIZE;
use crate::agent::tools::{
    ToolExecutionError,
    ToolExecutionOutputItem,
};
use crate::util::path::resolve_path_fuzzy;
use crate::util::providers::SystemProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOp {
    /// Path to the file
    pub path: String,
    /// Number of lines to read
    pub limit: Option<u32>,
    /// Line offset from the start of the file to start reading from
    pub offset: Option<u32>,
}

impl FileOp {
    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        let path = PathBuf::from(resolve_path_fuzzy(&self.path, provider).map_err(|e| e.to_string())?);
        if !path.exists() {
            return Err(format!("'{}' does not exist", path.to_string_lossy()));
        }
        let file_md = tokio::fs::metadata(&path).await;
        let Ok(file_md) = file_md else {
            return Err(format!(
                "Failed to check file metadata for '{}'",
                path.to_string_lossy()
            ));
        };
        if !file_md.is_file() {
            return Err(format!("'{}' is not a file", path.to_string_lossy()));
        }
        Ok(())
    }

    pub async fn execute<P: SystemProvider>(
        &self,
        provider: &P,
    ) -> Result<ToolExecutionOutputItem, ToolExecutionError> {
        let path = PathBuf::from(
            resolve_path_fuzzy(&self.path, provider).map_err(|e| ToolExecutionError::Custom(e.to_string()))?,
        );

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
        let mut byte_count: u32 = 0;
        let start_line = self.offset.unwrap_or_default() as usize + 1; // 1-indexed
        let mut last_line = start_line;
        while let Some((i, line)) = file_lines.next().await {
            match line {
                Ok(l) => {
                    byte_count += l.len() as u32 + 1; // +1 for newline
                    if byte_count > MAX_READ_SIZE {
                        is_truncated = true;
                        break;
                    }
                    last_line = i + 1; // 1-indexed
                    content.push(l);
                },
                Err(err) => {
                    return Err(ToolExecutionError::io(format!("Failed to read line {}", i + 1), err));
                },
            }
        }

        let mut content = content.join("\n");
        if is_truncated {
            // Count remaining lines to report total
            let mut total_lines = last_line;
            while let Some((i, _)) = file_lines.next().await {
                total_lines = i + 1;
            }
            content.push_str(&format!(
                "\n...truncated (showing lines {}-{} of {}). Use offset: {} to continue reading.",
                start_line, last_line, total_lines, last_line
            ));
        }
        Ok(ToolExecutionOutputItem::Text(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::ToolExecutionOutputItem;
    use crate::agent::tools::fs_read::{
        FsRead,
        FsReadOperation,
    };
    use crate::util::test::TestBase;

    #[tokio::test]
    async fn test_fs_read_single_file() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "line1\nline2\nline3"))
            .await;

        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: test_base.join("test.txt").to_string_lossy().to_string(),
                limit: None,
                offset: None,
            })],
        };

        assert!(tool.validate(&test_base).await.is_ok());
        let result = tool.execute(&test_base).await.unwrap();
        assert_eq!(result.items.len(), 1);
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert_eq!(content, "line1\nline2\nline3");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_with_offset_and_limit() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "line1\nline2\nline3\nline4\nline5"))
            .await;

        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: test_base.join("test.txt").to_string_lossy().to_string(),
                limit: Some(2),
                offset: Some(1),
            })],
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert_eq!(content, "line2\nline3");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_multiple_files() {
        let test_base = TestBase::new()
            .await
            .with_file(("file1.txt", "content1"))
            .await
            .with_file(("file2.txt", "content2"))
            .await;

        let tool = FsRead {
            operations: vec![
                FsReadOperation::Line(FileOp {
                    path: test_base.join("file1.txt").to_string_lossy().to_string(),
                    limit: None,
                    offset: None,
                }),
                FsReadOperation::Line(FileOp {
                    path: test_base.join("file2.txt").to_string_lossy().to_string(),
                    limit: None,
                    offset: None,
                }),
            ],
        };

        let result = tool.execute(&test_base).await.unwrap();
        assert_eq!(result.items.len(), 2);
    }

    #[tokio::test]
    async fn test_fs_read_validate_nonexistent_file() {
        let test_base = TestBase::new().await;
        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: "/nonexistent/file.txt".to_string(),
                limit: None,
                offset: None,
            })],
        };

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_validate_directory_path() {
        let test_base = TestBase::new().await;

        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: test_base.join("").to_string_lossy().to_string(),
                limit: None,
                offset: None,
            })],
        };

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_fs_read_validate_symlink_to_file() {
        let test_base = TestBase::new().await.with_file(("target.txt", "symlink content")).await;

        tokio::fs::symlink(test_base.join("target.txt"), test_base.join("link.txt"))
            .await
            .unwrap();

        let op = FileOp {
            path: test_base.join("link.txt").to_string_lossy().to_string(),
            limit: None,
            offset: None,
        };

        assert!(op.validate(&test_base).await.is_ok());
        let result = op.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result {
            assert_eq!(content, "symlink content");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_truncation_includes_metadata() {
        let test_base = TestBase::new().await;

        // Create a file larger than MAX_READ_SIZE (250KB)
        let long_line = "x".repeat(1000);
        let lines: Vec<&str> = (0..300).map(|_| long_line.as_str()).collect();
        let large_content = lines.join("\n");

        let file_path = test_base.join("large.txt");
        tokio::fs::write(&file_path, &large_content).await.unwrap();

        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: file_path.to_string_lossy().to_string(),
                limit: None,
                offset: None,
            })],
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("showing lines"), "should include line range metadata");
            assert!(content.contains("of 300"), "should include total line count");
            assert!(content.contains("Use offset:"), "should suggest next offset");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_no_truncation_no_metadata() {
        // Small file should NOT have truncation metadata
        let test_base = TestBase::new()
            .await
            .with_file(("small.txt", "line1\nline2\nline3"))
            .await;

        let tool = FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: test_base.join("small.txt").to_string_lossy().to_string(),
                limit: None,
                offset: None,
            })],
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(!content.contains("truncated"), "small file should not be truncated");
            assert!(!content.contains("Use offset"), "small file should not suggest offset");
        } else {
            panic!("expected text output");
        }
    }
}
