use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};
use syntect::util::LinesWithEndings;

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

const FS_WRITE_TOOL_DESCRIPTION: &str = r#"
A tool for creating and editing text files.

WHEN TO USE THIS TOOL:
- Use when you need to create a new file, or modify an existing file
- Perfect for updating text-based file formats

HOW TO USE:
- Provide the path to the file you want to create or modify
- Specify the operation to perform: one of `create`, `strReplace`, or `insert`
- Use `create` to create a new file. Required parameter is `content`. Parent directories will be created if they are missing.
- Use `strReplace` to replace and update the content of an existing file.
- Use `insert` to insert content at a specific line, or append content to the end of a file.

TIPS:
- To append content to the end of a file, use `insert` with no `insert_line`
"#;

const FS_WRITE_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "enum": [
                "create",
                "strReplace",
                "insert"
            ],
            "description": "The commands to run. Allowed options are: `create`, `strReplace`, `insert`"
        },
        "content": {
            "description": "Required parameter of `create` and `insert` commands.",
            "type": "string"
        },
        "insertLine": {
            "description": "Optional parameter of `insert` command. Line is 0-indexed. `content` will be inserted at the provided line. If not provided, content will be inserted at the end of the file on a new line, inserting a newline at the end of the file if it is missing.",
            "type": "integer"
        },
        "newStr": {
            "description": "Required parameter of `strReplace` command containing the new string.",
            "type": "string"
        },
        "oldStr": {
            "description": "Required parameter of `strReplace` command containing the string in `path` to replace.",
            "type": "string"
        },
        "replaceAll": {
            "description": "Optional parameter of `strReplace` command. Default is false. When true, all instances of `oldStr` will be replaced with `newStr`.",
            "type": "boolean"
        },
        "path": {
            "description": "Path to the file",
            "type": "string"
        }
    },
    "required": [
        "command",
        "path"
    ]
}
"#;

#[cfg(unix)]
const NEWLINE: &str = "\n";

#[cfg(windows)]
const NEWLINE: &str = "\r\n";

/// Normalize CRLF line endings to LF for consistent string matching.
fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n")
}

impl BuiltInToolTrait for FsWrite {
    fn name() -> BuiltInToolName {
        BuiltInToolName::FsWrite
    }

    fn description() -> std::borrow::Cow<'static, str> {
        FS_WRITE_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        FS_WRITE_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["fs_write", "write"])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "command")]
pub enum FsWrite {
    Create(FileCreate),
    StrReplace(StrReplace),
    Insert(Insert),
}

impl FsWrite {
    pub fn path(&self) -> &str {
        match self {
            FsWrite::Create(v) => &v.path,
            FsWrite::StrReplace(v) => &v.path,
            FsWrite::Insert(v) => &v.path,
        }
    }

    pub fn start_lines(&self) -> Vec<u32> {
        match self {
            FsWrite::Create(v) => v.start_line.into_iter().collect(),
            FsWrite::StrReplace(v) => v.start_lines.clone(),
            FsWrite::Insert(v) => v.start_line.into_iter().collect(),
        }
    }

    fn canonical_path<P: SystemProvider>(&self, provider: &P) -> Result<PathBuf, String> {
        Ok(PathBuf::from(
            canonicalize_path_sys(self.path(), provider).map_err(|e| e.to_string())?,
        ))
    }

