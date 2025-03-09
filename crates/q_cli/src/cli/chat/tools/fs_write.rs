use std::borrow::Cow;
use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    bail,
    eyre,
};
use fig_os_shim::Context;
use serde::Deserialize;
use tracing::warn;

use super::{
    InvokeOutput,
    format_path,
    sanitize_path_tool_arg,
    stylize_output_if_able,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum FsWrite {
    /// The tool spec should only require `file_text`, but the model sometimes doesn't want to
    /// provide it. Thus, including `new_str` as a fallback check, if it's available.
    #[serde(rename = "create")]
    Create {
        path: String,
        file_text: Option<String>,
        new_str: Option<String>,
    },
    #[serde(rename = "str_replace")]
    StrReplace {
        path: String,
        old_str: String,
        new_str: String,
    },
    #[serde(rename = "insert")]
    Insert {
        path: String,
        insert_line: usize,
        new_str: String,
    },
    #[serde(rename = "append")]
    Append { path: String, new_str: String },
}

impl FsWrite {
    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let fs = ctx.fs();
        let cwd = ctx.env().current_dir()?;
        match self {
            FsWrite::Create { path, .. } => {
                let file_text = self.canonical_create_command_text();
                let path = sanitize_path_tool_arg(ctx, path);
                if let Some(parent) = path.parent() {
                    fs.create_dir_all(parent).await?;
                }

                let invoke_description = if fs.exists(&path) { "Replacing: " } else { "Creating: " };
                queue!(
                    updates,
                    style::Print(invoke_description),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format_path(cwd, &path)),
                    style::ResetColor,
                    style::Print("\n"),
                )?;
                fs.write(&path, file_text.as_bytes()).await?;
                Ok(Default::default())
            },
            FsWrite::StrReplace { path, old_str, new_str } => {
                let path = sanitize_path_tool_arg(ctx, path);
                let file = fs.read_to_string(&path).await?;
                let matches = file.match_indices(old_str).collect::<Vec<_>>();
                queue!(
                    updates,
                    style::Print("Updating: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format_path(cwd, &path)),
                    style::ResetColor,
                    style::Print("\n"),
                )?;
                match matches.len() {
                    0 => Err(eyre!("no occurrences of \"{old_str}\" were found")),
                    1 => {
                        let file = file.replacen(old_str, new_str, 1);
                        fs.write(path, file).await?;
                        Ok(Default::default())
                    },
                    x => Err(eyre!("{x} occurrences of old_str were found when only 1 is expected")),
                }
            },
            FsWrite::Insert {
                path,
                insert_line,
                new_str,
            } => {
                let path = sanitize_path_tool_arg(ctx, path);
                let mut file = fs.read_to_string(&path).await?;
                queue!(
                    updates,
                    style::Print("Updating: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format_path(cwd, &path)),
                    style::ResetColor,
                    style::Print("\n"),
                )?;

                // Get the index of the start of the line to insert at.
                let num_lines = file.lines().enumerate().map(|(i, _)| i + 1).last().unwrap_or(1);
                let insert_line = insert_line.clamp(&0, &num_lines);
                let mut i = 0;
                for _ in 0..*insert_line {
                    let line_len = &file[i..].find("\n").map_or(file[i..].len(), |i| i + 1);
                    i += line_len;
                }
                file.insert_str(i, new_str);
                fs.write(&path, &file).await?;
                Ok(Default::default())
            },
            FsWrite::Append { path, new_str } => {
                let path = sanitize_path_tool_arg(ctx, path);

                // Return an error if the file doesn't exist
                if !fs.exists(&path) {
                    bail!("The file does not exist: {}", path.display());
                }

                queue!(
                    updates,
                    style::Print("Appending to: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(format_path(cwd, &path)),
                    style::ResetColor,
                    style::Print("\n"),
                )?;

                // Read existing content
                let mut file_content = fs.read_to_string(&path).await.unwrap_or_default();

                // Check if we need to add a newline before appending
                // Only add a newline if the file is not empty and doesn't already end with one
                // Also don't add a newline if the new content starts with one
                if !file_content.is_empty() && !file_content.ends_with('\n') && !new_str.starts_with('\n') {
                    file_content.push('\n');
                }

                // Append the new content
                file_content.push_str(new_str);
                fs.write(&path, file_content).await?;
                Ok(Default::default())
            },
        }
    }

    pub fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let cwd = ctx.env().current_dir()?;
        match self {
            FsWrite::Create { path, .. } => {
                let file_text = self.canonical_create_command_text();
                let relative_path = format_path(cwd, path);
                queue!(
                    updates,
                    style::Print("Path: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&relative_path),
                    style::ResetColor,
                    style::Print("\n\n"),
                )?;
                if ctx.fs().exists(path) {
                    let prev = ctx.fs().read_to_string_sync(path)?;
                    let prev = stylize_output_if_able(ctx, &relative_path, prev.as_str(), None, Some("-"));
                    let new = stylize_output_if_able(ctx, &relative_path, &file_text, None, Some("+"));
                    queue!(
                        updates,
                        style::Print("Replacing:\n"),
                        style::Print(prev),
                        style::ResetColor,
                        style::Print("\n\n"),
                        style::Print("With:\n"),
                        style::Print(new),
                        style::ResetColor,
                        style::Print("\n\n")
                    )?;
                } else {
                    let file = stylize_output_if_able(ctx, &relative_path, &file_text, None, None);
                    queue!(
                        updates,
                        style::Print("Contents:\n"),
                        style::Print(file),
                        style::ResetColor,
                    )?;
                }
                Ok(())
            },
            FsWrite::Insert {
                path,
                insert_line,
                new_str,
            } => {
                let relative_path = format_path(cwd, path);
                let file = stylize_output_if_able(ctx, &relative_path, new_str, Some(*insert_line), Some("+"));
                queue!(
                    updates,
                    style::Print("Path: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(relative_path),
                    style::ResetColor,
                    style::Print("\n\nContents:\n"),
                    style::Print(file),
                    style::ResetColor,
                )?;
                Ok(())
            },
            FsWrite::StrReplace { path, old_str, new_str } => {
                let relative_path = format_path(cwd, path);
                let file = ctx.fs().read_to_string_sync(&relative_path)?;
                // TODO: we should pass some additional lines as context before and after the file.
                let (start_line, _) = match line_number_at(&file, old_str) {
                    Some((start_line, end_line)) => (Some(start_line), Some(end_line)),
                    _ => (None, None),
                };
                let old_str = stylize_output_if_able(ctx, &relative_path, old_str, start_line, Some("-"));
                let new_str = stylize_output_if_able(ctx, &relative_path, new_str, start_line, Some("+"));
                queue!(
                    updates,
                    style::Print("Path: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(relative_path),
                    style::ResetColor,
                    style::Print("\n\n"),
                    style::Print("Replacing:\n"),
                    style::Print(old_str),
                    style::ResetColor,
                    style::Print("\n\n"),
                    style::Print("With:\n"),
                    style::Print(new_str),
                    style::ResetColor
                )?;
                Ok(())
            },
            FsWrite::Append { path, new_str } => {
                let relative_path = format_path(cwd, path);
                let file = stylize_output_if_able(ctx, &relative_path, new_str, None, Some("+"));
                queue!(
                    updates,
                    style::Print("Path: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(relative_path),
                    style::ResetColor,
                    style::Print("\n\nAppending content:\n"),
                    style::Print(file),
                    style::ResetColor,
                )?;
                Ok(())
            },
        }
    }

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        match self {
            FsWrite::Create { path, .. } => {
                if path.is_empty() {
                    bail!("Path must not be empty")
                };
            },
            FsWrite::StrReplace { path, .. } | FsWrite::Insert { path, .. } => {
                let path = sanitize_path_tool_arg(ctx, path);
                if !path.exists() {
                    bail!("The provided path must exist in order to replace or insert contents into it")
                }
            },
            FsWrite::Append { path, new_str } => {
                if path.is_empty() {
                    bail!("Path must not be empty")
                };
                if new_str.is_empty() {
                    bail!("Content to append must not be empty")
                };
            },
        }

        Ok(())
    }

    /// Returns the text to use for the [FsWrite::Create] command. This is required since we can't
    /// rely on the model always providing `file_text`.
    fn canonical_create_command_text(&self) -> String {
        match self {
            FsWrite::Create { file_text, new_str, .. } => match (file_text, new_str) {
                (Some(file_text), _) => file_text.clone(),
                (None, Some(new_str)) => {
                    warn!("required field `file_text` is missing, using the provided `new_str` instead");
                    new_str.clone()
                },
                _ => {
                    warn!("no content provided for the create command");
                    String::new()
                },
            },
            _ => String::new(),
        }
    }
}

/// Limits the passed str to `max_len`.
///
/// If the str exceeds `max_len`, then the first `max_len` characters are returned with a suffix of
/// `"<...Truncated>`. Otherwise, the str is returned as is.
#[allow(dead_code)]
fn truncate_str(text: &str, max_len: usize) -> Cow<'_, str> {
    if text.len() > max_len {
        let mut out = String::new();
        let t = "<...Truncated>";
        out.push_str(&text[..max_len]);
        out.push_str(t);
        out.into()
    } else {
        text.into()
    }
}

/// Returns a 1-indexed line number range of the start and end of `needle` inside `file`.
fn line_number_at(file: impl AsRef<str>, needle: impl AsRef<str>) -> Option<(usize, usize)> {
    let file = file.as_ref();
    let needle = needle.as_ref();
    if let Some((i, _)) = file.match_indices(needle).next() {
        let start = file[..i].matches("\n").count();
        let end = needle.matches("\n").count();
        Some((start + 1, start + end + 1))
    } else {
        None
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

    #[test]
    fn test_fs_write_deserialize() {
        let path = "/my-file";
        let file_text = "hello world";

        // create
        let v = serde_json::json!({
            "path": path,
            "command": "create",
            "file_text": file_text
        });
        let fw = serde_json::from_value::<FsWrite>(v).unwrap();
        assert!(matches!(fw, FsWrite::Create { .. }));

        // str_replace
        let v = serde_json::json!({
            "path": path,
            "command": "str_replace",
            "old_str": "prev string",
            "new_str": "new string",
        });
        let fw = serde_json::from_value::<FsWrite>(v).unwrap();
        assert!(matches!(fw, FsWrite::StrReplace { .. }));

        // insert
        let v = serde_json::json!({
            "path": path,
            "command": "insert",
            "insert_line": 3,
            "new_str": "new string",
        });
        let fw = serde_json::from_value::<FsWrite>(v).unwrap();
        assert!(matches!(fw, FsWrite::Insert { .. }));

        // append
        let v = serde_json::json!({
            "path": path,
            "command": "append",
            "new_str": "appended content",
        });
        let fw = serde_json::from_value::<FsWrite>(v).unwrap();
        assert!(matches!(fw, FsWrite::Append { .. }));
    }

    #[tokio::test]
    async fn test_fs_write_tool_create() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        let file_text = "Hello, world!";
        let v = serde_json::json!({
            "path": "/my-file",
            "command": "create",
            "file_text": file_text
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        assert_eq!(ctx.fs().read_to_string("/my-file").await.unwrap(), file_text);

        let file_text = "Goodbye, world!\nSee you later";
        let v = serde_json::json!({
            "path": "/my-file",
            "command": "create",
            "file_text": file_text
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        assert_eq!(ctx.fs().read_to_string("/my-file").await.unwrap(), file_text);

        let file_text = "This is a new string";
        let v = serde_json::json!({
            "path": "/my-file",
            "command": "create",
            "new_str": file_text
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        assert_eq!(ctx.fs().read_to_string("/my-file").await.unwrap(), file_text);
    }

    #[tokio::test]
    async fn test_fs_write_tool_str_replace() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // No instances found
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "str_replace",
            "old_str": "asjidfopjaieopr",
            "new_str": "1623749",
        });
        assert!(
            serde_json::from_value::<FsWrite>(v)
                .unwrap()
                .invoke(&ctx, &mut stdout)
                .await
                .is_err()
        );

        // Multiple instances found
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "str_replace",
            "old_str": "Hello world!",
            "new_str": "Goodbye world!",
        });
        assert!(
            serde_json::from_value::<FsWrite>(v)
                .unwrap()
                .invoke(&ctx, &mut stdout)
                .await
                .is_err()
        );

        // Single instance found and replaced
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "str_replace",
            "old_str": "1: Hello world!",
            "new_str": "1: Goodbye world!",
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        assert_eq!(
            ctx.fs()
                .read_to_string(TEST_FILE_PATH)
                .await
                .unwrap()
                .lines()
                .next()
                .unwrap(),
            "1: Goodbye world!",
            "expected the only occurrence to be replaced"
        );
    }

    #[tokio::test]
    async fn test_fs_write_tool_insert_at_beginning() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        let new_str = "1: New first line!\n";
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "insert",
            "insert_line": 0,
            "new_str": new_str,
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        let actual = ctx.fs().read_to_string(TEST_FILE_PATH).await.unwrap();
        assert_eq!(
            format!("{}\n", actual.lines().next().unwrap()),
            new_str,
            "expected the first line to be updated to '{}'",
            new_str
        );
        assert_eq!(
            actual.lines().skip(1).collect::<Vec<_>>(),
            TEST_FILE_CONTENTS.lines().collect::<Vec<_>>(),
            "the rest of the file should not have been updated"
        );
    }

    #[tokio::test]
    async fn test_fs_write_tool_insert_after_first_line() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        let new_str = "2: New second line!\n";
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "insert",
            "insert_line": 1,
            "new_str": new_str,
        });

        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        let actual = ctx.fs().read_to_string(TEST_FILE_PATH).await.unwrap();
        assert_eq!(
            format!("{}\n", actual.lines().nth(1).unwrap()),
            new_str,
            "expected the second line to be updated to '{}'",
            new_str
        );
        assert_eq!(
            actual.lines().skip(2).collect::<Vec<_>>(),
            TEST_FILE_CONTENTS.lines().skip(1).collect::<Vec<_>>(),
            "the rest of the file should not have been updated"
        );
    }

    #[tokio::test]
    async fn test_fs_write_tool_insert_when_no_newlines_in_file() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let mut stdout = std::io::stdout();

        let test_file_path = "/file.txt";
        let test_file_contents = "hello there";
        ctx.fs().write(test_file_path, test_file_contents).await.unwrap();

        let new_str = "test";

        // First, test appending
        let v = serde_json::json!({
            "path": test_file_path,
            "command": "insert",
            "insert_line": 1,
            "new_str": new_str,
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        let actual = ctx.fs().read_to_string(test_file_path).await.unwrap();
        assert_eq!(actual, format!("{}{}", test_file_contents, new_str),);

        // Then, test prepending
        let v = serde_json::json!({
            "path": test_file_path,
            "command": "insert",
            "insert_line": 0,
            "new_str": new_str,
        });
        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        let actual = ctx.fs().read_to_string(test_file_path).await.unwrap();
        assert_eq!(actual, format!("{}{}{}", new_str, test_file_contents, new_str),);
    }

    #[tokio::test]
    async fn test_fs_write_tool_append() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test appending to existing file
        let content_to_append = "\n5: Appended line";
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "command": "append",
            "new_str": content_to_append,
        });

        serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        let actual = ctx.fs().read_to_string(TEST_FILE_PATH).await.unwrap();
        assert_eq!(
            actual,
            format!("{}{}", TEST_FILE_CONTENTS, content_to_append),
            "Content should be appended to the end of the file"
        );

        // Test appending to non-existent file (should fail)
        let new_file_path = "/new_append_file.txt";
        let content = "This is a new file created by append";
        let v = serde_json::json!({
            "path": new_file_path,
            "command": "append",
            "new_str": content,
        });

        let result = serde_json::from_value::<FsWrite>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await;

        assert!(result.is_err(), "Appending to non-existent file should fail");
    }

    #[test]
    fn test_truncate_str() {
        let s = "Hello, world!";
        assert_eq!(truncate_str(s, 6), "Hello,<...Truncated>");
        let s = "Hello, world!";
        assert_eq!(truncate_str(s, 13), s);
        let s = "Hello, world!";
        assert_eq!(truncate_str(s, 0), "<...Truncated>");
    }
}
