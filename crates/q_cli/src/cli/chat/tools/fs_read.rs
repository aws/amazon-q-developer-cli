use std::collections::VecDeque;
use std::fs::Metadata;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    bail,
};
use fig_os_shim::Context;
use serde::{
    Deserialize,
    Serialize,
};
use syntect::util::LinesWithEndings;
use tracing::{
    debug,
    warn,
};

use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
    format_path,
    sanitize_path_tool_arg,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode")]
pub enum FsRead {
    Line(FsLine),
    Directory(FsDirectory),
    Search(FsSearch),
}

impl FsRead {
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        match self {
            FsRead::Line(fs_line) => fs_line.validate(ctx).await,
            FsRead::Directory(fs_directory) => fs_directory.validate(ctx).await,
            FsRead::Search(fs_search) => fs_search.validate(ctx).await,
        }
    }

    pub async fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        match self {
            FsRead::Line(fs_line) => fs_line.queue_description(ctx, updates).await,
            FsRead::Directory(fs_directory) => fs_directory.queue_description(updates),
            FsRead::Search(fs_search) => fs_search.queue_description(updates),
        }
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        match self {
            FsRead::Line(fs_line) => fs_line.invoke(ctx, updates).await,
            FsRead::Directory(fs_directory) => fs_directory.invoke(ctx, updates).await,
            FsRead::Search(fs_search) => fs_search.invoke(ctx, updates).await,
        }
    }
}

/// Read lines from a file.
#[derive(Debug, Clone, Deserialize)]
pub struct FsLine {
    pub path: String,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
}

impl FsLine {
    const DEFAULT_END_LINE: i32 = -1;
    const DEFAULT_START_LINE: i32 = 1;

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        if !path.exists() {
            bail!("'{}' does not exist", self.path);
        }
        let is_file = ctx.fs().symlink_metadata(&path).await?.is_file();
        if !is_file {
            bail!("'{}' is not a file", self.path);
        }
        Ok(())
    }

    pub async fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let line_count = ctx.fs().read_to_string(&path).await?.lines().count();
        queue!(
            updates,
            style::Print("Reading file: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(", "),
        )?;

        let start = convert_negative_index(line_count, self.start_line()) + 1;
        let end = convert_negative_index(line_count, self.end_line()) + 1;
        match (start, end) {
            _ if start == 1 && end == line_count => Ok(queue!(updates, style::Print("all lines".to_string()))?),
            _ if end == line_count => Ok(queue!(
                updates,
                style::Print("from line "),
                style::SetForegroundColor(Color::Green),
                style::Print(start),
                style::ResetColor,
                style::Print(" to end of file"),
            )?),
            _ => Ok(queue!(
                updates,
                style::Print("from line "),
                style::SetForegroundColor(Color::Green),
                style::Print(start),
                style::ResetColor,
                style::Print(" to "),
                style::SetForegroundColor(Color::Green),
                style::Print(end),
                style::ResetColor,
            )?),
        }
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let relative_path = format_path(ctx.env().current_dir()?, &path);
        debug!(?path, "Reading");
        let file = ctx.fs().read_to_string(&path).await?;
        let line_count = file.lines().count();
        let (start, end) = (
            convert_negative_index(line_count, self.start_line()),
            convert_negative_index(line_count, self.end_line()),
        );

        // safety check to ensure end is always greater than start
        let end = end.max(start);

        if start >= line_count {
            bail!(
                "starting index: {} is outside of the allowed range: ({}, {})",
                self.start_line(),
                -(line_count as i64),
                line_count
            );
        }

        // The range should be inclusive on both ends.
        let file_contents = file
            .lines()
            .skip(start)
            .take(end - start + 1)
            .collect::<Vec<_>>()
            .join("\n");

        queue!(
            updates,
            style::Print("Reading: "),
            style::SetForegroundColor(Color::Green),
            style::Print(relative_path),
            style::ResetColor,
            style::Print("\n"),
        )?;

        let byte_count = file_contents.len();
        if byte_count > MAX_TOOL_RESPONSE_SIZE {
            bail!(
                "This tool only supports reading {MAX_TOOL_RESPONSE_SIZE} bytes at a
time. You tried to read {byte_count} bytes. Try executing with fewer lines specified."
            );
        }

        Ok(InvokeOutput {
            output: OutputKind::Text(file_contents),
        })
    }

    fn start_line(&self) -> i32 {
        self.start_line.unwrap_or(Self::DEFAULT_START_LINE)
    }

    fn end_line(&self) -> i32 {
        self.end_line.unwrap_or(Self::DEFAULT_END_LINE)
    }
}

