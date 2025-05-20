use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    ContextCompat as _,
    Result,
    bail,
    eyre,
};
use serde::{
    Deserialize,
    Serialize,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{
    LinesWithEndings,
    as_24_bit_terminal_escaped,
};
use tracing::error;

use super::{
    InvokeOutput,
    OutputKind,
    format_path,
    sanitize_path_tool_arg,
    supports_truecolor,
};
use crate::platform::Context;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// File system write operations with batch support
#[derive(Debug, Clone, Deserialize)]
pub struct FsWrite {
    pub file_edits: Vec<FileWithEdits>,
    pub summary: Option<String>,
}

/// Represents a file with multiple edits
#[derive(Debug, Clone, Deserialize)]
pub struct FileWithEdits {
    pub path: String,
    pub edits: Vec<FileEdit>,
}

/// Represents a single edit operation in a batch
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum FileEdit {
    #[serde(rename = "create")]
    Create {
        file_text: Option<String>,
        new_str: Option<String>,
    },
    #[serde(rename = "rewrite")]
    Rewrite {
        file_text: Option<String>,
        new_str: Option<String>,
    },
    #[serde(rename = "str_replace")]
    StrReplace { old_str: String, new_str: String },
    #[serde(rename = "insert")]
    Insert { insert_line: usize, new_str: String },
    #[serde(rename = "append")]
    Append { new_str: String },
    #[serde(rename = "replace_lines")]
    ReplaceLines {
        start_line: usize,
        end_line: usize,
        new_str: String,
    },
    #[serde(rename = "delete_lines")]
    DeleteLines { start_line: usize, end_line: usize },
}

/// Response for a single file write operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteResult {
    pub path: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits_applied: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits_failed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful_edits: Option<Vec<EditResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_edits: Option<Vec<FailedEdit>>,
}

/// Represents a failed edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedEdit {
    pub command: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub command: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWriteResult {
    pub total_files: usize,
    pub files_modified: usize,
    pub files_failed: usize,
    pub total_edits_applied: usize,
    pub total_edits_failed: usize,
    pub file_results: Vec<FileWriteResult>,
}

impl FileWriteResult {
    /// Create a new error FileWriteResult
    pub fn error(path: String, error: String) -> Self {
        Self {
            path,
            success: false,
            error: Some(error),
            edits_applied: Some(0),
            edits_failed: None,
            successful_edits: Some(Vec::new()),
            failed_edits: None,
        }
    }

    /// Add a failed edit to the result
    pub fn add_failed_edit(&mut self, command: String, error: String) {
        let failed_edit = FailedEdit { command, error };

        if let Some(failed_edits) = &mut self.failed_edits {
            failed_edits.push(failed_edit);
        } else {
            self.failed_edits = Some(vec![failed_edit]);
        }

        if let Some(edits_failed) = &mut self.edits_failed {
            *edits_failed += 1;
        } else {
            self.edits_failed = Some(1);
        }
    }
}

impl FsWrite {
    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let _fs = ctx.fs();
        let cwd = ctx.env().current_dir()?;

        let mut file_results = Vec::new();
        let mut total_edits_applied = 0;
        let mut total_edits_failed = 0;
        let mut files_modified = 0;
        let mut files_failed = 0;

        // Process each file with its edits
        for file_with_edits in &self.file_edits {
            let path = sanitize_path_tool_arg(ctx, &file_with_edits.path);
            let path_str = file_with_edits.path.clone();
            let relative_path = format_path(&cwd, &path);

            // Check if file exists (except for create operations)
            if !path.exists()
                && !file_with_edits
                    .edits
                    .iter()
                    .any(|e| matches!(e, FileEdit::Create { .. }))
            {
                queue_formatted_message(updates, "File not found: ", &relative_path, Color::Red)?;
                file_results.push(FileWriteResult::error(path_str, "File not found".to_string()));
                files_failed += 1;
                continue;
            }

            // Sort edits by type and position to avoid line number issues
            // Order: Create/Rewrite first, line-based operations from bottom to top, string operations, append
            // last
            let mut edits = file_with_edits.edits.clone();
            edits.sort_by(|a, b| {
                // First sort by operation type priority
                let a_priority = get_operation_priority(a);
                let b_priority = get_operation_priority(b);

                if a_priority != b_priority {
                    return a_priority.cmp(&b_priority);
                }

                // For line-based operations, sort from bottom to top
                match (a, b) {
                    // ReplaceLines and DeleteLines: sort by start_line in reverse order
                    (
                        FileEdit::ReplaceLines {
                            start_line: a_start, ..
                        },
                        FileEdit::ReplaceLines {
                            start_line: b_start, ..
                        },
                    ) => {
                        b_start.cmp(a_start) // Reverse order (highest line number first)
                    },
                    (
                        FileEdit::DeleteLines {
                            start_line: a_start, ..
                        },
                        FileEdit::DeleteLines {
                            start_line: b_start, ..
                        },
                    ) => {
                        b_start.cmp(a_start) // Reverse order (highest line number first)
                    },
                    // Insert: sort by insert_line in reverse order
                    (
                        FileEdit::Insert {
                            insert_line: a_line, ..
                        },
                        FileEdit::Insert {
                            insert_line: b_line, ..
                        },
                    ) => {
                        b_line.cmp(a_line) // Reverse order (highest line number first)
                    },
                    // Default case: maintain original order
                    _ => std::cmp::Ordering::Equal,
                }
            });

            // Apply each edit
            let mut success_count = 0;
            let mut result = FileWriteResult {
                path: path_str,
                success: true,
                error: None,
                edits_applied: Some(0),
                edits_failed: Some(0),
                successful_edits: Some(Vec::new()),
                failed_edits: None,
            };

            for edit in edits {
                match apply_edit_with_diff(ctx, &path, &edit, &relative_path, updates).await {
                    Ok(details) => {
                        success_count += 1;
                        if let Some(count) = &mut result.edits_applied {
                            *count += 1;
                        }

                        let command = get_command_name(&edit);

                        if let Some(successful_edits) = &mut result.successful_edits {
                            successful_edits.push(EditResult {
                                command: command.to_string(),
                                details,
                            });
                        }
                    },
                    Err(e) => {
                        let command = get_command_name(&edit);

                        result.success = false;
                        result.add_failed_edit(command.to_string(), e.to_string());

                        super::queue_function_result(
                            &format!("Error applying edit: {}: {}", command, e),
                            updates,
                            true,
                            false,
                        )?;
                    },
                }
            }

            // If any edits succeeded, count as modified; if all failed, count as failed
            if success_count > 0 {
                files_modified += 1;
                // Still mark the file as successful overall if at least one edit worked
                result.success = true;
            } else if result.edits_failed.unwrap_or(0) > 0 {
                result.error = Some("All edits failed".to_string());
                files_failed += 1;
                result.success = false;
            }

            total_edits_applied += result.edits_applied.unwrap_or(0);
            total_edits_failed += result.edits_failed.unwrap_or(0);

            file_results.push(result);
        }

