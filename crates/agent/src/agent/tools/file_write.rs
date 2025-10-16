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
    ToolExecutionResult,
};
use crate::agent::util::path::canonicalize_path;

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
- Read the file first before making modifications to ensure you have the most up-to-date version of the file.
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
                "str_replace",
                "insert"
            ],
            "description": "The commands to run. Allowed options are: `create`, `str_replace`, `insert`"
        },
        "content": {
            "description": "Required parameter of `create` and `insert` commands.",
            "type": "string"
        },
        "insert_line": {
            "description": "Optional parameter of `insert` command. Line is 0-indexed. `content` will be inserted at the provided line. If not provided, content will be inserted at the end of the file on a new line, inserting a newline at the end of the file if it is missing.",
            "type": "integer"
        },
        "new_str": {
            "description": "Required parameter of `str_replace` command containing the new string.",
            "type": "string"
        },
        "old_str": {
            "description": "Required parameter of `str_replace` command containing the string in `path` to replace.",
            "type": "string"
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

    fn canonical_path(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from(
            canonicalize_path(self.path()).map_err(|e| e.to_string())?,
        ))
    }

    pub async fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();

        if self.path().is_empty() {
            errors.push("Path must not be empty".to_string());
        }

        match &self {
            FsWrite::Create(_) => (),
            FsWrite::StrReplace(_) => {
                if !self.canonical_path()?.exists() {
                    errors.push(
                        "The provided path must exist in order to replace or insert contents into it".to_string(),
                    );
                }
            },
            FsWrite::Insert(v) => {
                if v.content.is_empty() {
                    errors.push("Content to insert must not be empty".to_string());
                }
            },
        }

        if !errors.is_empty() {
            Err(errors.join("\n"))
        } else {
            Ok(())
        }
    }

    pub async fn make_context(&self) -> eyre::Result<FsWriteContext> {
        // TODO - return file diff context
        Ok(FsWriteContext {
            path: self.path().to_string(),
        })
    }

    pub async fn execute(&self, _state: Option<&mut FsWriteState>) -> ToolExecutionResult {
        let path = self.canonical_path().map_err(ToolExecutionError::Custom)?;

        match &self {
            FsWrite::Create(v) => v.execute(path).await?,
            FsWrite::StrReplace(v) => v.execute(path).await?,
            FsWrite::Insert(v) => v.execute(path).await?,
        }

        Ok(Default::default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCreate {
    path: String,
    content: String,
}

impl FileCreate {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<(), ToolExecutionError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    ToolExecutionError::io(format!("failed to create directory {}", parent.to_string_lossy()), e)
                })?;
            }
        }

        tokio::fs::write(path, &self.content)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to write to {}", path.to_string_lossy()), e))?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrReplace {
    path: String,
    old_str: String,
    new_str: String,
    replace_all: bool,
}

impl StrReplace {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<(), ToolExecutionError> {
        let path = path.as_ref();

        let file = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;

        let matches = file.match_indices(&self.old_str).collect::<Vec<_>>();
        match matches.len() {
            0 => {
                return Err(ToolExecutionError::Custom(format!(
                    "no occurrences of \"{}\" were found",
                    &self.old_str
                )));
            },
            1 => {
                let file = file.replacen(&self.old_str, &self.new_str, 1);
                tokio::fs::write(path, file)
                    .await
                    .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;
            },
            x => {
                if !self.replace_all {
                    return Err(ToolExecutionError::Custom(format!(
                        "{x} occurrences of old_str were found when only 1 is expected"
                    )));
                }
                let file = file.replace(&self.old_str, &self.new_str);
                tokio::fs::write(path, file)
                    .await
                    .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;
            },
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insert {
    path: String,
    content: String,
    insert_line: Option<u32>,
}

impl Insert {
    async fn execute(&self, path: impl AsRef<Path>) -> Result<(), ToolExecutionError> {
        let path = path.as_ref();

        let mut file = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to read {}", path.to_string_lossy()), e))?;

        let line_count = file.lines().count() as u32;

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

        tokio::fs::write(path, file)
            .await
            .map_err(|e| ToolExecutionError::io(format!("failed to write to {}", path.to_string_lossy()), e))?;

        Ok(())
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