/// Search in a file.
#[derive(Debug, Clone, Deserialize)]
pub struct FsSearch {
    pub path: String,
    pub pattern: String,
    pub context_lines: Option<usize>,
}

impl FsSearch {
    const CONTEXT_LINE_PREFIX: &str = "  ";
    const DEFAULT_CONTEXT_LINES: usize = 2;
    const MATCHING_LINE_PREFIX: &str = "→ ";

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let relative_path = format_path(ctx.env().current_dir()?, &path);
        if !path.exists() {
            bail!("File not found: {}", relative_path);
        }
        if !ctx.fs().symlink_metadata(path).await?.is_file() {
            bail!("Path is not a file: {}", relative_path);
        }
        if self.pattern.is_empty() {
            bail!("Search pattern cannot be empty");
        }
        Ok(())
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Searching: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(" for pattern: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.pattern.to_lowercase()),
            style::ResetColor,
        )?;
        Ok(())
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let file_path = sanitize_path_tool_arg(ctx, &self.path);
        let pattern = &self.pattern;
        let relative_path = format_path(ctx.env().current_dir()?, &file_path);

        let file_content = ctx.fs().read_to_string(&file_path).await?;
        let lines: Vec<&str> = LinesWithEndings::from(&file_content).collect();

        let mut results = Vec::new();
        let mut total_matches = 0;

        // Case insensitive search
        let pattern_lower = pattern.to_lowercase();
        for (line_num, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains(&pattern_lower) {
                total_matches += 1;
                let start = line_num.saturating_sub(self.context_lines());
                let end = lines.len().min(line_num + self.context_lines() + 1);
                let mut context_text = Vec::new();
                (start..end).for_each(|i| {
                    let prefix = if i == line_num {
                        Self::MATCHING_LINE_PREFIX
                    } else {
                        Self::CONTEXT_LINE_PREFIX
                    };
                    let line_text = lines[i].to_string();
                    context_text.push(format!("{}{}: {}", prefix, i + 1, line_text));
                });
                let match_text = context_text.join("");
                results.push(SearchMatch {
                    line_number: line_num + 1,
                    context: match_text,
                });
            }
        }

        queue!(
            updates,
            style::SetForegroundColor(Color::Yellow),
            style::ResetColor,
            style::Print(format!(
                "Found {} matches for pattern '{}' in {}\n",
                total_matches, pattern, relative_path
            )),
            style::Print("\n"),
            style::ResetColor,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Text(serde_json::to_string(&results)?),
        })
    }

    fn context_lines(&self) -> usize {
        self.context_lines.unwrap_or(Self::DEFAULT_CONTEXT_LINES)
    }
}

/// List directory contents.
#[derive(Debug, Clone, Deserialize)]
pub struct FsDirectory {
    pub path: String,
    pub depth: Option<usize>,
}