        // Add summary at the end using consistent formatting
        let total_files = self.file_edits.len();

        // Format the summary header with consistent styling
        super::queue_function_result("Summary:", updates, false, true)?;

        if total_files > 1 {
            // Format the files modified message with consistent styling
            super::queue_function_result(
                &format!("Files modified: {}/{}", files_modified, total_files),
                updates,
                false,
                true,
            )?;

            if files_failed > 0 {
                // Format the files with errors message with consistent styling
                super::queue_function_result(&format!("Files with errors: {}", files_failed), updates, true, true)?;
            }
        }

        if total_edits_applied > 0 {
            // Format the edits applied message with consistent styling
            super::queue_function_result(&format!("Edits applied: {}", total_edits_applied), updates, false, true)?;
        }

        if total_edits_failed > 0 {
            // Format the edits failed message with consistent styling
            super::queue_function_result(&format!("Edits failed: {}", total_edits_failed), updates, true, true)?;
        }
        // Create a single result object instead of a vector
        let batch_result = BatchWriteResult {
            total_files: self.file_edits.len(),
            files_modified,
            files_failed,
            total_edits_applied,
            total_edits_failed,
            file_results,
        };

        // Return the results as JSON
        let json_result = serde_json::to_string(&batch_result)?;
        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::from_str(&json_result)?),
        })
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        if self.file_edits.is_empty() {
            bail!("file_edits must not be empty");
        }

        for file_with_edits in &self.file_edits {
            if file_with_edits.edits.is_empty() {
                bail!("Each file must have at least one edit");
            }

            // Validate each edit
            for edit in &file_with_edits.edits {
                match edit {
                    FileEdit::Create { file_text, new_str } => {
                        if file_text.is_none() && new_str.is_none() {
                            bail!("Create operation must provide either file_text or new_str");
                        }
                    },
                    FileEdit::Rewrite { file_text, new_str } => {
                        if file_text.is_none() && new_str.is_none() {
                            bail!("Rewrite operation must provide either file_text or new_str");
                        }
                    },
                    FileEdit::StrReplace { old_str, .. } => {
                        if old_str.is_empty() {
                            bail!("old_str must not be empty for str_replace operation");
                        }
                    },
                    FileEdit::ReplaceLines {
                        start_line,
                        end_line,
                        new_str,
                    } => {
                        if start_line > end_line {
                            bail!("start_line must be less than or equal to end_line");
                        }
                        if new_str.is_empty() {
                            bail!("new_str must not be empty for replace_lines operation");
                        }
                    },
                    FileEdit::DeleteLines { start_line, end_line } => {
                        if start_line > end_line {
                            bail!("start_line must be less than or equal to end_line");
                        }
                    },
                    FileEdit::Append { new_str } => {
                        if new_str.is_empty() {
                            bail!("new_str must not be empty for append operation");
                        }
                    },
                    FileEdit::Insert { new_str, .. } => {
                        if new_str.is_empty() {
                            bail!("new_str must not be empty for insert operation");
                        }
                    },
                }
            }
        }

        Ok(())
    }

    pub fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let cwd = ctx.env().current_dir()?;

        queue!(
            updates,
            style::Print("Batch file operation: "),
            style::SetForegroundColor(Color::Green),
            style::Print(format!("{} files", self.file_edits.len())),
            style::ResetColor,
            style::Print("\n"),
        )?;

        // Add the summary if available
        super::queue_summary(self.summary.as_deref(), updates, Some(2))?;

        queue!(updates, style::Print("\n\n"))?;

        // Display a summary of each file and its edits with diffs
        for file_with_edits in &self.file_edits {
            let path_str = &file_with_edits.path;
            let path = sanitize_path_tool_arg(ctx, path_str);
            let relative_path = format_path(&cwd, &path);

            queue!(
                updates,
                style::Print("File: "),
                style::SetForegroundColor(Color::Green),
                style::Print(&relative_path),
                style::ResetColor,
                style::Print("\n"),
            )?;

            // Display each edit with diff
            for (i, edit) in file_with_edits.edits.iter().enumerate() {
                queue!(updates, style::Print(format!("  Edit {}: ", i + 1)),)?;

                match edit {
                    FileEdit::Create { file_text, new_str } => {
                        queue!(updates, style::Print("Create file\n"),)?;

                        let content = match (file_text, new_str) {
                            (Some(text), _) => text.clone(),
                            (None, Some(text)) => text.clone(),
                            _ => String::new(),
                        };

                        let stylized = stylize_output_if_able(ctx, &path, &content);
                        print_diff(updates, &StylizedFile::default(), &stylized, 1)?;
                    },
                    FileEdit::Rewrite { file_text, new_str } => {
                        queue!(updates, style::Print("Rewrite file\n"),)?;

                        let content = match (file_text, new_str) {
                            (Some(text), _) => text.clone(),
                            (None, Some(text)) => text.clone(),
                            _ => String::new(),
                        };

                        let old_content = if path.exists() {
                            ctx.fs().read_to_string_sync(&path).unwrap_or_default()
                        } else {
                            String::new()
                        };

                        let old_stylized = stylize_output_if_able(ctx, &path, &old_content);
                        let new_stylized = stylize_output_if_able(ctx, &path, &content);
                        print_diff(updates, &old_stylized, &new_stylized, 1)?;
                    },
                    FileEdit::StrReplace { old_str, new_str } => {
                        queue!(updates, style::Print("Replace text\n"),)?;

                        if path.exists() {
                            let file = ctx.fs().read_to_string_sync(&path)?;
                            let (start_line, _) = line_number_at(&file, old_str).unwrap_or((1, 1));

                            let old_stylized = stylize_output_if_able(ctx, &path, old_str);
                            let new_stylized = stylize_output_if_able(ctx, &path, new_str);
                            print_diff(updates, &old_stylized, &new_stylized, start_line)?;
                        }
                    },
                    FileEdit::Insert { insert_line, new_str } => {
                        queue!(updates, style::Print(format!("Insert at line {}\n", insert_line)),)?;

                        if path.exists() {
                            let file = ctx.fs().read_to_string_sync(&path)?;
                            let (prefix, start_line, suffix, _) =
                                get_lines_with_context(&file, *insert_line, *insert_line, 3);
                            let insert_line_content = LinesWithEndings::from(&file)
                                .nth(insert_line.checked_sub(1).unwrap_or(usize::MAX))
                                .unwrap_or_default();

                            let old = [prefix, insert_line_content, suffix].join("");
                            let new = [prefix, insert_line_content, new_str, suffix].join("");

                            let old_stylized = stylize_output_if_able(ctx, &path, &old);
                            let new_stylized = stylize_output_if_able(ctx, &path, &new);
                            print_diff(updates, &old_stylized, &new_stylized, start_line)?;
                        }
                    },
                    FileEdit::Append { new_str } => {
                        queue!(updates, style::Print("Append to file\n"),)?;

                        if path.exists() {
                            let file = ctx.fs().read_to_string_sync(&path)?;
                            let start_line = file.lines().count() + 1;
                            let new_stylized = stylize_output_if_able(ctx, &path, new_str);
                            print_diff(updates, &StylizedFile::default(), &new_stylized, start_line)?;
                        }
                    },
                    FileEdit::ReplaceLines {
                        start_line,
                        end_line,
                        new_str,
                    } => {
                        queue!(
                            updates,
                            style::Print(format!("Replace lines {} to {}\n", start_line, end_line)),
                        )?;

                        if path.exists() {
                            let file = ctx.fs().read_to_string_sync(&path)?;
                            let lines: Vec<&str> = file.lines().collect();

                            if *start_line <= lines.len() {
                                let old_content = build_content_from_lines(&lines, *start_line, *end_line);

                                let old_stylized = stylize_output_if_able(ctx, &path, &old_content);
                                let new_stylized = stylize_output_if_able(ctx, &path, new_str);
                                print_diff(updates, &old_stylized, &new_stylized, *start_line)?;
                            }
                        }
                    },
                    FileEdit::DeleteLines { start_line, end_line } => {
                        queue!(
                            updates,
                            style::Print(format!("Delete lines {} to {}\n", start_line, end_line)),
                        )?;

                        if path.exists() {
                            let file = ctx.fs().read_to_string_sync(&path)?;
                            let lines: Vec<&str> = file.lines().collect();

                            if *start_line <= lines.len() {
                                let old_content = build_content_from_lines(&lines, *start_line, *end_line);

                                let old_stylized = stylize_output_if_able(ctx, &path, &old_content);
                                print_diff(updates, &old_stylized, &StylizedFile::default(), *start_line)?;
                            }
                        }
                    },
                }

                queue!(updates, style::Print("\n"))?;
            }

            queue!(updates, style::Print("\n"))?;
        }

        Ok(())
    }
}
/// Writes `content` to `path`, adding a newline if necessary.
/// Also creates parent directories if they don't exist.
async fn write_to_file(ctx: &Context, path: impl AsRef<Path>, mut content: String) -> Result<()> {
    // Ensure parent directories exist
    if let Some(parent) = path.as_ref().parent() {
        ctx.fs().create_dir_all(parent).await?;
    }

    // Add newline if needed
    if !content.ends_with_newline() {
        content.push('\n');
    }

    // Write content to file
    ctx.fs().write(path.as_ref(), content).await?;
    Ok(())
}

