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
use tracing::{
    debug,
    trace,
    warn,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionResult,
};
use crate::agent::tools::{
    ToolExecutionOutput,
    ToolExecutionOutputItem,
};
use crate::agent::util::glob::matches_any_pattern;
use crate::util::path::canonicalize_path_sys;
use crate::util::providers::SystemProvider;

const LS_TOOL_DESCRIPTION: &str = r#"
A tool for listing directory contents.

HOW TO USE:
- Provide the path to the directory you want to view
- Optionally provide a depth to recursively list directory contents
- Optionally provide a list of glob patterns to exclude files and directories from being searched

LIMITATIONS:
- Only 1000 entries will be returned
- Directories containing over 10000 entries will be truncated
"#;

const LS_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "path": {
            "type": "string",
            "description": "Path to the directory"
        },
        "depth": {
            "type": "integer",
            "description": "Depth of a recursive directory listing",
            "default": 0
        },
        "ignore": {
            "type": "array",
            "description": "List of glob patterns to ignore",
            "items": {
                "type": "string",
                "description": "Glob pattern to ignore"
            }
        }
    },
    "required": [
        "path"
    ]
}
"#;

/// Directory names to not search through when performing recursive directory listings.
///
/// The model would have to explicitly search these directories if it wants to.
const IGNORE_PATTERNS: [&str; 7] = ["node_modules", "bin", "build", "dist", "out", ".cache", ".git"];

// The max number of entry listing results to send to the model.
const MAX_LS_ENTRIES: usize = 1000;

/// The maximum amount of entries that will be read within a given directory.
const MAX_ENTRY_COUNT_PER_DIR: usize = 10_000;

impl BuiltInToolTrait for Ls {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Ls
    }

    fn description() -> std::borrow::Cow<'static, str> {
        LS_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        LS_SCHEMA.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ls {
    pub path: String,
    pub depth: Option<usize>,
    pub ignore: Option<Vec<String>>,
}

impl Ls {
    const DEFAULT_DEPTH: usize = 0;

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        let path = self.canonical_path(provider)?;
        if !path.exists() {
            return Err(format!("Directory not found: {}", path.to_string_lossy()));
        }
        if !tokio::fs::symlink_metadata(&path)
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
            prefix.push(format!("User id: {}", user_id));
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
                .map_err(|e| format!("failed to get next entry: {}", e))?
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
                    // Exclude the directory from being searched if it is a commonly ignored
                    // directory.
                    if matches_any_pattern(IGNORE_PATTERNS, entry.path.to_string_lossy()) {
                        continue;
                    }
                    dir_queue.push_back((entry.path.clone(), depth + 1));
                }
            }
        }

        let prefix = prefix.join("\n");
        let result = result.join("\n");
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
            "{}\n{}",
            prefix, result
        ))]))
    }

    fn matches_ignore_patterns(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref().to_string_lossy();
        match &self.ignore {
            Some(patterns) => matches_any_pattern(patterns, path),
            None => false,
        }
    }

    fn canonical_path<P: SystemProvider>(&self, provider: &P) -> Result<PathBuf, String> {
        Ok(PathBuf::from(
            canonicalize_path_sys(&self.path, provider).map_err(|e| e.to_string())?,
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
    use crate::agent::util::test::TestDir;
    use crate::util::test::TestProvider;

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
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new()
            .with_file(("file1.txt", "content1"))
            .await
            .with_file(("file2.txt", "content2"))
            .await;

        let tool = Ls {
            path: test_dir.join("").to_string_lossy().to_string(),
            depth: None,
            ignore: None,
        };

        assert!(tool.validate(&test_provider).await.is_ok());
        let result = tool.execute(&test_provider).await.unwrap();
        assert_eq!(result.items.len(), 1);

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("file1.txt"));
            assert!(content.contains("file2.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_recursive() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new()
            .with_file(("root.txt", "root"))
            .await
            .with_file(("subdir/nested.txt", "nested"))
            .await;

        let tool = Ls {
            path: test_dir.join("").to_string_lossy().to_string(),
            depth: Some(1),
            ignore: None,
        };

        let result = tool.execute(&test_provider).await.unwrap();

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("root.txt"));
            assert!(content.contains("subdir"));
            assert!(content.contains("nested.txt"));
        }
    }

    #[tokio::test]
    async fn test_ls_with_ignore_patterns() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new()
            .with_file(("keep.txt", "keep"))
            .await
            .with_file(("ignore.log", "ignore"))
            .await;

        let tool = Ls {
            path: test_dir.join("").to_string_lossy().to_string(),
            depth: None,
            ignore: Some(vec!["*.log".to_string()]),
        };

        let result = tool.execute(&test_provider).await.unwrap();

        if let ToolExecutionOutputItem::Text(content) = &result.items[0] {
            assert!(content.contains("keep.txt"));
            assert!(!content.contains("ignore.log"));
        }
    }

    #[tokio::test]
    async fn test_ls_validate_nonexistent_directory() {
        let test_provider = TestProvider::new();
        let tool = Ls {
            path: "/nonexistent/directory".to_string(),
            depth: None,
            ignore: None,
        };

        assert!(tool.validate(&test_provider).await.is_err());
    }

    #[tokio::test]
    async fn test_ls_validate_file_not_directory() {
        let test_provider = TestProvider::new();
        let test_dir = TestDir::new().with_file(("file.txt", "content")).await;

        let tool = Ls {
            path: test_dir.join("file.txt").to_string_lossy().to_string(),
            depth: None,
            ignore: None,
        };

        assert!(tool.validate(&test_provider).await.is_err());
    }
}
