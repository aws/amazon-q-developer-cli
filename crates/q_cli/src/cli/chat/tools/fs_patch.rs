use std::io::{
    self,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

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
use serde::Deserialize;
use tracing::debug;

use super::{
    InvokeOutput,
    OutputKind,
    format_path,
    sanitize_path_tool_arg,
};

/// Tool for patching file(s) with a unified patch.
#[derive(Debug, Clone, Deserialize)]
pub struct FsPatch {
    /// The patch content as a string.
    pub patch_content: String,
    /// Root directory where the patch should be applied.
    pub root_dir: String,
    #[serde(skip)]
    patch: Option<Patch>,
}

impl FsPatch {
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let root_dir = sanitize_path_tool_arg(ctx, &self.root_dir);
        if !root_dir.exists() {
            bail!("Root directory '{}' does not exist", self.root_dir);
        }
        if !root_dir.is_dir() {
            bail!("'{}' is not a directory", self.root_dir);
        }
        if let Err(err) = self.patch() {
            bail!("invalid patch content: {:?}", err);
        }
        Ok(())
    }

    pub fn queue_description(&mut self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        let cwd = ctx.env().current_dir()?;
        let root_dir = sanitize_path_tool_arg(ctx, &self.root_dir);
        let root_dir = format_path(&cwd, root_dir);
        queue!(
            updates,
            style::Print("Applying patch to directory: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&root_dir),
            style::ResetColor,
            style::Print("\n\n")
        )?;
        for file_patch in &self.patch()?.file_patches {
            queue!(
                updates,
                style::SetAttribute(style::Attribute::Bold),
                style::Print(format!("--- a/{}\n", file_patch.target_file)),
                style::Print(format!("+++ b/{}\n", file_patch.reference_file)),
                style::SetAttribute(style::Attribute::Reset)
            )?;
            for hunk in &file_patch.hunks {
                queue!(
                    updates,
                    style::SetForegroundColor(Color::Blue),
                    style::Print(hunk.header_string()),
                    style::SetForegroundColor(Color::Reset),
                )?;
                for line in &hunk.lines {
                    let color = match line {
                        PatchLine::Context(_) => Color::Reset,
                        PatchLine::Addition(_) => Color::Green,
                        PatchLine::Deletion(_) => Color::Red,
                    };
                    queue!(
                        updates,
                        style::SetForegroundColor(color),
                        style::Print(line.to_string()),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        let cwd = ctx.env().current_dir()?;
        let root_dir = sanitize_path_tool_arg(ctx, &self.root_dir);
        let patch = Patch::from_unified_format(&self.patch_content)?;
        let patches = patch.apply(ctx, &root_dir).await?;
        for (path, content) in patches {
            if let Some(parent) = path.parent() {
                ctx.fs().create_dir_all(parent).await?;
            }
            let verb = if path.exists() { "patched" } else { "created" };
            let relative_path = format_path(&cwd, &path);
            ctx.fs().write(&path, content).await?;
            queue!(
                updates,
                style::Print(format!("Successfully {verb} file at ")),
                style::SetForegroundColor(Color::Green),
                style::Print(relative_path),
                style::ResetColor,
                style::Print("\n"),
            )?;
        }

        Ok(InvokeOutput {
            output: OutputKind::Text(format!(
                "Applied patch to {} files in {}",
                patch.file_patches.len(),
                self.root_dir
            )),
        })
    }

    fn patch(&mut self) -> Result<&Patch> {
        Ok(self
            .patch
            .get_or_insert(Patch::from_unified_format(&self.patch_content)?))
    }
}

type PatchResult<T> = std::result::Result<T, PatchError>;

/// Represents a patch file.
#[derive(Debug, Clone)]
pub struct Patch {
    pub file_patches: Vec<FilePatch>,
}

impl Patch {
    /// Parses a patch in the unified format.
    pub fn from_unified_format(content: &str) -> PatchResult<Self> {
        let mut file_patches = Vec::new();
        let mut current_patch: Option<FilePatch> = None;
        let mut current_hunk: Option<PatchHunk> = None;

        for line in content.lines() {
            if line.starts_with("diff --git ") || line.starts_with("index") {
                continue;
            }
            if line.starts_with("--- ") {
                // Finish previous patch if any.
                if let Some(mut patch) = current_patch.take() {
                    if let Some(hunk) = current_hunk.take() {
                        patch.hunks.push(hunk);
                    }
                    file_patches.push(patch);
                }

                // Start a new patch.
                let path = line.strip_prefix("--- ").unwrap_or("").trim();
                // Skip the a/ prefix if present.
                let path = path.strip_prefix("a/").unwrap_or(path);
                current_patch = Some(FilePatch {
                    target_file: path.to_string(),
                    reference_file: String::new(),
                    hunks: Vec::new(),
                });
            } else if line.starts_with("+++ ") {
                if let Some(patch) = current_patch.as_mut() {
                    let path = line.strip_prefix("+++ ").unwrap_or("").trim();
                    // Skip the b/ prefix if present.
                    let path = path.strip_prefix("b/").unwrap_or(path);
                    patch.reference_file = path.to_string();
                }
            } else if line.starts_with("@@ ") {
                // Finish previous hunk if any.
                if let Some(hunk) = current_hunk.take() {
                    if let Some(patch) = current_patch.as_mut() {
                        patch.hunks.push(hunk);
                    }
                }
                // Parse hunk header: @@ -start,count +start,count @@
                let header = line
                    .strip_prefix("@@ ")
                    .and_then(|s| s.split(" @@").next())
                    .ok_or_else(|| PatchError::Format("Invalid hunk header".to_string()))?;

                let parts: Vec<&str> = header.split(' ').collect();
                if parts.len() < 2 {
                    return Err(PatchError::Format("Invalid hunk header format".to_string()));
                }

                let source_part = parts[0]
                    .strip_prefix('-')
                    .ok_or_else(|| PatchError::Format("Invalid source hunk specification".to_string()))?;
                let target_part = parts[1]
                    .strip_prefix('+')
                    .ok_or_else(|| PatchError::Format("Invalid target hunk specification".to_string()))?;
                let (source_start, source_count) = parse_hunk_spec(source_part)?;
                let (target_start, target_count) = parse_hunk_spec(target_part)?;

                current_hunk = Some(PatchHunk {
                    target_start: source_start,
                    target_count: source_count,
                    reference_start: target_start,
                    reference_count: target_count,
                    lines: Vec::new(),
                });
            } else if let Some(hunk) = current_hunk.as_mut() {
                // Parse hunk content
                if let Some(l) = line.strip_prefix(' ') {
                    hunk.lines.push(PatchLine::Context(l.to_string()));
                } else if let Some(l) = line.strip_prefix('+') {
                    hunk.lines.push(PatchLine::Addition(l.to_string()));
                } else if let Some(l) = line.strip_prefix('-') {
                    hunk.lines.push(PatchLine::Deletion(l.to_string()));
                } else if !line.is_empty() {
                    // Unexpected line in hunk
                    return Err(PatchError::Format(format!("Unexpected line in hunk: {}", line)));
                }
            }
        }

        // Add the last hunk and patch if any
        if let Some(mut patch) = current_patch.take() {
            if let Some(hunk) = current_hunk.take() {
                patch.hunks.push(hunk);
            }
            file_patches.push(patch);
        }

        Ok(Patch { file_patches })
    }

    /// Returns the result of applying each file patch, along with the [PathBuf] to the
    /// file which should be written to.
    pub async fn apply(&self, ctx: &Context, root_dir: impl AsRef<Path>) -> PatchResult<Vec<(PathBuf, String)>> {
        let mut results = Vec::new();
        for file_patch in &self.file_patches {
            let result = file_patch.apply(ctx, &root_dir).await?;
            let path = root_dir.as_ref().join(file_patch.target_file.clone());
            results.push((path, result));
        }
        Ok(results)
    }
}

/// Contains changes to a single target file within a patch.
#[derive(Debug, Clone)]
pub struct FilePatch {
    /// The target file path. This is the first file argument in `diff -u`.
    pub target_file: String,
    /// The reference file path for the diff. This is the second file argument in `diff -u`.
    pub reference_file: String,
    pub hunks: Vec<PatchHunk>,
}

impl FilePatch {
    /// Returns the result of applying [Self::hunks] to [Self::target_file].
    async fn apply(&self, ctx: &Context, root_dir: impl AsRef<Path>) -> PatchResult<String> {
        let root_dir = ctx.fs().canonicalize(root_dir.as_ref()).await?;
        let path = root_dir.join(&self.target_file);
        let original_content = if ctx.fs().exists(&path) {
            ctx.fs().read_to_string(&path).await?
        } else {
            debug!(?path, "Creating new file for patch");
            String::new()
        };
        let original_lines: Vec<&str> = original_content.lines().collect();
        let mut result_lines = Vec::new();
        let mut current_line = 0;
        for hunk in &self.hunks {
            // Add lines before the hunk.
            while current_line < hunk.target_start.saturating_sub(1) && current_line < original_lines.len() {
                result_lines.push(original_lines[current_line].to_string());
                current_line += 1;
            }

            for line in &hunk.lines {
                match line {
                    PatchLine::Context(content) => {
                        if current_line < original_lines.len() {
                            if original_lines[current_line] != content {
                                return Err(PatchError::Apply(format!(
                                    "Context mismatch at line {}: expected '{}', found '{}'",
                                    current_line + 1,
                                    content,
                                    original_lines[current_line]
                                )));
                            }
                            result_lines.push(content.clone());
                            current_line += 1;
                        } else {
                            return Err(PatchError::Apply(format!(
                                "Unexpected end of file while applying context at line {}",
                                current_line + 1
                            )));
                        }
                    },
                    PatchLine::Addition(content) => {
                        result_lines.push(content.clone());
                    },
                    PatchLine::Deletion(_) => {
                        // Skip the line in the original file.
                        if current_line < original_lines.len() {
                            current_line += 1;
                        } else {
                            return Err(PatchError::Apply(format!(
                                "Unexpected end of file while applying deletion at line {}",
                                current_line + 1
                            )));
                        }
                    },
                }
            }
        }

        // Add remaining lines after the last hunk.
        while current_line < original_lines.len() {
            result_lines.push(original_lines[current_line].to_string());
            current_line += 1;
        }

        Ok(format!("{}\n", result_lines.join("\n")))
    }
}

/// Represents a single modification (ie, "hunk") to a file.
///
/// # Example
///
/// In the unified format, this looks something like the following:
/// ```text
/// @@ -4,2 +5,3 @@
///  Line 4
/// -Line 5
/// +Line 5 modified
/// +Line 6
/// ```
/// In this example:
/// - `target_start` is 4 (Line 4 starts at line 4 in the original file)
/// - `target_count` is 2 (Line 4, plus the single '-' line being removed)
/// - `reference_start` is 5 (Line 4 starts at line 5 in the reference file being diffed against)
/// - `reference_count` is 3 (Line 4, plus the two '+' lines being added).
#[derive(Debug, Clone)]
pub struct PatchHunk {
    /// 1-indexed start line in the source file.
    pub target_start: usize,
    pub target_count: usize,
    /// 1-indexed start line in the reference file.
    pub reference_start: usize,
    pub reference_count: usize,
    pub lines: Vec<PatchLine>,
}

impl PatchHunk {
    fn header_string(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@\n",
            self.target_start, self.target_count, self.reference_start, self.reference_count
        )
    }
}

impl std::fmt::Display for PatchHunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.header_string())?;
        for line in &self.lines {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}

/// A single line within a patch hunk.
#[derive(Debug, Clone)]
pub enum PatchLine {
    Context(String),
    Addition(String),
    Deletion(String),
}

impl std::fmt::Display for PatchLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchLine::Context(s) => writeln!(f, " {s}"),
            PatchLine::Addition(s) => writeln!(f, "+{s}"),
            PatchLine::Deletion(s) => writeln!(f, "-{s}"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid patch format: {0}")]
    Format(String),
    #[error("Failed to apply patch: {0}")]
    Apply(String),
}

/// Helper function to parse hunk specifications like "1,5" or "1"
fn parse_hunk_spec(spec: &str) -> PatchResult<(usize, usize)> {
    let parts: Vec<&str> = spec.split(',').collect();
    match parts.len() {
        1 => {
            let start = parts[0]
                .parse::<usize>()
                .map_err(|_err| PatchError::Format(format!("Invalid line number: {}", parts[0])))?;
            Ok((start, 1)) // Default count is 1 if not specified
        },
        2 => {
            let start = parts[0]
                .parse::<usize>()
                .map_err(|_err| PatchError::Format(format!("Invalid line number: {}", parts[0])))?;
            let count = parts[1]
                .parse::<usize>()
                .map_err(|_err| PatchError::Format(format!("Invalid line count: {}", parts[1])))?;
            Ok((start, count))
        },
        _ => Err(PatchError::Format(format!("Invalid hunk specification: {}", spec))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fig_os_shim::Context;

    use super::*;

    const TEST_ROOT_DIR: &str = "/";
    const TEST_FILE_PATH: &str = "test_file.txt";
    /// The path to the test file resolved with [TEST_ROOT_DIR].
    const TEST_FILE_ABSOLUTE_PATH: &str = "/test_file.txt";
    const TEST_FILE_2_ABSOLUTE_PATH: &str = "/test_file_2.txt";
    const TEST_FILE_CONTENT: &str = "\
Line 1
Line 2
Line 3
Line 4
Line 5
";
    const TEST_SIMPLE_PATCH_CONTENT: &str = "\
diff --git a/test_file.txt b/test_file.txt
index 1234567..abcdef 100644
--- a/test_file.txt
+++ b/test_file.txt
@@ -1,3 +1,4 @@
 Line 1
-Line 2
+Line 2 modified
+Line 2.5 added
 Line 3
@@ -4,2 +5,2 @@
 Line 4
-Line 5
+Line 5 modified
diff --git a/test_file.txt b/test_file_2.txt
index 6789..ghijkl 100644
--- a/test_file_2.txt
+++ b/test_file.txt
@@ -1,3 +1,4 @@
+Line 1
+Line 2
+Line 2.5 added
+Line 3
";

    macro_rules! make_fs_patch {
        ($json:tt) => {{ serde_json::from_value::<FsPatch>(serde_json::json!($json)).unwrap() }};
    }

    async fn setup_test_directory(ctx: &Context) {
        ctx.fs()
            .write(Path::new(TEST_ROOT_DIR).join(TEST_FILE_PATH), TEST_FILE_CONTENT)
            .await
            .unwrap();
    }

    #[test]
    fn test_fs_patch_deser() {
        serde_json::from_value::<FsPatch>(serde_json::json!({ "root_dir": "/my/dir", "patch_content": "test patch" }))
            .unwrap();
    }

    #[tokio::test]
    async fn test_fs_patch_tool() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        setup_test_directory(&ctx).await;
        let mut stdout = std::io::stdout();

        // When
        make_fs_patch!({ "root_dir": TEST_ROOT_DIR, "patch_content": TEST_SIMPLE_PATCH_CONTENT })
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        // Then
        let result = ctx.fs().read_to_string(TEST_FILE_ABSOLUTE_PATH).await.unwrap();
        assert_eq!(
            result,
            "Line 1\nLine 2 modified\nLine 2.5 added\nLine 3\nLine 4\nLine 5 modified\n"
        );
        let result = ctx.fs().read_to_string(TEST_FILE_2_ABSOLUTE_PATH).await.unwrap();
        assert_eq!(result, "Line 1\nLine 2\nLine 2.5 added\nLine 3\n");
    }

    #[tokio::test]
    async fn test_fs_patch_validate() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        setup_test_directory(&ctx).await;

        // Test non-existent directory
        let mut v =
            make_fs_patch!({ "root_dir": "/non/existent/directory", "patch_content": TEST_SIMPLE_PATCH_CONTENT });
        assert!(
            v.validate(&ctx)
                .await
                .unwrap_err()
                .to_string()
                .contains("does not exist")
        );

        // Test root_dir not a directory
        let mut v = make_fs_patch!({ "root_dir": TEST_FILE_ABSOLUTE_PATH, "patch_content": TEST_SIMPLE_PATCH_CONTENT });
        assert!(
            v.validate(&ctx)
                .await
                .unwrap_err()
                .to_string()
                .contains("not a directory")
        );

        // Test invalid patch
        let mut v = make_fs_patch!({ "root_dir": "/", "patch_content": "@@ abcd" });
        assert!(
            v.validate(&ctx)
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid patch")
        );
    }

    #[test]
    fn test_parse_simple_patch() {
        for patch in [
            Patch::from_unified_format(TEST_SIMPLE_PATCH_CONTENT).unwrap(),
            // Also verify that the added git lines parse the same as with `diff -u`.
            Patch::from_unified_format(
                TEST_SIMPLE_PATCH_CONTENT
                    .lines()
                    .skip(2)
                    .collect::<Vec<_>>()
                    .join("\n")
                    .as_str(),
            )
            .unwrap(),
        ] {
            assert_eq!(patch.file_patches.len(), 2);
            assert_eq!(patch.file_patches[0].target_file, TEST_FILE_PATH);
            assert_eq!(patch.file_patches[0].reference_file, TEST_FILE_PATH);
            assert_eq!(patch.file_patches[0].hunks.len(), 2);
            let hunk1 = &patch.file_patches[0].hunks[0];
            assert_eq!(hunk1.target_start, 1);
            assert_eq!(hunk1.target_count, 3);
            assert_eq!(hunk1.reference_start, 1);
            assert_eq!(hunk1.reference_count, 4);
            assert_eq!(hunk1.lines.len(), 5);
            let hunk2 = &patch.file_patches[0].hunks[1];
            assert_eq!(hunk2.target_start, 4);
            assert_eq!(hunk2.target_count, 2);
            assert_eq!(hunk2.reference_start, 5);
            assert_eq!(hunk2.reference_count, 2);
            assert_eq!(hunk2.lines.len(), 3);
        }
    }
}