/// Returns true if the string ends with a newline character.
trait EndsWithNewline {
    fn ends_with_newline(&self) -> bool;
}

impl EndsWithNewline for str {
    fn ends_with_newline(&self) -> bool {
        self.ends_with('\n')
    }
}

impl EndsWithNewline for String {
    fn ends_with_newline(&self) -> bool {
        self.ends_with('\n')
    }
}

/// Helper function to handle file creation and rewrite operations
///
/// - `check_exists`: If true, will fail if file already exists (for Create operation)
/// - Returns a message describing the operation result
async fn handle_file_creation(
    ctx: &Context,
    path: &Path,
    file_text: &Option<String>,
    new_str: &Option<String>,
    check_exists: bool,
) -> Result<String> {
    // Check if file exists for Create operation
    if check_exists && ctx.fs().exists(path) {
        bail!("File already exists. Use 'rewrite' command to override existing files.");
    }

    // Get content from either file_text or new_str
    let content = match (file_text, new_str) {
        (Some(text), _) => text.clone(),
        (None, Some(text)) => text.clone(),
        _ => String::new(),
    };

    // Write the content to the file
    write_to_file(ctx, path, content.clone()).await?;

    // Return success message
    let operation = if check_exists { "Created" } else { "Rewrote" };
    Ok(format!("{} file with {} bytes", operation, content.len()))
}

