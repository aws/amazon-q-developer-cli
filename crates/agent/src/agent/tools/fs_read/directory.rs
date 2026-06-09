use std::collections::VecDeque;
use std::fs::Metadata;
use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs::DirEntry;
#[cfg(unix)]
use tracing::warn;
use tracing::{
    debug,
    trace,
};

use crate::agent::tools::{
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::util::glob::matches_any_pattern;
use crate::util::path::resolve_path_fuzzy;
use crate::util::providers::SystemProvider;

/// Directory names to not search through when performing recursive directory listings.
///
/// The model would have to explicitly search these directories if it wants to.
/// Directory patterns to ignore when traversing (common build/cache directories).
/// Used by ls tool and @directory references.
pub const IGNORE_PATTERNS: [&str; 7] = ["node_modules", ".git", "dist", "build", "out", ".cache", "target"];

// The max number of entry listing results to send to the model.
const MAX_LS_ENTRIES: usize = 1000;

/// The maximum amount of entries that will be read within a given directory.
const MAX_ENTRY_COUNT_PER_DIR: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryOp {
    pub path: String,
    pub depth: Option<usize>,
    pub exclude_patterns: Option<Vec<String>>,
}

impl DirectoryOp {
    const DEFAULT_DEPTH: usize = 0;

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        let path = self.canonical_path(provider)?;
        if !path.exists() {
            return Err(format!("Directory not found: {}", path.to_string_lossy()));
        }
        if !tokio::fs::metadata(&path)
            .await
            .map_err(|e| {
                format!(
                    "failed to check file metadata for path '{}': {}",
                    path.to_string_lossy(),
                    e
                )
            })?
            .is_dir()
        {
            return Err(format!("Path is not a directory: {}", path.to_string_lossy()));
        }
        Ok(())
    }

    pub async fn execute<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let path = self.canonical_path(provider)?;
        let max_depth = self.depth();
        debug!(?path, max_depth, "Reading directory at path with depth");

        // Lines to include before the listing results
        let mut prefix = Vec::new();
        // Directory listing results
        let mut result = Vec::new();

        #[cfg(unix)]
        {
            let user_id = unsafe { libc::geteuid() };
            prefix.push(format!("User id: {user_id}"));
        }

        let mut dir_queue = VecDeque::new();
        dir_queue.push_back((path.clone(), 0));
        while let Some((dir_path, depth)) = dir_queue.pop_front() {
            if depth > max_depth {
                break;
            }

            let mut read_dir = tokio::fs::read_dir(&dir_path)
                .await
                .map_err(|e| format!("failed to read directory path '{}': {}", dir_path.to_string_lossy(), e))?;

            let mut entries = Vec::new();
            let mut exceeded_threshold = false;

            let mut i = 0;
            while let Some(ent) = read_dir
                .next_entry()
                .await
                .map_err(|e| format!("failed to get next entry: {e}"))?
            {
                // Ignore the entry if it matches one of the ignore arguments.
                let entry_path = ent.path();
                if self.matches_ignore_patterns(&entry_path) {
                    trace!("ignoring file: {}", entry_path.to_string_lossy());
                    continue;
                }

                entries.push(Entry::new(ent).await?);
                i += 1;
                if i > MAX_ENTRY_COUNT_PER_DIR {
                    exceeded_threshold = true;
                }
            }

            entries.sort_by_key(|ent| ent.last_modified);
            entries.reverse();

            // Finally, handle results
            for entry in &entries {
                result.push(entry.to_long_format());

                // Break if we've exceeded the Ls result threshold.
                if result.len() > MAX_LS_ENTRIES {
                    prefix.push(format!(
                        "Directory at {} was truncated (has total {}{} entries)",
                        dir_path.to_string_lossy(),
                        entries.len(),
                        if exceeded_threshold { "+" } else { "" }
                    ));
                    break;
                }

                // Otherwise, continue searching
                if entry.metadata.is_dir() {
                    if self.matches_ignore_patterns(&entry.path) {
                        continue;
                    }
                    dir_queue.push_back((entry.path.clone(), depth + 1));
                }
            }
        }

        let prefix = prefix.join("\n");
        let result = result.join("\n");
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
            "{prefix}\n{result}"
        ))]))
    }

    fn matches_ignore_patterns(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        let full_path = path.to_string_lossy();
        let file_name = path.file_name().map(|n| n.to_string_lossy());
        match &self.exclude_patterns {
            Some(patterns) => {
                matches_any_pattern(patterns, &*full_path)
                    || file_name.is_some_and(|name| matches_any_pattern(patterns, &*name))
            },
            None => file_name.is_some_and(|name| matches_any_pattern(IGNORE_PATTERNS, &*name)),
        }
    }

    fn canonical_path<P: SystemProvider>(&self, provider: &P) -> Result<PathBuf, String> {
        Ok(PathBuf::from(
            resolve_path_fuzzy(&self.path, provider).map_err(|e| e.to_string())?,
        ))
    }

    fn depth(&self) -> usize {
        self.depth.unwrap_or(Self::DEFAULT_DEPTH)
    }
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    metadata: Metadata,
    /// Seconds since UNIX Epoch
    last_modified: u64,
}