    pub async fn validate<P: SystemProvider>(&mut self, provider: &P) -> Result<(), String> {
        let mut errors = Vec::new();

        if self.path().is_empty() {
            errors.push("Path must not be empty".to_string());
        }

        let path = self.canonical_path(provider)?;

        match self {
            FsWrite::Create(v) => {
                v.start_line = Some(1);
            },
            FsWrite::StrReplace(v) => {
                let old_str_normalized = normalize_line_endings(&v.old_str);
                let new_str_normalized = normalize_line_endings(&v.new_str);
                // Reject when old_str is a verbatim substring of new_str. Repeated calls with
                // this pattern silently re-match the just-written content and grow the file
                // (linearly when replace_all=false, exponentially when replace_all=true and
                // old_str appears in new_str more than once). Trim trailing newlines from
                // old_str before the check to tolerate common LLM artifacts.
                let old_str_trimmed = old_str_normalized.trim_end_matches('\n');
                if !old_str_trimmed.is_empty() && new_str_normalized.contains(old_str_trimmed) {
                    errors.push("Cannot edit file: old_str is a substring of new_str".to_string());
                }
                if !path.exists() {
                    errors.push(
                        "The provided path must exist in order to replace or insert contents into it".to_string(),
                    );
                } else if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    let normalized = normalize_line_endings(&content);
                    let matches: Vec<_> = normalized.match_indices(&old_str_normalized).collect();
                    if matches.is_empty() {
                        errors.push("The provided old_str was not found in the file".to_string());
                    } else if v.replace_all {
                        // SAFETY: byte_offset from match_indices() is always a valid char boundary
                        #[allow(clippy::string_slice)]
                        {
                            v.start_lines = matches
                                .iter()
                                .map(|(byte_offset, _)| {
                                    (normalized[..*byte_offset].lines().count() as u32).saturating_add(1)
                                })
                                .collect();
                        }
                    } else {
                        let byte_offset = matches[0].0;
                        // SAFETY: byte_offset from match_indices() is always a valid char boundary
                        #[allow(clippy::string_slice)]
                        {
                            v.start_lines = vec![(normalized[..byte_offset].lines().count() as u32).saturating_add(1)];
                        }
                    }
                }
            },
            FsWrite::Insert(v) => {
                if v.content.is_empty() {
                    errors.push("Content to insert must not be empty".to_string());
                }
                v.start_line = match v.insert_line {
                    Some(line) => Some(line.saturating_add(1)),
                    None => tokio::fs::read_to_string(&path)
                        .await
                        .ok()
                        .map(|c| (c.lines().count() as u32).saturating_add(1)),
                };
            },
        }

        if !errors.is_empty() {
            Err(errors.join("\n"))
        } else {
            Ok(())
        }
    }

    pub async fn make_context(&self) -> eyre::Result<FsWriteContext> {
        Ok(FsWriteContext {
            path: self.path().to_string(),
        })
    }

    pub async fn execute<P: SystemProvider>(
        &self,
        _state: Option<&mut FsWriteState>,
        provider: &P,
    ) -> ToolExecutionResult {
        let path = self.canonical_path(provider).map_err(ToolExecutionError::Custom)?;

        let message = match &self {
            FsWrite::Create(v) => v.execute(&path).await?,
            FsWrite::StrReplace(v) => v.execute(&path).await?,
            FsWrite::Insert(v) => v.execute(&path).await?,
        };

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(message)]))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileCreate {
    pub path: String,
    pub content: String,
    /// Starting line number (1-indexed), computed during validation
    #[serde(skip)]
    pub start_line: Option<u32>,
}

impl FileCreate {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<String, ToolExecutionError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolExecutionError::io(format!("failed to create directory {}", parent.to_string_lossy()), e)
            })?;
        }

        let line_count = self.content.lines().count();
        tokio::fs::write(path, &self.content)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to write to {}", path.to_string_lossy()), e))?;

        Ok(format!(
            "Successfully created {} ({} lines).",
            path.to_string_lossy(),
            line_count
        ))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrReplace {
    path: String,
    pub old_str: String,
    pub new_str: String,
    #[serde(default)]
    replace_all: bool,
    /// Starting line numbers (1-indexed), computed during validation
    #[serde(skip)]
    pub start_lines: Vec<u32>,
}

impl StrReplace {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<String, ToolExecutionError> {
        let path = path.as_ref();

        let file = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;

        let has_crlf = file.contains("\r\n");
        let normalized = normalize_line_endings(&file);
        let old_str_normalized = normalize_line_endings(&self.old_str);
        let new_str_normalized = normalize_line_endings(&self.new_str);

        let matches = normalized.match_indices(&old_str_normalized).collect::<Vec<_>>();
        let count = matches.len();
        let result = match count {
            0 => {
                return Err(ToolExecutionError::Custom(format!(
                    "no occurrences of \"{}\" were found",
                    &self.old_str
                )));
            },
            1 => normalized.replacen(&old_str_normalized, &new_str_normalized, 1),
            x => {
                if !self.replace_all {
                    return Err(ToolExecutionError::Custom(format!(
                        "{x} occurrences of old_str were found when only 1 is expected"
                    )));
                }
                normalized.replace(&old_str_normalized, &new_str_normalized)
            },
        };