/// Helper function for string operations (StrReplace, Insert, Append)
///
/// - `operation`: Type of operation ("replace", "insert", "append")
/// - `old_str`: String to replace (for StrReplace only)
/// - `new_str`: New content to add
/// - `insert_line`: Line number to insert at (for Insert only)
/// - Returns a message describing the operation result
async fn handle_string_operation(
    ctx: &Context,
    path: &Path,
    operation: &str,
    old_str: Option<&str>,
    new_str: &str,
    insert_line: Option<usize>,
) -> Result<String> {
    let mut file = ctx.fs().read_to_string(path).await?;

    match operation {
        "replace" => {
            let old_str = old_str.ok_or_else(|| eyre!("old_str is required for replace operation"))?;
            let matches = file.match_indices(old_str).collect::<Vec<_>>();

            match matches.len() {
                0 => bail!("no occurrences of \"{}\" were found", old_str),
                1 => {
                    file = file.replacen(old_str, new_str, 1);
                    write_to_file(ctx, path, file).await?;
                    Ok(format!("Replaced {} characters with {}", old_str.len(), new_str.len()))
                },
                x => bail!("{x} occurrences of old_str were found when only 1 is expected"),
            }
        },
        "insert" => {
            let insert_line = insert_line.ok_or_else(|| eyre!("insert_line is required for insert operation"))?;

            // Get the index of the start of the line to insert at
            let num_lines = file.lines().enumerate().map(|(i, _)| i + 1).last().unwrap_or(1);
            let insert_line = insert_line.clamp(0, num_lines);
            let mut i = 0;
            for _ in 0..insert_line {
                let line_len = file[i..].find('\n').map_or(file[i..].len(), |j| j + 1);
                i += line_len;
            }
            file.insert_str(i, new_str);
            write_to_file(ctx, path, file).await?;

            Ok(format!("Inserted {} characters at line {}", new_str.len(), insert_line))
        },
        "append" => {
            if !file.ends_with_newline() {
                file.push('\n');
            }
            file.push_str(new_str);
            write_to_file(ctx, path, file).await?;

            Ok(format!("Appended {} characters", new_str.len()))
        },
        _ => bail!("Unknown string operation: {}", operation),
    }
}