impl Entry {
    async fn new(ent: DirEntry) -> Result<Self, String> {
        let entry_path = ent.path();

        let metadata = ent
            .metadata()
            .await
            .map_err(|e| format!("failed to get metadata for {}: {}", entry_path.to_string_lossy(), e))?;

        let last_modified = metadata
            .modified()
            .map_err(|e| {
                format!(
                    "failed to get modified time for {}: {}",
                    ent.path().to_string_lossy(),
                    e
                )
            })?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| {
                format!(
                    "modified time for file '{}' is before unix epoch: {}",
                    ent.path().to_string_lossy(),
                    e
                )
            })?
            .as_secs();

        Ok(Self {
            path: entry_path,
            metadata,
            last_modified,
        })
    }

    #[cfg(unix)]
    fn to_long_format(&self) -> String {
        use std::os::unix::fs::{
            MetadataExt,
            PermissionsExt,
        };

        let formatted_mode = format_mode(self.metadata.permissions().mode())
            .into_iter()
            .collect::<String>();

        let datetime = time::OffsetDateTime::from_unix_timestamp(self.last_modified as i64).unwrap();
        let formatted_date = datetime
            .format(time::macros::format_description!(
                "[month repr:short] [day] [hour]:[minute]"
            ))
            .unwrap();

        format!(
            "{}{} {} {} {} {} {} {}",
            format_ftype(&self.metadata),
            formatted_mode,
            self.metadata.nlink(),
            self.metadata.uid(),
            self.metadata.gid(),
            self.metadata.size(),
            formatted_date,
            self.path.to_string_lossy()
        )
    }

    #[cfg(windows)]
    fn to_long_format(&self) -> String {
        use std::os::windows::fs::MetadataExt;

        let datetime = time::OffsetDateTime::from_unix_timestamp(self.last_modified as i64).unwrap();
        let formatted_date = datetime
            .format(time::macros::format_description!(
                "[month repr:short] [day] [hour]:[minute]"
            ))
            .unwrap();

        // Windows doesn't have Unix-style permissions, so we show a simplified format
        let attrs = if self.metadata.is_dir() { "d" } else { "-" };
        let readonly = if self.metadata.permissions().readonly() {
            "r-"
        } else {
            "rw"
        };

        format!(
            "{}{} {} {} {}",
            attrs,
            readonly,
            self.metadata.file_size(),
            formatted_date,
            self.path.to_string_lossy()
        )
    }
}