impl FsDirectory {
    const DEFAULT_DEPTH: usize = 0;

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let relative_path = format_path(ctx.env().current_dir()?, &path);
        if !path.exists() {
            bail!("Directory not found: {}", relative_path);
        }
        if !ctx.fs().symlink_metadata(path).await?.is_dir() {
            bail!("Path is not a directory: {}", relative_path);
        }
        Ok(())
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Reading directory: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(" "),
        )?;
        let depth = self.depth.unwrap_or_default();
        Ok(queue!(
            updates,
            style::Print(format!("with maximum depth of {}", depth))
        )?)
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let cwd = ctx.env().current_dir()?;
        let max_depth = self.depth();
        debug!(?path, max_depth, "Reading directory at path with depth");
        let mut result = Vec::new();
        let mut dir_queue = VecDeque::new();
        dir_queue.push_back((path, 0));
        while let Some((path, depth)) = dir_queue.pop_front() {
            if depth > max_depth {
                break;
            }
            let relative_path = format_path(&cwd, &path);
            queue!(
                updates,
                style::Print("Reading: "),
                style::SetForegroundColor(Color::Green),
                style::Print(&relative_path),
                style::ResetColor,
                style::Print("\n"),
            )?;
            let mut read_dir = ctx.fs().read_dir(path).await?;
            while let Some(ent) = read_dir.next_entry().await? {
                use std::os::unix::fs::MetadataExt;
                let md = ent.metadata().await?;
                let formatted_mode = format_mode(md.permissions().mode()).into_iter().collect::<String>();

                let modified_timestamp = md.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let datetime = time::OffsetDateTime::from_unix_timestamp(modified_timestamp as i64).unwrap();
                let formatted_date = datetime
                    .format(time::macros::format_description!(
                        "[month repr:short] [day] [hour]:[minute]"
                    ))
                    .unwrap();

                // Mostly copying "The Long Format" from `man ls`.
                // TODO: query user/group database to convert uid/gid to names?
                result.push(format!(
                    "{}{} {} {} {} {} {} {}",
                    format_ftype(&md),
                    formatted_mode,
                    md.nlink(),
                    md.uid(),
                    md.gid(),
                    md.size(),
                    formatted_date,
                    ent.path().to_string_lossy()
                ));
                if md.is_dir() {
                    dir_queue.push_back((ent.path(), depth + 1));
                }
            }
        }

        let file_count = result.len();
        let result = result.join("\n");
        let byte_count = result.len();
        if byte_count > MAX_TOOL_RESPONSE_SIZE {
            bail!(
                "This tool only supports reading up to {MAX_TOOL_RESPONSE_SIZE} bytes at a time. You tried to read {byte_count} bytes ({file_count} files). Try executing with fewer lines specified."
            );
        }

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    fn depth(&self) -> usize {
        self.depth.unwrap_or(Self::DEFAULT_DEPTH)
    }
}