/// Helper function to handle line modification operations (ReplaceLines and DeleteLines)
///
/// - `start_line`: 1-indexed starting line number
/// - `end_line`: 1-indexed ending line number
/// - `replacement`: Optional replacement content (None for delete operations)
/// - Returns a message describing the operation result
async fn handle_line_modification(
    ctx: &Context,
    path: &Path,
    start_line: usize,
    end_line: usize,
    replacement: Option<&str>,
) -> Result<String> {
    let file = ctx.fs().read_to_string(path).await?;

    // Convert to 0-based indexing
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.saturating_sub(1);

    // Split the file into lines
    let lines: Vec<&str> = file.lines().collect();

    // Validate line numbers
    if start_idx >= lines.len() {
        bail!("start_line is beyond the end of the file");
    }

    // Build the new file content
    let mut new_content = String::new();

    // Add lines before the modification
    for line in lines.iter().take(start_idx) {
        new_content.push_str(line);
        new_content.push('\n');
    }

    // Add the replacement content if provided
    if let Some(content) = replacement {
        new_content.push_str(content);
        if !content.ends_with_newline() {
            new_content.push('\n');
        }
    }

    // Add lines after the modification
    let end_idx = end_idx.min(lines.len() - 1);
    for line in lines.iter().skip(end_idx + 1) {
        new_content.push_str(line);
        new_content.push('\n');
    }

    // Write the new content to the file
    write_to_file(ctx, path, new_content).await?;

    // Return success message
    let operation = if replacement.is_some() { "Replaced" } else { "Deleted" };
    let lines_modified = end_idx - start_idx + 1;

    if replacement.is_some() {
        Ok(format!(
            "{} lines {} to {} with {} characters",
            operation,
            start_line,
            end_line,
            replacement.unwrap_or("").len()
        ))
    } else {
        Ok(format!(
            "{} {} lines ({} to {})",
            operation, lines_modified, start_line, end_line
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const TEST_FILE_CONTENTS: &str = "\
1: Hello world!
2: This is line 2
3: asdf
4: Hello world!
";

    const TEST_FILE_PATH: &str = "/test_file.txt";
    const TEST_HIDDEN_FILE_PATH: &str = "/aaaa2/.hidden";

    /// Sets up the following filesystem structure:
    /// ```text
    /// test_file.txt
    /// /home/testuser/
    /// /aaaa1/
    ///     /bbbb1/
    ///         /cccc1/
    /// /aaaa2/
    ///     .hidden
    /// ```
    async fn setup_test_directory() -> Arc<Context> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let fs = ctx.fs();
        fs.write(TEST_FILE_PATH, TEST_FILE_CONTENTS).await.unwrap();
        fs.create_dir_all("/aaaa1/bbbb1/cccc1").await.unwrap();
        fs.create_dir_all("/aaaa2").await.unwrap();
        fs.write(TEST_HIDDEN_FILE_PATH, "this is a hidden file").await.unwrap();
        ctx
    }

    #[tokio::test]
    async fn test_fs_write_batch_operations() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Create a batch operation with multiple files and edits
        let fs_write = FsWrite {
            summary: None,
            file_edits: vec![
                FileWithEdits {
                    path: TEST_FILE_PATH.to_string(),
                    edits: vec![
                        FileEdit::StrReplace {
                            old_str: "1: Hello world!".to_string(),
                            new_str: "1: Batch replaced!".to_string(),
                        },
                        FileEdit::Append {
                            new_str: "5: Appended by batch".to_string(),
                        },
                    ],
                },
                FileWithEdits {
                    path: "/batch_test.txt".to_string(),
                    edits: vec![FileEdit::Create {
                        file_text: Some("This is a new file created by batch operation".to_string()),
                        new_str: None,
                    }],
                },
            ],
        };

        // Invoke the batch operation
        let result = fs_write.invoke(&ctx, &mut stdout).await.unwrap();

        // Verify the results
        let batch_result: BatchWriteResult = match &result.output {
            OutputKind::Json(json) => serde_json::from_value(json.clone()).unwrap(),
            _ => panic!("Expected JSON output"),
        };
        assert_eq!(batch_result.file_results.len(), 2);
        assert_eq!(batch_result.total_files, 2);
        assert_eq!(batch_result.files_modified, 2);
        assert_eq!(batch_result.files_failed, 0);
        assert_eq!(batch_result.total_edits_applied, 3);
        assert_eq!(batch_result.total_edits_failed, 0);

        // Check first file results
        let file_results = &batch_result.file_results;
        assert!(file_results[0].success);
        assert_eq!(file_results[0].edits_applied, Some(2));

        // Check second file results
        assert_eq!(file_results[1].path, "/batch_test.txt");
        assert!(file_results[1].success);
        assert_eq!(file_results[1].edits_applied, Some(1));

        // Verify file contents
        let file1_content = ctx.fs().read_to_string(TEST_FILE_PATH).await.unwrap();
        assert!(file1_content.contains("1: Batch replaced!"));
        assert!(file1_content.contains("5: Appended by batch"));

        let file2_content = ctx.fs().read_to_string("/batch_test.txt").await.unwrap();
        assert!(file2_content.contains("This is a new file created by batch operation"));
    }

    #[tokio::test]
    async fn test_fs_write_batch_with_errors() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Create a batch operation with some errors
        let fs_write = FsWrite {
            summary: None,
            file_edits: vec![
                FileWithEdits {
                    path: TEST_FILE_PATH.to_string(),
                    edits: vec![
                        FileEdit::StrReplace {
                            old_str: "non-existent text".to_string(),
                            new_str: "This won't work".to_string(),
                        },
                        FileEdit::Append {
                            new_str: "This should still work".to_string(),
                        },
                    ],
                },
                FileWithEdits {
                    path: "/non-existent-file.txt".to_string(),
                    edits: vec![FileEdit::StrReplace {
                        old_str: "some text".to_string(),
                        new_str: "This won't work either".to_string(),
                    }],
                },
            ],
        };

        // Invoke the batch operation
        let result = fs_write.invoke(&ctx, &mut stdout).await.unwrap();

        // Verify the results
        let batch_result: BatchWriteResult = match &result.output {
            OutputKind::Json(json) => serde_json::from_value(json.clone()).unwrap(),
            _ => panic!("Expected JSON output"),
        };

        assert_eq!(batch_result.total_files, 2);
        assert_eq!(batch_result.files_modified, 1);
        assert_eq!(batch_result.files_failed, 1);
        assert_eq!(batch_result.total_edits_applied, 1);
        assert_eq!(batch_result.total_edits_failed, 1);

        // Check first file results - should have one success and one failure
        let file_results = &batch_result.file_results;
        assert_eq!(file_results[0].path, TEST_FILE_PATH);
        assert!(file_results[0].success); // Overall success because at least one edit succeeded
        assert_eq!(file_results[0].edits_applied, Some(1));
        assert_eq!(file_results[0].edits_failed, Some(1));
        assert!(file_results[0].failed_edits.is_some());
        assert_eq!(file_results[0].failed_edits.as_ref().unwrap().len(), 1);

        // Check second file results - should be a complete failure
        assert_eq!(file_results[1].path, "/non-existent-file.txt");
        assert!(!file_results[1].success);
        assert!(file_results[1].error.is_some());

        // Verify file contents - the append should have worked
        let file1_content = ctx.fs().read_to_string(TEST_FILE_PATH).await.unwrap();
        assert!(file1_content.contains("This should still work"));
    }
}
/// Returns a git-diff style comparison between `old_str` and `new_str`.
/// - `start_line` - 1-indexed line number that `old_str` and `new_str` start at.
fn print_diff(
    updates: &mut impl Write,
    old_str: &StylizedFile,
    new_str: &StylizedFile,
    start_line: usize,
) -> Result<()> {
    let diff = similar::TextDiff::from_lines(&old_str.content, &new_str.content);

    // First, get the gutter width required for both the old and new lines.
    let (mut max_old_i, mut max_new_i) = (1, 1);
    for change in diff.iter_all_changes() {
        if let Some(i) = change.old_index() {
            max_old_i = i + start_line;
        }
        if let Some(i) = change.new_index() {
            max_new_i = i + start_line;
        }
    }
    let old_line_num_width = terminal_width_required_for_line_count(max_old_i);
    let new_line_num_width = terminal_width_required_for_line_count(max_new_i);

    // Now, print
    fn fmt_index(i: Option<usize>, start_line: usize) -> String {
        match i {
            Some(i) => (i + start_line).to_string(),
            _ => " ".to_string(),
        }
    }
    for change in diff.iter_all_changes() {
        // Define the colors per line.
        let (text_color, gutter_bg_color, line_bg_color) = match (change.tag(), new_str.truecolor) {
            (similar::ChangeTag::Equal, true) => (style::Color::Reset, new_str.gutter_bg, new_str.line_bg),
            (similar::ChangeTag::Delete, true) => (
                style::Color::Reset,
                style::Color::Rgb { r: 79, g: 40, b: 40 },
                style::Color::Rgb { r: 36, g: 25, b: 28 },
            ),
            (similar::ChangeTag::Insert, true) => (
                style::Color::Reset,
                style::Color::Rgb { r: 40, g: 67, b: 43 },
                style::Color::Rgb { r: 24, g: 38, b: 30 },
            ),
            (similar::ChangeTag::Equal, false) => (style::Color::Reset, new_str.gutter_bg, new_str.line_bg),
            (similar::ChangeTag::Delete, false) => (style::Color::Red, new_str.gutter_bg, new_str.line_bg),
            (similar::ChangeTag::Insert, false) => (style::Color::Green, new_str.gutter_bg, new_str.line_bg),
        };
        // Define the change tag character to print, if any.
        let sign = match change.tag() {
            similar::ChangeTag::Equal => " ",
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
        };

        let old_i_str = fmt_index(change.old_index(), start_line);
        let new_i_str = fmt_index(change.new_index(), start_line);

        // Print the gutter and line numbers.
        queue!(updates, style::SetBackgroundColor(gutter_bg_color))?;
        queue!(
            updates,
            style::SetForegroundColor(text_color),
            style::Print(sign),
            style::Print(" ")
        )?;
        queue!(
            updates,
            style::Print(format!(
                "{:>old_line_num_width$}",
                old_i_str,
                old_line_num_width = old_line_num_width
            ))
        )?;
        if sign == " " {
            queue!(updates, style::Print(", "))?;
        } else {
            queue!(updates, style::Print("  "))?;
        }
        queue!(
            updates,
            style::Print(format!(
                "{:>new_line_num_width$}",
                new_i_str,
                new_line_num_width = new_line_num_width
            ))
        )?;
        // Print the line.
        queue!(
            updates,
            style::SetForegroundColor(style::Color::Reset),
            style::Print(":"),
            style::SetForegroundColor(text_color),
            style::SetBackgroundColor(line_bg_color),
            style::Print(" "),
            style::Print(change),
            style::ResetColor,
        )?;
    }
    queue!(
        updates,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine),
        style::Print("\n"),
    )?;

    Ok(())
}

