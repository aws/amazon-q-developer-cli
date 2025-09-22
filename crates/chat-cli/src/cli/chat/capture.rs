use std::collections::HashMap;
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

use crate::cli::ConversationState;
use crate::os::Os;

/// Manages a shadow git repository for tracking and restoring workspace changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManager {
    /// Path to the shadow (bare) git repository
    pub shadow_repo_path: PathBuf,

    /// All captures in chronological order
    pub captures: Vec<Capture>,

    /// Fast lookup: tag -> index in captures vector
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
pub struct Capture {
    pub tag: String,
    pub timestamp: DateTime<Local>,
    pub description: String,
    pub history_index: usize,
    pub is_turn: bool,
    pub tool_name: Option<String>,
}

impl CaptureManager {
    /// Initialize capture manager automatically (when in a git repo)
    pub async fn auto_init(os: &Os, shadow_path: impl AsRef<Path>) -> Result<Self> {
        if !is_git_installed() {
            bail!("Git is not installed. Captures require git to function.");
        }
        if !is_in_git_repo() {
            bail!("Not in a git repository. Use '/capture init' to manually enable captures.");
        }

        let manager = Self::manual_init(os, shadow_path).await?;
        Ok(manager)
    }

    /// Initialize capture manager manually
    pub async fn manual_init(os: &Os, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        os.fs.create_dir_all(path).await?;

        // Initialize bare repository
        run_git(path, false, &["init", "--bare", &path.to_string_lossy()])?;

        // Configure git
        configure_git(&path.to_string_lossy())?;

        // Create initial capture
        stage_commit_tag(&path.to_string_lossy(), "Initial state", "0")?;

        let initial_capture = Capture {
            tag: "0".to_string(),
            timestamp: Local::now(),
            description: "Initial state".to_string(),
            history_index: 0,
            is_turn: true,
            tool_name: None,
        };

        let mut tag_index = HashMap::new();
        tag_index.insert("0".to_string(), 0);

        Ok(Self {
            shadow_repo_path: path.to_path_buf(),
            captures: vec![initial_capture],
            tag_index,
            current_turn: 0,
            tools_in_turn: 0,
            pending_user_message: None,
            message_locked: false,
            file_stats_cache: HashMap::new(),
        })
    }

    /// Create a new capture point
    pub fn create_capture(
        &mut self,
        tag: &str,
        description: &str,
        history_index: usize,
        is_turn: bool,
        tool_name: Option<String>,
    ) -> Result<()> {
        // Stage, commit and tag
        stage_commit_tag(&self.shadow_repo_path.to_string_lossy(), description, tag)?;

        // Record capture metadata
        let capture = Capture {
            tag: tag.to_string(),
            timestamp: Local::now(),
            description: description.to_string(),
            history_index,
            is_turn,
            tool_name,
        };

        self.captures.push(capture);
        self.tag_index.insert(tag.to_string(), self.captures.len() - 1);

        // Cache file stats for this capture
        if let Ok(stats) = self.compute_file_stats(tag) {
            self.file_stats_cache.insert(tag.to_string(), stats);
        }

        Ok(())
    }

    /// Restore workspace to a specific capture
    pub fn restore(&self, conversation: &mut ConversationState, tag: &str, hard: bool) -> Result<()> {
        let capture = self.get_capture(tag)?;

        // Restore files
        let args = if hard {
            vec!["reset", "--hard", tag]
        } else {
            vec!["checkout", tag, "--", "."]
        };

        let output = run_git(&self.shadow_repo_path, true, &args)?;
        if !output.status.success() {
            bail!("Failed to restore: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Restore conversation history
        while conversation.history().len() > capture.history_index {
            conversation
                .pop_from_history()
                .ok_or(eyre!("Failed to restore conversation history"))?;
        }

        Ok(())
    }

    /// Get file change statistics for a capture
    pub fn compute_file_stats(&self, tag: &str) -> Result<FileStats> {
        if tag == "0" {
            return Ok(FileStats::default());
        }

        let prev_tag = get_previous_tag(tag);
        self.compute_stats_between(&prev_tag, tag)
    }

    /// Compute file statistics between two captures
    pub fn compute_stats_between(&self, from: &str, to: &str) -> Result<FileStats> {
        let output = run_git(&self.shadow_repo_path, false, &["diff", "--name-status", from, to])?;

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

    /// Generate detailed diff between captures
    pub fn diff(&self, from: &str, to: &str) -> Result<String> {
        let mut result = String::new();

        // Get file changes
        let output = run_git(&self.shadow_repo_path, false, &["diff", "--name-status", from, to])?;

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
        let stat_output = run_git(&self.shadow_repo_path, false, &[
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
        let output = run_git(&self.shadow_repo_path, true, &["status", "--porcelain"])?;
        Ok(!output.stdout.is_empty())
    }

    /// Clean up shadow repository
    pub async fn cleanup(&self, os: &Os) -> Result<()> {
        if self.shadow_repo_path.exists() {
            os.fs.remove_dir_all(&self.shadow_repo_path).await?;
        }
        Ok(())
    }

    fn get_capture(&self, tag: &str) -> Result<&Capture> {
        self.tag_index
            .get(tag)
            .and_then(|&idx| self.captures.get(idx))
            .ok_or_else(|| eyre!("Capture '{}' not found", tag))
    }
}

impl Drop for CaptureManager {
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

pub const CAPTURE_MESSAGE_MAX_LENGTH: usize = 60;

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
    run_git(Path::new(shadow_path), false, &["config", "user.name", "Q"])?;
    run_git(Path::new(shadow_path), false, &["config", "user.email", "qcli@local"])?;
    run_git(Path::new(shadow_path), false, &["config", "core.preloadindex", "true"])?;
    Ok(())
}

fn stage_commit_tag(shadow_path: &str, message: &str, tag: &str) -> Result<()> {
    // Stage all changes
    run_git(Path::new(shadow_path), true, &["add", "-A"])?;

    // Commit
    let output = run_git(Path::new(shadow_path), true, &[
        "commit",
        "--allow-empty",
        "--no-verify",
        "-m",
        message,
    ])?;

    if !output.status.success() {
        bail!("Git commit failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Tag
    let output = run_git(Path::new(shadow_path), false, &["tag", tag])?;
    if !output.status.success() {
        bail!("Git tag failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

fn run_git(dir: &Path, with_work_tree: bool, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new("git");
    cmd.arg(format!("--git-dir={}", dir.display()));

    if with_work_tree {
        cmd.arg("--work-tree=.");
    }

    cmd.args(args);

    let output = cmd.output()?;
    if !output.status.success() && !output.stderr.is_empty() {
        bail!(String::from_utf8_lossy(&output.stderr).to_string());
    }

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
