use std::collections::{
    HashMap,
    VecDeque,
};
use std::path::{
    Path,
    PathBuf,
};
use std::process::{
    Command,
    Output,
};

use chrono::{
    DateTime,
    Local,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    bail,
    eyre,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;

use crate::cli::ConversationState;
use crate::cli::chat::conversation::HistoryEntry;
use crate::os::Os;

/// Manages a shadow git repository for tracking and restoring workspace changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointManager {
    /// Path to the shadow (bare) git repository
    pub shadow_repo_path: PathBuf,

    /// Path to current working directory
    pub work_tree_path: PathBuf,

    /// All checkpoints in chronological order
    pub checkpoints: Vec<Checkpoint>,

    /// Fast lookup: tag -> index in checkpoints vector
    pub tag_index: HashMap<String, usize>,

    /// Track the current turn number
    pub current_turn: usize,

    /// Track tool uses within current turn
    pub tools_in_turn: usize,

    /// Last user message for commit description
    pub pending_user_message: Option<String>,

    /// Whether the message has been locked for this turn
    pub message_locked: bool,

    /// Cached file change statistics
    #[serde(default)]
    pub file_stats_cache: HashMap<String, FileStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub tag: String,
    pub timestamp: DateTime<Local>,
    pub description: String,
    pub history_snapshot: VecDeque<HistoryEntry>,
    pub is_turn: bool,
    pub tool_name: Option<String>,
}

impl CheckpointManager {
    /// Initialize checkpoint manager automatically (when in a git repo)
    pub async fn auto_init(
        os: &Os,
        shadow_path: impl AsRef<Path>,
        current_history: &VecDeque<HistoryEntry>,
    ) -> Result<Self> {
        if !is_git_installed() {
            bail!("Checkpoints are not available. Git is required but not installed.");
        }
        if !is_in_git_repo() {
            bail!("Checkpoints are not available in this directory. Use '/checkpoint init' to enable checkpoints.");
        }

        let manager = Self::manual_init(os, shadow_path, current_history).await?;
        Ok(manager)
    }

    /// Initialize checkpoint manager manually
    pub async fn manual_init(
        os: &Os,
        path: impl AsRef<Path>,
        current_history: &VecDeque<HistoryEntry>,
    ) -> Result<Self> {
        let path = path.as_ref();
        os.fs.create_dir_all(path).await?;

        let work_tree_path =
            std::env::current_dir().map_err(|e| eyre!("Failed to get current working directory: {}", e))?;

        // Initialize bare repository
        run_git(path, None, &["init", "--bare", &path.to_string_lossy()])?;

        // Configure git
        configure_git(&path.to_string_lossy())?;

        // Create initial checkpoint
        stage_commit_tag(&path.to_string_lossy(), &work_tree_path, "Initial state", "0")?;

        let initial_checkpoint = Checkpoint {
            tag: "0".to_string(),
            timestamp: Local::now(),
            description: "Initial state".to_string(),
            history_snapshot: current_history.clone(),
            is_turn: true,
            tool_name: None,
        };

        let mut tag_index = HashMap::new();
        tag_index.insert("0".to_string(), 0);

        Ok(Self {
            shadow_repo_path: path.to_path_buf(),
            work_tree_path,
            checkpoints: vec![initial_checkpoint],
            tag_index,
            current_turn: 0,
            tools_in_turn: 0,
            pending_user_message: None,
            message_locked: false,
            file_stats_cache: HashMap::new(),
        })
    }