/// Returns a 1-indexed line number range of the start and end of `needle` inside `file`.
fn line_number_at(file: impl AsRef<str>, needle: impl AsRef<str>) -> Option<(usize, usize)> {
    let file = file.as_ref();
    let needle = needle.as_ref();
    if let Some((i, _)) = file.match_indices(needle).next() {
        let start = file[..i].matches('\n').count();
        let end = needle.matches('\n').count();
        Some((start + 1, start + end + 1))
    } else {
        None
    }
}

/// Returns the number of terminal cells required for displaying line numbers. This is used to
/// determine how many characters the gutter should allocate when displaying line numbers for a
/// text file.
///
/// For example, `10` and `99` both take 2 cells, whereas `100` and `999` take 3.
fn terminal_width_required_for_line_count(line_count: usize) -> usize {
    line_count.to_string().chars().count()
}

/// Represents a [String] that is potentially stylized with truecolor escape codes.
#[derive(Debug)]
struct StylizedFile {
    /// Whether or not the file is stylized with 24bit color.
    truecolor: bool,
    /// File content. If [Self::truecolor] is true, then it has escape codes for styling with 24bit
    /// color.
    content: String,
    /// Background color for the gutter.
    gutter_bg: style::Color,
    /// Background color for the line content.
    line_bg: style::Color,
}