        let output = if has_crlf { result.replace('\n', "\r\n") } else { result };

        tokio::fs::write(path, output)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to write {}", path.to_string_lossy()), e))?;

        Ok(format!(
            "Successfully replaced {} occurrence(s) in {}.",
            count,
            path.to_string_lossy()
        ))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insert {
    path: String,
    content: String,
    insert_line: Option<u32>,
    /// Starting line number (1-indexed), computed during validation
    #[serde(skip)]
    pub start_line: Option<u32>,
}

impl Insert {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<String, ToolExecutionError> {
        let path = path.as_ref();

        let mut file = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;

        let line_count = file.lines().count() as u32;
        let inserted_lines = self.content.lines().count();

        if let Some(insert_line) = self.insert_line {
            let insert_line = insert_line.clamp(0, line_count);

            // Get the index to insert at.
            let mut i = 0;
            for line in LinesWithEndings::from(&file).take(insert_line as usize) {
                i += line.len();
            }

            let mut content = self.content.clone();
            if !content.ends_with(NEWLINE) {
                content.push_str(NEWLINE);
            }
            file.insert_str(i, &content);
        } else {
            if !file.ends_with(NEWLINE) {
                file.push_str(NEWLINE);
            }
            file.push_str(&self.content);
        }

        tokio::fs::write(path, &file)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to write to {}", path.to_string_lossy()), e))?;

        let location = match self.insert_line {
            Some(line) => format!("at line {}", line),
            None => "at end of file".to_string(),
        };
        Ok(format!(
            "Successfully inserted {} line(s) {} in {}.",
            inserted_lines,
            location,
            path.to_string_lossy()
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteContext {
    path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteState {
    pub line_tracker: FileLineTracker,
}

/// Contains metadata for tracking user and agent contribution metrics for a given file for
/// `fs_write` tool uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLineTracker {
    /// Line count at the end of the last `fs_write`
    pub prev_fswrite_lines: usize,
    /// Line count before `fs_write` executes
    pub before_fswrite_lines: usize,
    /// Line count after `fs_write` executes
    pub after_fswrite_lines: usize,
    /// Lines added by agent in the current operation
    pub lines_added_by_agent: usize,
    /// Lines removed by agent in the current operation
    pub lines_removed_by_agent: usize,
    /// Whether or not this is the first `fs_write` invocation
    pub is_first_write: bool,
}

impl Default for FileLineTracker {
    fn default() -> Self {
        Self {
            prev_fswrite_lines: 0,
            before_fswrite_lines: 0,
            after_fswrite_lines: 0,
            lines_added_by_agent: 0,
            lines_removed_by_agent: 0,
            is_first_write: true,
        }
    }
}

impl FileLineTracker {
    pub fn lines_by_user(&self) -> isize {
        (self.before_fswrite_lines as isize) - (self.prev_fswrite_lines as isize)
    }

    pub fn lines_by_agent(&self) -> isize {
        (self.lines_added_by_agent + self.lines_removed_by_agent) as isize
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test::TestBase;

    #[tokio::test]
    async fn test_validate_sets_start_line_str_replace() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "first\nsecond\nthird"))
            .await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "second".to_string(),
            new_str: "replaced".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert_eq!(tool.start_lines(), vec![2]); // "second" is on line 2
    }

    #[tokio::test]
    async fn test_validate_str_replace_old_str_not_found() {
        let test_base = TestBase::new().await.with_file(("test.txt", "hello world")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "nonexistent".to_string(),
            new_str: "replaced".to_string(),
            ..Default::default()
        });

        let result = tool.validate(&test_base).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("old_str was not found"));
    }

    #[tokio::test]
    async fn test_validate_sets_start_line_create() {
        let test_base = TestBase::new().await;

        let mut tool = FsWrite::Create(FileCreate {
            path: test_base.join("new.txt").to_string_lossy().to_string(),
            content: "hello world".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert_eq!(tool.start_lines(), vec![1]);
    }

    #[tokio::test]
    async fn test_validate_sets_start_line_insert() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "line1\nline2\nline3"))
            .await;

        let mut tool = FsWrite::Insert(Insert {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            content: "inserted".to_string(),
            insert_line: Some(2),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert_eq!(tool.start_lines(), vec![3]); // Inserted content starts at line 3
    }

    #[tokio::test]
    async fn test_validate_sets_start_line_insert_append() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "line1\nline2\nline3"))
            .await;