#[cfg(unix)]
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
#[cfg(unix)]
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
    use super::*;
    use crate::util::test::TestBase;

    #[test]
    #[cfg(unix)]
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
    async fn test_ls_basic_directory() {
        let test_base = TestBase::new()
            .await
            .with_file(("file1.txt", "content1"))
            .await
            .with_file(("file2.txt", "content2"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        assert!(tool.validate(&test_base).await.is_ok());
        let result = tool.execute(&test_base).await.unwrap();
        assert_eq!(result.items.len(), 1);

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("file1.txt"));
            assert!(content.contains("file2.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_recursive() {
        let test_base = TestBase::new()
            .await
            .with_file(("root.txt", "root"))
            .await
            .with_file(("subdir/nested.txt", "nested"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: Some(1),
            exclude_patterns: None,
        };

        let result = tool.execute(&test_base).await.unwrap();

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("root.txt"));
            assert!(content.contains("subdir"));
            assert!(content.contains("nested.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_with_ignore_patterns() {
        let test_base = TestBase::new()
            .await
            .with_file(("keep.txt", "keep"))
            .await
            .with_file(("ignore.log", "ignore"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec!["*.log".to_string()]),
        };

        let result = tool.execute(&test_base).await.unwrap();

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(!content.contains("ignore.log"));
        }
    }

    #[tokio::test]
    async fn test_ls_validate_nonexistent_directory() {
        let test_base = TestBase::new().await;
        let tool = DirectoryOp {
            path: "/nonexistent/directory".to_string(),
            depth: None,
            exclude_patterns: None,
        };

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_ls_validate_file_not_directory() {
        let test_base = TestBase::new().await.with_file(("file.txt", "content")).await;

        let tool = DirectoryOp {
            path: test_base.join("file.txt").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        assert!(tool.validate(&test_base).await.is_err());
    }

    #[tokio::test]
    async fn test_ls_default_excludes_ignore_patterns() {
        let test_base = TestBase::new()
            .await
            .with_file(("keep.txt", "content"))
            .await
            .with_directory(".git")
            .await
            .with_directory("node_modules")
            .await
            .with_directory("build")
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(!content.contains(".git"), "default should exclude .git");
            assert!(!content.contains("node_modules"), "default should exclude node_modules");
            assert!(!content.contains("build"), "default should exclude build");
        }
    }

    #[tokio::test]
    async fn test_ls_empty_excludes_disables_filtering() {
        let test_base = TestBase::new()
            .await
            .with_file(("keep.txt", "content"))
            .await
            .with_directory(".git")
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(content.contains(".git"), "empty excludes should show .git");
        }
    }

    #[tokio::test]
    async fn test_ls_custom_exclude_simple_name() {
        let test_base = TestBase::new()
            .await
            .with_file(("keep.txt", "content"))
            .await
            .with_directory("foo")
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec!["foo".to_string()]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(
                !content.contains("foo"),
                "custom simple name pattern should exclude foo"
            );
        }
    }

    #[tokio::test]
    async fn test_ls_empty_excludes_allows_recursive_traversal() {
        let test_base = TestBase::new().await.with_file(("build/nested.txt", "content")).await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: Some(1),
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("build"), "empty excludes should show build dir");
            assert!(
                content.contains("nested.txt"),
                "empty excludes should recurse into build"
            );
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_validate_symlink_to_directory() {
        let test_base = TestBase::new()
            .await
            .with_file(("target_dir/file.txt", "content"))
            .await;

        tokio::fs::symlink(test_base.join("target_dir"), test_base.join("link_dir"))
            .await
            .unwrap();

        let tool = DirectoryOp {
            path: test_base.join("link_dir").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        assert!(tool.validate(&test_base).await.is_ok());
        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("file.txt"));
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_ls_empty_directory() {
        let test_base = TestBase::new().await.with_directory("empty_dir").await;

        let tool = DirectoryOp {
            path: test_base.join("empty_dir").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        assert!(tool.validate(&test_base).await.is_ok());
        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            // Should have the user id prefix but no entries
            assert!(!content.contains("empty_dir"));
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_long_format_file_metadata() {
        let test_base = TestBase::new().await.with_file(("hello.txt", "hello world")).await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            // Long format: type+perms nlink uid gid size date path
            assert!(content.contains("hello.txt"));
            // File type indicator '-' for regular file
            let lines: Vec<&str> = content.lines().collect();
            let hello_line = lines.iter().find(|l| l.contains("hello.txt")).unwrap();
            assert!(hello_line.starts_with('-'), "regular file should start with '-'");
            // Should contain file size (11 bytes)
            assert!(hello_line.contains("11"), "should show file size");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_long_format_directory_metadata() {
        let test_base = TestBase::new().await.with_directory("mydir").await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            let lines: Vec<&str> = content.lines().collect();
            let dir_line = lines.iter().find(|l| l.contains("mydir")).unwrap();
            assert!(dir_line.starts_with('d'), "directory should start with 'd'");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_symlink_shown_in_listing() {
        let test_base = TestBase::new().await.with_file(("real_file.txt", "content")).await;

        tokio::fs::symlink(test_base.join("real_file.txt"), test_base.join("link.txt"))
            .await
            .unwrap();

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("link.txt"));
            assert!(content.contains("real_file.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_hidden_files_shown_without_filter() {
        let test_base = TestBase::new()
            .await
            .with_file((".hidden", "secret"))
            .await
            .with_file(("visible.txt", "public"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(
                content.contains(".hidden"),
                "hidden files should appear with empty excludes"
            );
            assert!(content.contains("visible.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_hidden_files_filtered_by_pattern() {
        let test_base = TestBase::new()
            .await
            .with_file((".hidden", "secret"))
            .await
            .with_file(("visible.txt", "public"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![".*".to_string()]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(!content.contains(".hidden"), "dotfiles should be filtered");
            assert!(content.contains("visible.txt"));
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let test_base = TestBase::new().await.with_directory("no_read").await;

        // Remove read permission
        let perms = std::fs::Permissions::from_mode(0o000);
        std::fs::set_permissions(test_base.join("no_read"), perms).unwrap();

        let tool = DirectoryOp {
            path: test_base.join("no_read").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        let result = tool.execute(&test_base).await;
        // Restore permissions for cleanup
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(test_base.join("no_read"), perms).unwrap();

        // Should fail with permission error (unless running as root)
        let uid = unsafe { libc::geteuid() };
        if uid != 0 {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("failed to read directory"));
        }
    }

    #[tokio::test]
    async fn test_ls_deep_nesting() {
        let test_base = TestBase::new().await.with_file(("a/b/c/d/e/deep.txt", "deep")).await;

        // depth=0 should only show top level
        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: Some(0),
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            let sep = std::path::MAIN_SEPARATOR;
            assert!(
                content.contains(&format!("{sep}a")) || content.contains("a"),
                "should list top-level directory 'a'"
            );
            assert!(!content.contains("deep.txt"), "depth 0 should not show nested files");
        }

        // depth=5 should reach deep.txt (a/b/c/d/e = 5 levels)
        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: Some(5),
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("deep.txt"), "depth 5 should reach nested file");
        }
    }

    #[tokio::test]
    async fn test_ls_depth_none_defaults_to_zero() {
        let test_base = TestBase::new()
            .await
            .with_file(("top.txt", "top"))
            .await
            .with_file(("sub/nested.txt", "nested"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("top.txt"));
            assert!(content.contains("sub"));
            assert!(!content.contains("nested.txt"), "default depth should not recurse");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_output_contains_user_id() {
        let test_base = TestBase::new().await.with_file(("file.txt", "content")).await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("User id:"), "output should contain user id prefix");
        }
    }

    #[tokio::test]
    async fn test_ls_sorted_by_modified_time_descending() {
        let test_base = TestBase::new().await.with_file(("old.txt", "old")).await;

        // Windows NTFS mtime granularity can be up to 100ms; use a larger delay to guarantee ordering
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let test_base = test_base.with_file(("new.txt", "new")).await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            let new_pos = content.find("new.txt").unwrap();
            let old_pos = content.find("old.txt").unwrap();
            assert!(new_pos < old_pos, "newer files should appear first");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_ls_broken_symlink() {
        let test_base = TestBase::new().await;

        // Create a symlink pointing to a non-existent target
        tokio::fs::symlink("/nonexistent_target", test_base.join("broken_link"))
            .await
            .unwrap();

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec![]),
        };

        // DirEntry::metadata uses lstat (does NOT follow symlinks), so a broken
        // symlink's metadata still resolves and the entry should appear in the listing.
        let result = tool.execute(&test_base).await;
        assert!(result.is_ok(), "broken symlinks should be handled gracefully via lstat");
        let output = result.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &output.items[0] {
            assert!(
                content.contains("broken_link"),
                "broken symlink should appear in directory listing, got: {content}"
            );
        } else {
            panic!("expected Text output");
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_format_mode_all_permissions() {
        assert_eq!(format_mode(0o777).iter().collect::<String>(), "rwxrwxrwx");
        assert_eq!(format_mode(0o755).iter().collect::<String>(), "rwxr-xr-x");
        assert_eq!(format_mode(0o644).iter().collect::<String>(), "rw-r--r--");
        assert_eq!(format_mode(0o600).iter().collect::<String>(), "rw-------");
        assert_eq!(format_mode(0o111).iter().collect::<String>(), "--x--x--x");
        assert_eq!(format_mode(0o222).iter().collect::<String>(), "-w--w--w-");
        assert_eq!(format_mode(0o444).iter().collect::<String>(), "r--r--r--");
    }

    #[test]
    #[cfg(unix)]
    fn test_format_mode_masks_high_bits() {
        // High bits (setuid, setgid, sticky) should be masked out
        assert_eq!(format_mode(0o4755).iter().collect::<String>(), "rwxr-xr-x");
        assert_eq!(format_mode(0o2755).iter().collect::<String>(), "rwxr-xr-x");
        assert_eq!(format_mode(0o1755).iter().collect::<String>(), "rwxr-xr-x");
    }

    #[tokio::test]
    async fn test_ls_multiple_exclude_patterns() {
        let test_base = TestBase::new()
            .await
            .with_file(("keep.txt", "keep"))
            .await
            .with_file(("remove.log", "log"))
            .await
            .with_file(("remove.tmp", "tmp"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: None,
            exclude_patterns: Some(vec!["*.log".to_string(), "*.tmp".to_string()]),
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(!content.contains("remove.log"));
            assert!(!content.contains("remove.tmp"));
        }
    }

    #[tokio::test]
    async fn test_ls_recursive_excludes_apply_to_subdirs() {
        let test_base = TestBase::new()
            .await
            .with_file(("src/main.rs", "fn main() {}"))
            .await
            .with_file(("src/node_modules/pkg.js", "module"))
            .await;

        let tool = DirectoryOp {
            path: test_base.join("").to_string_lossy().to_string(),
            depth: Some(2),
            exclude_patterns: None, // uses default IGNORE_PATTERNS
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("main.rs"));
            assert!(!content.contains("pkg.js"), "node_modules contents should be excluded");
        }
    }
}