impl Default for StylizedFile {
    fn default() -> Self {
        Self {
            truecolor: false,
            content: Default::default(),
            gutter_bg: style::Color::Reset,
            line_bg: style::Color::Reset,
        }
    }
}

fn stylize_output_if_able(ctx: &Context, path: impl AsRef<Path>, file_text: &str) -> StylizedFile {
    if supports_truecolor(ctx) {
        match stylized_file(path, file_text) {
            Ok(s) => return s,
            Err(err) => {
                error!(?err, "unable to syntax highlight the output");
            },
        }
    }
    StylizedFile {
        truecolor: false,
        content: file_text.to_string(),
        gutter_bg: style::Color::Reset,
        line_bg: style::Color::Reset,
    }
}

/// Returns a 24bit terminal escaped syntax-highlighted [String] of the file pointed to by `path`,
/// if able.
fn stylized_file(path: impl AsRef<Path>, file_text: impl AsRef<str>) -> Result<StylizedFile> {
    let ps = &*SYNTAX_SET;
    let ts = &*THEME_SET;

    let extension = path
        .as_ref()
        .extension()
        .wrap_err("missing extension")?
        .to_str()
        .wrap_err("not utf8")?;

    let syntax = ps
        .find_syntax_by_extension(extension)
        .wrap_err_with(|| format!("missing extension: {}", extension))?;

    let theme = &ts.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let file_text = file_text.as_ref().lines();
    let mut file = String::new();
    for line in file_text {
        let mut ranges = Vec::new();
        ranges.append(&mut highlighter.highlight_line(line, ps)?);
        let mut escaped_line = as_24_bit_terminal_escaped(&ranges[..], false);
        escaped_line.push_str(&format!(
            "{}\n",
            crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine),
        ));
        file.push_str(&escaped_line);
    }

    let (line_bg, gutter_bg) = match (theme.settings.background, theme.settings.gutter) {
        (Some(line_bg), Some(gutter_bg)) => (line_bg, gutter_bg),
        (Some(line_bg), None) => (line_bg, line_bg),
        _ => bail!("missing theme"),
    };
    Ok(StylizedFile {
        truecolor: true,
        content: file,
        gutter_bg: syntect_to_crossterm_color(gutter_bg),
        line_bg: syntect_to_crossterm_color(line_bg),
    })
}

fn syntect_to_crossterm_color(syntect: syntect::highlighting::Color) -> style::Color {
    style::Color::Rgb {
        r: syntect.r,
        g: syntect.g,
        b: syntect.b,
    }
}

/// Helper function to get the command name from a FileEdit enum
fn get_command_name(edit: &FileEdit) -> &'static str {
    match edit {
        FileEdit::Create { .. } => "create",
        FileEdit::Rewrite { .. } => "rewrite",
        FileEdit::StrReplace { .. } => "str_replace",
        FileEdit::Insert { .. } => "insert",
        FileEdit::Append { .. } => "append",
        FileEdit::ReplaceLines { .. } => "replace_lines",
        FileEdit::DeleteLines { .. } => "delete_lines",
    }
}

/// Helper function to get the priority of an operation for sorting
/// Lower numbers = higher priority
fn get_operation_priority(edit: &FileEdit) -> u8 {
    match edit {
        // Create/Rewrite operations first (replace entire file)
        FileEdit::Create { .. } => 1,
        FileEdit::Rewrite { .. } => 2,
        // All line-number based operations have the same priority
        FileEdit::ReplaceLines { .. } => 3,
        FileEdit::DeleteLines { .. } => 3,
        FileEdit::Insert { .. } => 3,
        // String match operations
        FileEdit::StrReplace { .. } => 4,
        // Append operations last
        FileEdit::Append { .. } => 5,
    }
}

/// Helper function to build content from lines within a range
///
/// - `lines`: Vector of lines from a file
/// - `start_line`: 1-indexed starting line number
/// - `end_line`: 1-indexed ending line number
/// - Returns a string with the selected lines
fn build_content_from_lines(lines: &[&str], start_line: usize, end_line: usize) -> String {
    let mut content = String::new();
    let start_idx = start_line.saturating_sub(1);
    let end_idx = std::cmp::min(end_line, lines.len());

    for line in lines.iter().skip(start_idx).take(end_idx - start_idx) {
        content.push_str(line);
        content.push('\n');
    }

    content
}