    /// Create a new checkpoint point
    pub fn create_checkpoint(
        &mut self,
        tag: &str,
        description: &str,
        history: &VecDeque<HistoryEntry>,
        is_turn: bool,
        tool_name: Option<String>,
    ) -> Result<()> {
        // Stage, commit and tag
        stage_commit_tag(
            &self.shadow_repo_path.to_string_lossy(),
            &self.work_tree_path,
            description,
            tag,
        )?;

        // Record checkpoint metadata
        let checkpoint = Checkpoint {
            tag: tag.to_string(),
            timestamp: Local::now(),
            description: description.to_string(),
            history_snapshot: history.clone(),
            is_turn,
            tool_name,
        };

        // Check if checkpoint with this tag already exists
        if let Some(&existing_idx) = self.tag_index.get(tag) {
            if is_turn {
                // For turn checkpoints, always move to the end to maintain correct ordering
                self.checkpoints.remove(existing_idx);

                // Update all indices in tag_index that are greater than the removed index
                for (_, index) in self.tag_index.iter_mut() {
                    if *index > existing_idx {
                        *index -= 1;
                    }
                }

                // Add the updated checkpoint at the end
                self.checkpoints.push(checkpoint);
                self.tag_index.insert(tag.to_string(), self.checkpoints.len() - 1);
            }
        } else {
            // Add new checkpoint
            self.checkpoints.push(checkpoint);
            self.tag_index.insert(tag.to_string(), self.checkpoints.len() - 1);
        }

        // Cache file stats for this checkpoint
        if let Ok(stats) = self.compute_file_stats(tag) {
            self.file_stats_cache.insert(tag.to_string(), stats);
        }

        Ok(())
    }

    /// Restore workspace to a specific checkpoint
    pub fn restore(&self, conversation: &mut ConversationState, tag: &str, hard: bool) -> Result<()> {
        let checkpoint = self.get_checkpoint(tag)?;

        if hard {
            // Hard: reset the whole work-tree to the tag
            let output = run_git(&self.shadow_repo_path, Some(&self.work_tree_path), &[
                "reset", "--hard", tag,
            ])?;
            if !output.status.success() {
                bail!(
                    "Failed to restore checkpoint: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            // Soft: only restore tracked files. If the tag is an empty tree, this is a no-op.
            if !self.tag_has_any_paths(tag)? {
                // Nothing tracked in this checkpoint -> nothing to restore; treat as success.
                conversation.restore_to_checkpoint(checkpoint)?;
                return Ok(());
            }
            // Use checkout against work-tree
            let output = run_git(&self.shadow_repo_path, Some(&self.work_tree_path), &[
                "checkout", tag, "--", ".",
            ])?;
            if !output.status.success() {
                bail!(
                    "Failed to restore checkpoint: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // Restore conversation history
        conversation.restore_to_checkpoint(checkpoint)?;

        Ok(())
    }

    /// Return true iff the given tag/tree has any tracked paths.
    fn tag_has_any_paths(&self, tag: &str) -> eyre::Result<bool> {
        // Use `git ls-tree -r --name-only <tag>` to check if the tree is empty
        let out = run_git(
            &self.shadow_repo_path,
            // work_tree
            None,
            &["ls-tree", "-r", "--name-only", tag],
        )?;
        Ok(!out.stdout.is_empty())
    }

    /// Get file change statistics for a checkpoint
    pub fn compute_file_stats(&self, tag: &str) -> Result<FileStats> {
        if tag == "0" {
            return Ok(FileStats::default());
        }

        let prev_tag = get_previous_tag(tag);
        self.compute_stats_between(&prev_tag, tag)
    }

    /// Compute file statistics between two checkpoints
    pub fn compute_stats_between(&self, from: &str, to: &str) -> Result<FileStats> {
        let output = run_git(&self.shadow_repo_path, None, &["diff", "--name-status", from, to])?;

        let mut stats = FileStats::default();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((status, _)) = line.split_once('\t') {
                match status.chars().next() {
                    Some('A') => stats.added += 1,
                    Some('M') => stats.modified += 1,
                    Some('D') => stats.deleted += 1,
                    Some('R' | 'C') => stats.modified += 1,
                    _ => {},
                }
            }
        }

        Ok(stats)
    }

    /// Generate detailed diff between checkpoints
    pub fn diff(&self, from: &str, to: &str) -> Result<String> {
        let mut result = String::new();

        // Get file changes
        let output = run_git(&self.shadow_repo_path, None, &["diff", "--name-status", from, to])?;

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((status, file)) = line.split_once('\t') {
                match status.chars().next() {
                    Some('A') => result.push_str(&format!("  + {} (added)\n", file).green().to_string()),
                    Some('M') => result.push_str(&format!("  ~ {} (modified)\n", file).yellow().to_string()),
                    Some('D') => result.push_str(&format!("  - {} (deleted)\n", file).red().to_string()),
                    Some('R' | 'C') => result.push_str(&format!("  ~ {} (renamed)\n", file).yellow().to_string()),
                    _ => {},
                }
            }
        }

        // Add statistics
        let stat_output = run_git(&self.shadow_repo_path, None, &[
            "diff",
            from,
            to,
            "--stat",
            "--color=always",
        ])?;

        if stat_output.status.success() {
            result.push('\n');
            result.push_str(&String::from_utf8_lossy(&stat_output.stdout));
        }

        Ok(result)
    }

    /// Check for uncommitted changes
    pub fn has_changes(&self) -> Result<bool> {
        let output = run_git(&self.shadow_repo_path, Some(&self.work_tree_path), &[
            "status",
            "--porcelain",
        ])?;
        Ok(!output.stdout.is_empty())
    }

    /// Clean up shadow repository
    pub async fn cleanup(&self, os: &Os) -> Result<()> {
        if self.shadow_repo_path.exists() {
            os.fs.remove_dir_all(&self.shadow_repo_path).await?;
        }
        Ok(())
    }

    fn get_checkpoint(&self, tag: &str) -> Result<&Checkpoint> {
        self.tag_index
            .get(tag)
            .and_then(|&idx| self.checkpoints.get(idx))
            .ok_or_else(|| eyre!("Checkpoint '{}' not found", tag))
    }
}

impl Drop for CheckpointManager {
    fn drop(&mut self) {
        let path = self.shadow_repo_path.clone();
        // Try to spawn cleanup task
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tokio::fs::remove_dir_all(path).await;
            });
        } else {
            // Fallback to thread
            std::thread::spawn(move || {
                let _ = std::fs::remove_dir_all(path);
            });
        }
    }
}

