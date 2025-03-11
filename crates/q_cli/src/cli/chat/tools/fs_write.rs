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
use similar::DiffableStr;
use syntect::util::LinesWithEndings;
use tracing::warn;

use super::{
    InvokeOutput,
    StylizedFile,
    format_path,
    sanitize_path_tool_arg,
    stylize_output_if_able,
    terminal_width_required_for_line_count,
};
use crate::cli::chat::tools::supports_truecolor;

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
    /// Helper function to clean file content:
    /// 1. Ensures the file ends with a newline
    /// 2. Removes trailing whitespace from each line
    fn clean_file_content(content: String) -> String {
        let mut cleaned = content
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        cleaned.push('\n');
        cleaned
    }

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

                let cleaned_text = Self::clean_file_content(file_text);
                fs.write(&path, cleaned_text.as_bytes()).await?;
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
                        let cleaned_file = Self::clean_file_content(file);
                        fs.write(path, cleaned_file).await?;
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
                let cleaned_file = Self::clean_file_content(file);
                fs.write(&path, &cleaned_file).await?;
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

                let mut file_content = fs.read_to_string(&path).await.unwrap_or_default();
                if !file_content.ends_with_newline() {
                    file_content.push('\n');
                }
                file_content.push_str(new_str);
                if !file_content.ends_with_newline() {
                    file_content.push('\n');
                }
                fs.write(&path, file_content).await?;
                Ok(Default::default())
            },
        }
    }

    pub fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let cwd = ctx.env().current_dir()?;
        self.print_relative_path(ctx, updates)?;
        match self {
            FsWrite::Create { path, .. } => {
                let file_text = self.canonical_create_command_text();
                let relative_path = format_path(cwd, path);
                let prev = if ctx.fs().exists(path) {
                    let file = ctx.fs().read_to_string_sync(path)?;
                    stylize_output_if_able(ctx, path, &file)
                } else {
                    Default::default()
                };
                let new = stylize_output_if_able(ctx, &relative_path, &file_text);
                print_diff(ctx, updates, &prev, &new, 1)?;
                Ok(())
            },
            FsWrite::Insert {
                path,
                insert_line,
                new_str,
            } => {
                let relative_path = format_path(cwd, path);
                let file = ctx.fs().read_to_string_sync(&relative_path)?;

                // Diff the old with the new by adding extra context around the line being inserted
                // at.
                let (prefix, start_line, suffix, _) = get_lines_with_context(&file, *insert_line, *insert_line, 3);
                let insert_line_content = LinesWithEndings::from(&file)
                    // don't include any content if insert_line is 0
                    .nth(insert_line.checked_sub(1).unwrap_or(usize::MAX))
                    .unwrap_or_default();
                let old = [prefix, insert_line_content, suffix].join("");
                let new = [prefix, insert_line_content, new_str, suffix].join("");

                let old = stylize_output_if_able(ctx, &relative_path, &old);
                let new = stylize_output_if_able(ctx, &relative_path, &new);
                print_diff(ctx, updates, &old, &new, start_line)?;
                Ok(())
            },
            FsWrite::StrReplace { path, old_str, new_str } => {
                let relative_path = format_path(cwd, path);
                let file = ctx.fs().read_to_string_sync(&relative_path)?;
                let (start_line, _) = match line_number_at(&file, old_str) {
                    Some((start_line, end_line)) => (start_line, end_line),
                    _ => (0, 0),
                };
                let old_str = stylize_output_if_able(ctx, &relative_path, old_str);
                let new_str = stylize_output_if_able(ctx, &relative_path, new_str);
                print_diff(ctx, updates, &old_str, &new_str, start_line)?;

                Ok(())
            },
            FsWrite::Append { path, new_str } => {
                let relative_path = format_path(cwd, path);
                let start_line = ctx.fs().read_to_string_sync(&relative_path)?.lines().count() + 1;
                let file = stylize_output_if_able(ctx, &relative_path, new_str);
                print_diff(ctx, updates, &Default::default(), &file, start_line)?;
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

    fn print_relative_path(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let cwd = ctx.env().current_dir()?;
        let path = match self {
            FsWrite::Create { path, .. } => path,
            FsWrite::StrReplace { path, .. } => path,
            FsWrite::Insert { path, .. } => path,
            FsWrite::Append { path, .. } => path,
        };
        let relative_path = format_path(cwd, path);
        queue!(
            updates,
            style::Print("Path: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&relative_path),
            style::ResetColor,
            style::Print("\n\n"),
        )?;
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

/// Prints a git-diff style comparison between `old_str` and `new_str`.
/// - `start_line` - 1-indexed line number that `old_str` and `new_str` start at.
fn print_diff(
    ctx: &Context,
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
    fn fmt_i(i: Option<usize>, start_line: usize) -> String {
        match i {
            Some(i) => (i + start_line).to_string(),
            _ => " ".to_string(),
        }
    }
    for change in diff.iter_all_changes() {
        // Colors
        let (text_color, gutter_bg_color, line_bg_color) = match (change.tag(), supports_truecolor(ctx)) {
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
            (similar::ChangeTag::Equal, false) => (style::Color::Reset, style::Color::Reset, style::Color::Reset),
            (similar::ChangeTag::Delete, false) => (style::Color::Red, style::Color::Reset, style::Color::Reset),
            (similar::ChangeTag::Insert, false) => (style::Color::Green, style::Color::Reset, style::Color::Reset),
        };
        // Change tag character
        let sign = match change.tag() {
            similar::ChangeTag::Equal => " ",
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
        };

        let old_i_str = fmt_i(change.old_index(), start_line);
        let new_i_str = fmt_i(change.new_index(), start_line);
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

    use similar::DiffableStr;

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

        assert_eq!(
            ctx.fs().read_to_string("/my-file").await.unwrap(),
            format!("{}\n", file_text)
        );

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

        // File should end with a newline
        assert_eq!(
            ctx.fs().read_to_string("/my-file").await.unwrap(),
            format!("{}\n", file_text)
        );

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

        assert_eq!(
            ctx.fs().read_to_string("/my-file").await.unwrap(),
            format!("{}\n", file_text)
        );
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
        assert_eq!(actual, format!("{}{}\n", test_file_contents, new_str));

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
        assert_eq!(actual, format!("{}{}{}\n", new_str, test_file_contents, new_str));
    }

    #[tokio::test]
    async fn test_fs_write_tool_append() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test appending to existing file
        let content_to_append = "5: Appended line";
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
            format!("{}{}\n", TEST_FILE_CONTENTS, content_to_append),
            "Content should be appended to the end of the file with a newline added"
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

    #[test]
    fn test_clean_file_content() {
        // Test removing trailing whitespace
        let content = "Hello world!  \nThis is a test   \nWith trailing spaces    ";
        let expected = "Hello world!\nThis is a test\nWith trailing spaces\n";
        assert_eq!(FsWrite::clean_file_content(content.to_string()), expected);

        // Test ensuring ending newline
        let content = "Hello world!\nNo ending newline";
        let expected = "Hello world!\nNo ending newline\n";
        assert_eq!(FsWrite::clean_file_content(content.to_string()), expected);

        // Test with content already having ending newline
        let content = "Hello world!\nWith ending newline\n";
        let expected = "Hello world!\nWith ending newline\n";
        assert_eq!(FsWrite::clean_file_content(content.to_string()), expected);

        // Test with empty string
        let content = "";
        let expected = "\n";
        assert_eq!(FsWrite::clean_file_content(content.to_string()), expected);
    }

    #[test]
    fn test_lines_with_context() {
        let content = "Hello\nWorld!\nhow\nare\nyou\ntoday?";
        assert_eq!(get_lines_with_context(content, 1, 1, 1), ("", 1, "World!\n", 2));
        assert_eq!(get_lines_with_context(content, 0, 0, 2), ("", 1, "Hello\nWorld!\n", 2));
        assert_eq!(
            get_lines_with_context(content, 2, 4, 50),
            ("Hello\n", 1, "you\ntoday?", 6)
        );
        assert_eq!(get_lines_with_context(content, 4, 100, 2), ("World!\nhow\n", 2, "", 6));
    }
}