/// Helper function to queue formatted output to the terminal
///
/// This function queues a message with optional colored text to the terminal.
/// Uses queue_function_result for consistent formatting.
fn queue_formatted_message(
    updates: &mut impl Write,
    message: &str,
    highlighted_text: &str,
    color: Color,
) -> Result<()> {
    // Determine if this is an error message based on the color
    let is_error = color == Color::Red;

    // Format the message and use queue_function_result for consistent styling
    let formatted_message = format!("{}{}", message, highlighted_text);
    super::queue_function_result(&formatted_message, updates, is_error, false)?;

    Ok(())
}

/// Returns a prefix/suffix pair before and after the content dictated by `[start_line, end_line]`
/// within `content`. The updated start and end lines containing the original context along with
/// the suffix and prefix are returned.
///
/// Params:
/// - `start_line` - 1-indexed starting line of the content.
/// - `end_line` - 1-indexed ending line of the content.
/// - `context_lines` - number of lines to include before the start and end.
///
/// Returns `(prefix, new_start_line, suffix, new_end_line)`
fn get_lines_with_context(
    content: &str,
    start_line: usize,
    end_line: usize,
    context_lines: usize,
) -> (&str, usize, &str, usize) {
    let line_count = content.lines().count();
    // We want to support end_line being 0, in which case we should be able to set the first line
    // as the suffix.
    let zero_check_inc = if end_line == 0 { 0 } else { 1 };

    // Convert to 0-indexing.
    let (start_line, end_line) = (
        start_line.saturating_sub(1).clamp(0, line_count - 1),
        end_line.saturating_sub(1).clamp(0, line_count - 1),
    );
    let new_start_line = 0.max(start_line.saturating_sub(context_lines));
    let new_end_line = (line_count - 1).min(end_line + context_lines);

    // Build prefix
    let mut prefix_start = 0;
    for line in LinesWithEndings::from(content).take(new_start_line) {
        prefix_start += line.len();
    }
    let mut prefix_end = prefix_start;
    for line in LinesWithEndings::from(&content[prefix_start..]).take(start_line - new_start_line) {
        prefix_end += line.len();
    }

    // Build suffix
    let mut suffix_start = 0;
    for line in LinesWithEndings::from(content).take(end_line + zero_check_inc) {
        suffix_start += line.len();
    }
    let mut suffix_end = suffix_start;
    for line in LinesWithEndings::from(&content[suffix_start..]).take(new_end_line - end_line) {
        suffix_end += line.len();
    }

    (
        &content[prefix_start..prefix_end],
        new_start_line + 1,
        &content[suffix_start..suffix_end],
        new_end_line + zero_check_inc,
    )
}
/// Apply a single edit operation and display the diff
///
/// This function applies a single edit operation to a file and displays the diff
/// in the terminal. It returns a message describing the operation result.
async fn apply_edit_with_diff(
    ctx: &Context,
    path: &Path,
    edit: &FileEdit,
    relative_path: &str,
    updates: &mut impl Write,
) -> Result<String> {
    match edit {
        FileEdit::Create { file_text, new_str } => {
            // Use the helper function with check_exists=true
            let result = handle_file_creation(ctx, path, file_text, new_str, true).await?;

            super::queue_function_result(&format!("Created: {}", relative_path), updates, false, false)?;

            Ok(result)
        },
        FileEdit::Rewrite { file_text, new_str } => {
            // Use the helper function with check_exists=false
            let result = handle_file_creation(ctx, path, file_text, new_str, false).await?;

            super::queue_function_result(&format!("Rewritten: {}", relative_path), updates, false, false)?;

            Ok(result)
        },
        FileEdit::StrReplace { old_str, new_str } => {
            // Use the helper function for string replacement
            let result = handle_string_operation(ctx, path, "replace", Some(old_str), new_str, None).await?;

            super::queue_function_result(&format!("Updated: {}", relative_path), updates, false, false)?;

            Ok(result)
        },
        FileEdit::Insert { insert_line, new_str } => {
            // Use the helper function for insertion
            let result = handle_string_operation(ctx, path, "insert", None, new_str, Some(*insert_line)).await?;

            super::queue_function_result(
                &format!("Inserted at line: {} in {}", insert_line, relative_path),
                updates,
                false,
                false,
            )?;

            Ok(result)
        },
        FileEdit::Append { new_str } => {
            // Use the helper function for append
            let result = handle_string_operation(ctx, path, "append", None, new_str, None).await?;

            super::queue_function_result(&format!("Appended to: {}", relative_path), updates, false, false)?;

            Ok(result)
        },
        FileEdit::ReplaceLines {
            start_line,
            end_line,
            new_str,
        } => {
            // Use the helper function with the replacement content
            let result = handle_line_modification(ctx, path, *start_line, *end_line, Some(new_str)).await?;

            super::queue_function_result(
                &format!("Replaced lines: {} to {} in {}", start_line, end_line, relative_path),
                updates,
                false,
                false,
            )?;

            Ok(result)
        },
        FileEdit::DeleteLines { start_line, end_line } => {
            // Use the helper function with no replacement content
            let result = handle_line_modification(ctx, path, *start_line, *end_line, None).await?;

            super::queue_function_result(
                &format!("Deleted lines: {} to {} in {}", start_line, end_line, relative_path),
                updates,
                false,
                false,
            )?;

            Ok(result)
        },
    }
}