// Helper functions

/// Truncate message for display
pub fn truncate_message(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let truncated = &s[..max_len];
    if let Some(pos) = truncated.rfind(' ') {
        format!("{}...", &truncated[..pos])
    } else {
        format!("{}...", truncated)
    }
}

pub const CHECKPOINT_MESSAGE_MAX_LENGTH: usize = 60;

fn is_git_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_in_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn configure_git(shadow_path: &str) -> Result<()> {
    run_git(Path::new(shadow_path), None, &["config", "user.name", "Q"])?;
    run_git(Path::new(shadow_path), None, &["config", "user.email", "qcli@local"])?;
    run_git(Path::new(shadow_path), None, &["config", "core.preloadindex", "true"])?;
    Ok(())
}

fn stage_commit_tag(shadow_path: &str, work_tree: &Path, message: &str, tag: &str) -> Result<()> {
    // Stage all changes
    run_git(Path::new(shadow_path), Some(work_tree), &["add", "-A"])?;

    // Commit
    let output = run_git(Path::new(shadow_path), Some(work_tree), &[
        "commit",
        "--allow-empty",
        "--no-verify",
        "-m",
        message,
    ])?;

    if !output.status.success() {
        bail!(
            "Checkpoint initialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Tag
    let output = run_git(Path::new(shadow_path), None, &["tag", tag, "-f"])?;
    if !output.status.success() {
        bail!(
            "Checkpoint initialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn run_git(dir: &Path, work_tree: Option<&Path>, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new("git");
    cmd.arg(format!("--git-dir={}", dir.display()));

    if let Some(work_tree_path) = work_tree {
        cmd.arg(format!("--work-tree={}", work_tree_path.display()));
    }

    cmd.args(args);

    debug!("Executing git command: {:?}", cmd);
    let output = cmd.output()?;

    if !output.status.success() {
        debug!("Git command failed with exit code: {:?}", output.status.code());
        debug!("Git stderr: {}", String::from_utf8_lossy(&output.stderr));
        debug!("Git stdout: {}", String::from_utf8_lossy(&output.stdout));

        if !output.stderr.is_empty() {
            bail!(
                "Checkpoint operation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            bail!("Checkpoint operation failed unexpectedly");
        }
    }

    debug!("Git command succeeded");
    Ok(output)
}

fn get_previous_tag(tag: &str) -> String {
    // Parse turn.tool format
    if let Some((turn_str, tool_str)) = tag.split_once('.') {
        if let Ok(tool_num) = tool_str.parse::<usize>() {
            return if tool_num > 1 {
                format!("{}.{}", turn_str, tool_num - 1)
            } else {
                turn_str.to_string()
            };
        }
    }

    // Parse turn-only format
    if let Ok(turn) = tag.parse::<usize>() {
        return turn.saturating_sub(1).to_string();
    }

    "0".to_string()
}