        let mut tool = FsWrite::Insert(Insert {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            content: "appended".to_string(),
            insert_line: None,
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert_eq!(tool.start_lines(), vec![4]); // Appended content starts at line 4
    }

    #[tokio::test]
    async fn test_create_file() {
        let test_base = TestBase::new().await;
        let mut tool = FsWrite::Create(FileCreate {
            path: test_base.join("new.txt").to_string_lossy().to_string(),
            content: "hello world".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("new.txt")).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_create_file_with_parent_dirs() {
        let test_base = TestBase::new().await;
        let tool = FsWrite::Create(FileCreate {
            path: test_base.join("nested/dir/file.txt").to_string_lossy().to_string(),
            content: "nested content".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("nested/dir/file.txt"))
            .await
            .unwrap();
        assert_eq!(content, "nested content");
    }

    #[tokio::test]
    async fn test_str_replace_single_occurrence() {
        let test_base = TestBase::new().await.with_file(("test.txt", "hello world")).await;

        let tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "world".to_string(),
            new_str: "rust".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("test.txt")).await.unwrap();
        assert_eq!(content, "hello rust");
    }

    #[tokio::test]
    async fn test_str_replace_multiple_occurrences() {
        let test_base = TestBase::new().await.with_file(("test.txt", "foo bar foo")).await;

        let tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "foo".to_string(),
            new_str: "baz".to_string(),
            replace_all: true,
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("test.txt")).await.unwrap();
        assert_eq!(content, "baz bar baz");
    }

    #[tokio::test]
    async fn test_str_replace_no_match() {
        let test_base = TestBase::new().await.with_file(("test.txt", "hello world")).await;

        let tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "missing".to_string(),
            new_str: "replacement".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_insert_at_line() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", format!("line1{NEWLINE}line2{NEWLINE}line3")))
            .await;

        let tool = FsWrite::Insert(Insert {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            content: "inserted".to_string(),
            insert_line: Some(1),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("test.txt")).await.unwrap();
        assert_eq!(content, format!("line1{NEWLINE}inserted{NEWLINE}line2{NEWLINE}line3"));
    }

    #[tokio::test]
    async fn test_insert_append() {
        let test_base = TestBase::new().await.with_file(("test.txt", "existing")).await;

        let tool = FsWrite::Insert(Insert {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            content: "appended".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());

        let content = tokio::fs::read_to_string(test_base.join("test.txt")).await.unwrap();
        assert_eq!(content, format!("existing{NEWLINE}appended"));
    }

    #[tokio::test]
    async fn test_fs_write_validate_empty_path() {
        let test_base = TestBase::new().await;
        let mut tool = FsWrite::Create(FileCreate {
            path: "".to_string(),
            content: "content".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_fs_write_validate_nonexistent_file_for_replace() {
        let test_base = TestBase::new().await;
        let mut tool = FsWrite::StrReplace(StrReplace {
            path: "/nonexistent/file.txt".to_string(),
            old_str: "old".to_string(),
            new_str: "new".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_str_replace_crlf_file_with_lf_old_str() {
        let test_base = TestBase::new().await;
        let path = test_base.join("crlf.txt");
        tokio::fs::write(&path, "hello\r\nworld\r\nfoo").await.unwrap();

        let tool = FsWrite::StrReplace(StrReplace {
            path: path.to_string_lossy().to_string(),
            old_str: "world".to_string(),
            new_str: "rust".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello\r\nrust\r\nfoo");
    }

    #[tokio::test]
    async fn test_str_replace_crlf_file_preserves_crlf() {
        let test_base = TestBase::new().await;
        let path = test_base.join("crlf.txt");
        tokio::fs::write(&path, "line1\r\nline2\r\nline3\r\n").await.unwrap();

        let tool = FsWrite::StrReplace(StrReplace {
            path: path.to_string_lossy().to_string(),
            old_str: "line2".to_string(),
            new_str: "replaced".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "line1\r\nreplaced\r\nline3\r\n");
    }

    #[tokio::test]
    async fn test_str_replace_crlf_multiline_old_str() {
        let test_base = TestBase::new().await;
        let path = test_base.join("crlf.txt");
        tokio::fs::write(&path, "aaa\r\nbbb\r\nccc\r\n").await.unwrap();

        let tool = FsWrite::StrReplace(StrReplace {
            path: path.to_string_lossy().to_string(),
            old_str: "aaa\nbbb".to_string(),
            new_str: "xxx\nyyy".to_string(),
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "xxx\r\nyyy\r\nccc\r\n");
    }

    #[tokio::test]
    async fn test_validate_crlf_file_with_lf_old_str() {
        let test_base = TestBase::new().await;
        let path = test_base.join("crlf.txt");
        tokio::fs::write(&path, "first\r\nsecond\r\nthird").await.unwrap();

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: path.to_string_lossy().to_string(),
            old_str: "second".to_string(),
            new_str: "replaced".to_string(),
            ..Default::default()
        });

        assert!(tool.validate(&test_base).await.is_ok());
        assert_eq!(tool.start_lines(), vec![2]);
    }

    #[tokio::test]
    async fn test_str_replace_crlf_replace_all() {
        let test_base = TestBase::new().await;
        let path = test_base.join("crlf.txt");
        tokio::fs::write(&path, "foo\r\nbar\r\nfoo\r\n").await.unwrap();

        let tool = FsWrite::StrReplace(StrReplace {
            path: path.to_string_lossy().to_string(),
            old_str: "foo".to_string(),
            new_str: "baz".to_string(),
            replace_all: true,
            ..Default::default()
        });

        assert!(tool.execute(None, &test_base).await.is_ok());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "baz\r\nbar\r\nbaz\r\n");
    }

    #[tokio::test]
    async fn test_validate_rejects_old_str_substring_of_new_str() {
        let test_base = TestBase::new().await.with_file(("test.txt", "L5 → L6 promotion")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "L5 → L6 promotion".to_string(),
            new_str: "L5 → L6 promotion readiness evaluation — not a promotion".to_string(),
            ..Default::default()
        });

        let err = tool.validate(&test_base).await.unwrap_err();
        assert!(
            err.contains("old_str is a substring of new_str"),
            "expected substring-containment error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_rejects_old_str_substring_with_replace_all() {
        let test_base = TestBase::new().await.with_file(("test.txt", "foo bar foo")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "foo".to_string(),
            new_str: "wrap foo wrap".to_string(),
            replace_all: true,
            ..Default::default()
        });

        let err = tool.validate(&test_base).await.unwrap_err();
        assert!(
            err.contains("old_str is a substring of new_str"),
            "replace_all=true with substring containment must also be rejected, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_rejects_old_str_equals_new_str() {
        let test_base = TestBase::new().await.with_file(("test.txt", "hello")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "hello".to_string(),
            new_str: "hello".to_string(),
            ..Default::default()
        });

        let err = tool.validate(&test_base).await.unwrap_err();
        assert!(
            err.contains("old_str is a substring of new_str"),
            "no-op replace (old_str == new_str) must be rejected, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_rejects_substring_after_crlf_normalization() {
        // old_str uses LF, new_str uses CRLF — after normalization, old_str is contained.
        let test_base = TestBase::new().await.with_file(("test.txt", "foo\nbar")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "foo\nbar".to_string(),
            new_str: "foo\r\nbar\r\nbaz".to_string(),
            ..Default::default()
        });

        let err = tool.validate(&test_base).await.unwrap_err();
        assert!(
            err.contains("old_str is a substring of new_str"),
            "CRLF-normalized substring containment must be rejected, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_rejects_substring_with_trailing_newline_in_old_str() {
        // old_str has a trailing newline that is stripped before the substring check.
        let test_base = TestBase::new().await.with_file(("test.txt", "section\n")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "section\n".to_string(),
            new_str: "section header\n".to_string(),
            ..Default::default()
        });

        let err = tool.validate(&test_base).await.unwrap_err();
        assert!(
            err.contains("old_str is a substring of new_str"),
            "trailing-newline old_str must still be detected as substring, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_allows_disjoint_old_and_new_str() {
        let test_base = TestBase::new().await.with_file(("test.txt", "fn old_name() {}")).await;

        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("test.txt").to_string_lossy().to_string(),
            old_str: "fn old_name()".to_string(),
            new_str: "fn new_name()".to_string(),
            ..Default::default()
        });

        assert!(
            tool.validate(&test_base).await.is_ok(),
            "disjoint old_str and new_str must be allowed"
        );
    }

    /// "Wrap" patterns are a legitimate use case (e.g., wrapping a return value
    /// in `Ok(...)`, wrapping a function call with retry logic). They are
    /// supported when `old_str` includes enough surrounding context that the
    /// substring relationship breaks. This test documents that the canonical
    /// well-formed wrap pattern passes validation.
    #[tokio::test]
    async fn test_validate_allows_wrap_with_context() {
        let test_base = TestBase::new()
            .await
            .with_file(("lib.rs", "fn handler() {\n    return Some(value);\n}\n"))
            .await;

        // Wrap `Some(value)` in `Ok(...)`. Including the leading whitespace and
        // `return ` prefix makes `old_str` not appear in `new_str` (the bytes
        // after `return ` differ), so the substring guard passes.
        let mut tool = FsWrite::StrReplace(StrReplace {
            path: test_base.join("lib.rs").to_string_lossy().to_string(),
            old_str: "    return Some(value);".to_string(),
            new_str: "    return Ok(Some(value));".to_string(),
            ..Default::default()
        });

        assert!(
            tool.validate(&test_base).await.is_ok(),
            "wrap pattern with sufficient surrounding context must be allowed"
        );
    }

    /// Regression test for ticket P431388657.
    ///
    /// Demonstrates both halves of the bug story in one place:
    /// 1. `validate()` rejects the substring-containment pattern (the fix).
    /// 2. If `validate()` is bypassed, repeated `execute()` calls produce exponential file growth
    ///    (the bug at the raw-operation layer, justifying why the guard belongs in `validate()`).
    ///
    /// With `replace_all=true` and `old_str` appearing twice in `new_str`,
    /// each invocation doubles the count of matches → file grows ~2^N after
    /// N invocations. Five iterations produce ≥16× growth.
    #[tokio::test]
    async fn test_substring_cascade_repro_and_fix() {
        let test_base = TestBase::new()
            .await
            .with_file(("evidence.md", "L5 → L6 promotion review"))
            .await;
        let path_str = test_base.join("evidence.md").to_string_lossy().to_string();

        // new_str contains old_str TWICE → with replace_all=true, each call
        // multiplies the match count by 2. This is the catastrophic variant.
        let make_tool = || {
            FsWrite::StrReplace(StrReplace {
                path: path_str.clone(),
                old_str: "L5 → L6 promotion".to_string(),
                new_str: "L5 → L6 promotion readiness — not a L5 → L6 promotion".to_string(),
                replace_all: true,
                ..Default::default()
            })
        };

        // --- Part 1: the fix ---
        let mut tool = make_tool();
        let err = tool
            .validate(&test_base)
            .await
            .expect_err("validate() must reject old_str ⊂ new_str");
        assert!(
            err.contains("old_str is a substring of new_str"),
            "unexpected error: {err}"
        );

        // The file must be untouched after a rejected validate().
        let after_validate = tokio::fs::read_to_string(test_base.join("evidence.md")).await.unwrap();
        assert_eq!(after_validate, "L5 → L6 promotion review");

        // --- Part 2: the bug at the execute() layer ---
        // Bypass validate() and drive execute() directly to prove the cascade
        // exists at the raw-operation layer. This is what justifies placing
        // the guard in validate().
        let initial_size = after_validate.len();
        let initial_occurrences = after_validate.matches("L5 → L6 promotion").count();
        let tool = make_tool();
        for _ in 0..5 {
            tool.execute(None, &test_base)
                .await
                .expect("execute() does not itself check for substring containment");
        }
        let final_content = tokio::fs::read_to_string(test_base.join("evidence.md")).await.unwrap();
        let final_size = final_content.len();
        let final_occurrences = final_content.matches("L5 → L6 promotion").count();

        // Each invocation doubles occurrences: 1 → 2 → 4 → 8 → 16 → 32.
        assert_eq!(
            final_occurrences,
            initial_occurrences * 32,
            "expected 2^5 = 32x match-count growth, got {initial_occurrences} → {final_occurrences}"
        );
        assert!(
            final_size >= initial_size * 16,
            "expected exponential byte growth without the validate() guard: \
             initial={initial_size} bytes, final={final_size} bytes"
        );
    }
}