/// Converts negative 1-based indices to positive 0-based indices.
fn convert_negative_index(line_count: usize, i: i32) -> usize {
    if i <= 0 {
        (line_count as i32 + i).max(0) as usize
    } else {
        i as usize - 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchMatch {
    line_number: usize,
    context: String,
}

fn format_ftype(md: &Metadata) -> char {
    if md.is_symlink() {
        'l'
    } else if md.is_file() {
        '-'
    } else if md.is_dir() {
        'd'
    } else {
        warn!("unknown file metadata: {:?}", md);
        '-'
    }
}

/// Formats a permissions mode into the form used by `ls`, e.g. `0o644` to `rw-r--r--`
fn format_mode(mode: u32) -> [char; 9] {
    let mut mode = mode & 0o777;
    let mut res = ['-'; 9];
    fn octal_to_chars(val: u32) -> [char; 3] {
        match val {
            1 => ['-', '-', 'x'],
            2 => ['-', 'w', '-'],
            3 => ['-', 'w', 'x'],
            4 => ['r', '-', '-'],
            5 => ['r', '-', 'x'],
            6 => ['r', 'w', '-'],
            7 => ['r', 'w', 'x'],
            _ => ['-', '-', '-'],
        }
    }
    for c in res.rchunks_exact_mut(3) {
        c.copy_from_slice(&octal_to_chars(mode & 0o7));
        mode /= 0o10;
    }
    res
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
    fn test_negative_index_conversion() {
        assert_eq!(convert_negative_index(5, -100), 0);
        assert_eq!(convert_negative_index(5, -1), 4);
    }

    #[test]
    fn test_fs_read_deser() {
        serde_json::from_value::<FsRead>(serde_json::json!({ "path": "/test_file.txt", "mode": "Line" })).unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "path": "/test_file.txt", "mode": "Line", "end_line": 5 }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "path": "/test_file.txt", "mode": "Line", "start_line": -1 }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "path": "/test_file.txt", "mode": "Line", "start_line": None::<usize> }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(serde_json::json!({ "path": "/", "mode": "Directory" })).unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "path": "/test_file.txt", "mode": "Directory", "depth": 2 }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "path": "/test_file.txt", "mode": "Search", "pattern": "hello" }),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_fs_read_line_invoke() {
        let ctx = setup_test_directory().await;
        let lines = TEST_FILE_CONTENTS.lines().collect::<Vec<_>>();
        let mut stdout = std::io::stdout();

        macro_rules! assert_lines {
            ($start_line:expr, $end_line:expr, $expected:expr) => {
                let v = serde_json::json!({
                    "path": TEST_FILE_PATH,
                    "mode": "Line",
                    "start_line": $start_line,
                    "end_line": $end_line,
                });
                let output = serde_json::from_value::<FsRead>(v)
                    .unwrap()
                    .invoke(&ctx, &mut stdout)
                    .await
                    .unwrap();

                if let OutputKind::Text(text) = output.output {
                    assert_eq!(text, $expected.join("\n"), "actual(left) does not equal
                                expected(right) for (start_line, end_line): ({:?}, {:?})", $start_line, $end_line);
                } else {
                    panic!("expected text output");
                }
            }
        }
        assert_lines!(None::<i32>, None::<i32>, lines[..]);
        assert_lines!(1, 2, lines[..=1]);
        assert_lines!(1, -1, lines[..]);
        assert_lines!(2, 1, lines[1..=1]);
        assert_lines!(-2, -1, lines[2..]);
        assert_lines!(-2, None::<i32>, lines[2..]);
        assert_lines!(2, None::<i32>, lines[1..]);
    }

    #[tokio::test]
    async fn test_fs_read_line_past_eof() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "mode": "Line",
            "start_line": 100,
            "end_line": None::<i32>,
        });
        assert!(
            serde_json::from_value::<FsRead>(v)
                .unwrap()
                .invoke(&ctx, &mut stdout)
                .await
                .is_err()
        );
    }

    #[test]
    fn test_format_mode() {
        macro_rules! assert_mode {
            ($actual:expr, $expected:expr) => {
                assert_eq!(format_mode($actual).iter().collect::<String>(), $expected);
            };
        }
        assert_mode!(0o000, "---------");
        assert_mode!(0o700, "rwx------");
        assert_mode!(0o744, "rwxr--r--");
        assert_mode!(0o641, "rw-r----x");
    }

    #[tokio::test]
    async fn test_fs_read_directory_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Testing without depth
        let v = serde_json::json!({
            "mode": "Directory",
            "path": "/",
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert_eq!(text.lines().collect::<Vec<_>>().len(), 4);
        } else {
            panic!("expected text output");
        }

        // Testing with depth level 1
        let v = serde_json::json!({
            "mode": "Directory",
            "path": "/",
            "depth": 1,
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let lines = text.lines().collect::<Vec<_>>();
            assert_eq!(lines.len(), 7);
            assert!(
                !lines.iter().any(|l| l.contains("cccc1")),
                "directory at depth level 2 should not be included in output"
            );
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_search_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        macro_rules! invoke_search {
            ($value:tt) => {{
                let v = serde_json::json!($value);
                let output = serde_json::from_value::<FsRead>(v)
                    .unwrap()
                    .invoke(&ctx, &mut stdout)
                    .await
                    .unwrap();

                if let OutputKind::Text(value) = output.output {
                    serde_json::from_str::<Vec<SearchMatch>>(&value).unwrap()
                } else {
                    panic!("expected Text output")
                }
            }};
        }

        let matches = invoke_search!({
            "mode": "Search",
            "path": TEST_FILE_PATH,
            "pattern": "hello",
        });
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 1);
        assert_eq!(
            matches[0].context,
            format!(
                "{}1: 1: Hello world!\n{}2: 2: This is line 2\n{}3: 3: asdf\n",
                FsSearch::MATCHING_LINE_PREFIX,
                FsSearch::CONTEXT_LINE_PREFIX,
                FsSearch::CONTEXT_LINE_PREFIX
            )
        );
    }
}
