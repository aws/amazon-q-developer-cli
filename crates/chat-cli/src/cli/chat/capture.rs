use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;

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
// The shadow repo path that MUST be appended with a session-specific directory
// pub const SHADOW_REPO_DIR: &str = "/Users/aws/.amazonq/cli-captures/";

// CURRENT APPROACH:
// We only enable automatically enable checkpoints when the user is already in a git repo.
// Otherwise, the user must manually enable checkpoints using `/checkpoint init`.
// This is done so the user is aware that initializing checkpoints outside of a git repo may
// lead to long startup times.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManager {
    pub shadow_repo_path: PathBuf,
    pub captures: Vec<Capture>,
    pub tag_to_index: HashMap<String, usize>,
    pub num_turns: usize,
    pub num_tools_this_turn: usize,
    pub last_user_message: Option<String>,
    pub user_message_lock: bool,
    /// If true, delete the current session's shadow repo directory when dropped.
    #[serde(default)]
    pub clean_on_drop: bool,
    /// Track file changes for each capture
    #[serde(default)]
    pub file_changes: HashMap<String, FileChangeStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileChangeStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
    pub tag: String,
    pub timestamp: DateTime<Local>,
    pub message: String,
    pub history_index: usize,

    pub is_turn: bool,
    pub tool_name: Option<String>,
}

impl CaptureManager {
    pub async fn auto_init(os: &Os, shadow_path: impl AsRef<Path>) -> Result<Self> {
        if !is_git_installed() {
            bail!("Captures could not be enabled because git is not installed.");
        }
        if !is_in_git_repo() {
            bail!(
                "Must be in a git repo for automatic capture initialization. Use /capture init to manually enable captures."
            );
        }
        // Reuse bare repo init to keep storage model consistent.
        let mut s = Self::manual_init(os, shadow_path).await?;
        // Auto-initialized captures are considered ephemeral: clean when session ends.
        s.clean_on_drop = true;
        Ok(s)
    }

    pub async fn manual_init(os: &Os, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        os.fs.create_dir_all(path).await?;

        let output = Command::new("git")
            .args(["init", "--bare", &path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            bail!("git init failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        config(&path.to_string_lossy())?;
        stage_commit_tag(&path.to_string_lossy(), "Initial capture", "0")?;

        let captures = vec![Capture {
            tag: "0".to_string(),
            timestamp: Local::now(),
            message: "Initial capture".to_string(),
            history_index: 0,
            is_turn: true,
            tool_name: None,
        }];

        let mut tag_to_index = HashMap::new();
        tag_to_index.insert("0".to_string(), 0);

        Ok(Self {
            shadow_repo_path: path.to_path_buf(),
            captures,
            tag_to_index,
            num_turns: 0,
            num_tools_this_turn: 0,
            last_user_message: None,
            user_message_lock: false,
            clean_on_drop: false,
            file_changes: HashMap::new(),
        })
    }

    pub fn get_file_changes(&self, tag: &str) -> Result<FileChangeStats> {
        let git_dir_arg = format!("--git-dir={}", self.shadow_repo_path.display());

        // Get diff stats against previous tag
        let prev_tag = if tag == "0" {
            return Ok(FileChangeStats::default());
        } else {
            self.get_previous_tag(tag)?
        };

        let output = Command::new("git")
            .args([&git_dir_arg, "diff", "--name-status", &prev_tag, tag])
            .output()?;

        if !output.status.success() {
            bail!("Failed to get diff stats: {}", String::from_utf8_lossy(&output.stderr));
        }

        let mut stats = FileChangeStats::default();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(first_char) = line.chars().next() {
                match first_char {
                    'A' => stats.added += 1,
                    'M' => stats.modified += 1,
                    'D' => stats.deleted += 1,
                    _ => {},
                }
            }
        }

        Ok(stats)
    }

    fn get_previous_tag(&self, tag: &str) -> Result<String> {
        // Parse tag format "X" or "X.Y" to get previous
        if let Ok(turn) = tag.parse::<usize>() {
            if turn > 0 {
                return Ok((turn - 1).to_string());
            }
        } else if tag.contains('.') {
            let parts: Vec<&str> = tag.split('.').collect();
            if parts.len() == 2 {
                if let Ok(tool_num) = parts[1].parse::<usize>() {
                    if tool_num > 1 {
                        return Ok(format!("{}.{}", parts[0], tool_num - 1));
                    } else {
                        return Ok(parts[0].to_string());
                    }
                }
            }
        }
        Ok("0".to_string())
    }

    pub fn create_capture_with_stats(
        &mut self,
        tag: &str,
        commit_message: &str,
        history_index: usize,
        is_turn: bool,
        tool_name: Option<String>,
    ) -> Result<()> {
        self.create_capture(tag, commit_message, history_index, is_turn, tool_name)?;

        // Store file change stats
        if let Ok(stats) = self.get_file_changes(tag) {
            self.file_changes.insert(tag.to_string(), stats);
        }

        Ok(())
    }

    pub fn create_capture(
        &mut self,
        tag: &str,
        commit_message: &str,
        history_index: usize,
        is_turn: bool,
        tool_name: Option<String>,
    ) -> Result<()> {
        stage_commit_tag(&self.shadow_repo_path.to_string_lossy(), commit_message, tag)?;

        self.captures.push(Capture {
            tag: tag.to_string(),
            timestamp: Local::now(),
            message: commit_message.to_string(),
            history_index,
            is_turn,
            tool_name,
        });
        self.tag_to_index.insert(tag.to_string(), self.captures.len() - 1);

        Ok(())
    }

    pub fn restore_capture(&self, conversation: &mut ConversationState, tag: &str, hard: bool) -> Result<()> {
        let capture = self.get_capture(tag)?;
        let git_dir_arg = format!("--git-dir={}", self.shadow_repo_path.display());
        let output = if !hard {
            Command::new("git")
                .args([&git_dir_arg, "--work-tree=.", "checkout", tag, "--", "."])
                .output()?
        } else {
            Command::new("git")
                .args([&git_dir_arg, "--work-tree=.", "reset", "--hard", tag])
                .output()?
        };

        if !output.status.success() {
            bail!("git reset failed: {}", String::from_utf8_lossy(&output.stdout));
        }

        for _ in capture.history_index..conversation.history().len() {
            conversation
                .pop_from_history()
                .ok_or(eyre!("Tried to pop from empty history"))?;
        }

        Ok(())
    }

    pub async fn clean(&self, os: &Os) -> eyre::Result<()> {
        // In bare mode, shadow_repo_path is the session directory to delete.
        let path = &self.shadow_repo_path;

        println!("Deleting path: {}", path.display());

        if !path.exists() {
            return Ok(());
        }

        os.fs.remove_dir_all(path).await?;
        Ok(())
    }

    /// Delete the entire captures root (i.e., remove all session captures).
    /// This re-creates the empty root directory after deletion.
    pub async fn clean_all_sessions(&self, os: &Os) -> Result<()> {
        let root = self
            .shadow_repo_path
            .parent()
            .ok_or_else(|| eyre!("Could not determine captures root"))?;

        // Safety guard: ensure last component looks like "cli-captures"
        if root
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.contains("captures"))
            != Some(true)
        {
            bail!("Refusing to delete unexpected parent directory: {}", root.display());
        }

        println!("Deleting captures root: {}", root.display());
        os.fs.remove_dir_all(root).await?;
        os.fs.create_dir_all(root).await?; // recreate empty root
        Ok(())
    }

    pub fn diff_detailed(&self, tag1: &str, tag2: &str) -> Result<String> {
        let git_dir_arg = format!("--git-dir={}", self.shadow_repo_path.display());

        let output = Command::new("git")
            .args([&git_dir_arg, "diff", "--name-status", tag1, tag2])
            .output()?;

        if !output.status.success() {
            bail!("Failed to get diff: {}", String::from_utf8_lossy(&output.stderr));
        }

        let mut result = String::new();

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((status, file)) = line.split_once('\t') {
                match status {
                    "A" => result.push_str(&format!("  + {} (added)\n", file).green().to_string()),
                    "M" => result.push_str(&format!("  ~ {} (modified)\n", file).yellow().to_string()),
                    "D" => result.push_str(&format!("  - {} (deleted)\n", file).red().to_string()),
                    _ => {},
                }
            }
        }

        let output = Command::new("git")
            .args([&git_dir_arg, "diff", tag1, tag2, "--stat", "--color=always"])
            .output()?;

        if output.status.success() {
            result.push_str("\n");
            result.push_str(&String::from_utf8_lossy(&output.stdout));
        }

        Ok(result)
    }

    fn get_capture(&self, tag: &str) -> Result<&Capture> {
        let Some(index) = self.tag_to_index.get(tag) else {
            bail!("No capture with tag {tag}");
        };
        Ok(&self.captures[*index])
    }

    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let git_dir_arg = format!("--git-dir={}", self.shadow_repo_path.display());
        let output = Command::new("git")
            .args([&git_dir_arg, "--work-tree=.", "status", "--porcelain"])
            .output()?;

        if !output.status.success() {
            bail!("git status failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(!output.stdout.is_empty())
    }
}

impl Drop for CaptureManager {
    fn drop(&mut self) {
        // Only clean if this session was auto-initialized (ephemeral).
        if !self.clean_on_drop {
            return;
        }
        let path = self.shadow_repo_path.clone();
        // Prefer spawning on an active Tokio runtime if available.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                // Best-effort: swallow errors; we don't want to block Drop or panic here.
                let _ = tokio::fs::remove_dir_all(path).await;
            });
            return;
        }

        // Fallback: spawn a detached background thread. Still non-blocking.
        let _ = std::thread::Builder::new()
            .name("q-capture-cleaner".into())
            .spawn(move || {
                let _ = std::fs::remove_dir_all(&path);
            });
    }
}

pub const CAPTURE_MESSAGE_MAX_LENGTH: usize = 60;

pub fn truncate_message(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }

    let truncated = &s[..max_chars];
    match truncated.rfind(' ') {
        Some(pos) => format!("{}...", &truncated[..pos]),
        None => format!("{}...", truncated),
    }
}

pub fn is_git_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn is_in_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn stage_commit_tag(shadow_path: &str, commit_message: &str, tag: &str) -> Result<()> {
    let git_dir_arg = format!("--git-dir={}", shadow_path);
    let output = Command::new("git")
        .args([&git_dir_arg, "--work-tree=.", "add", "-A"])
        .output()?;

    if !output.status.success() {
        bail!("git add failed: {}", String::from_utf8_lossy(&output.stdout));
    }

    let output = Command::new("git")
        .args([
            &git_dir_arg,
            "--work-tree=.",
            "commit",
            "--allow-empty",
            "--no-verify",
            "-m",
            commit_message,
        ])
        .output()?;

    if !output.status.success() {
        bail!("git commit failed: {}", String::from_utf8_lossy(&output.stdout));
    }

    let output = Command::new("git").args([&git_dir_arg, "tag", tag]).output()?;

    if !output.status.success() {
        bail!("git tag failed: {}", String::from_utf8_lossy(&output.stdout));
    }
    Ok(())
}

pub fn config(shadow_path: &str) -> Result<()> {
    let git_dir_arg = format!("--git-dir={}", shadow_path);
    let output = Command::new("git")
        .args([&git_dir_arg, "config", "user.name", "Q"])
        .output()?;

    if !output.status.success() {
        bail!("git config failed: {}", String::from_utf8_lossy(&output.stdout));
    }

    let output = Command::new("git")
        .args([&git_dir_arg, "config", "user.email", "qcli@local"])
        .output()?;

    if !output.status.success() {
        bail!("git config failed: {}", String::from_utf8_lossy(&output.stdout));
    }

    let output = Command::new("git")
        .args([&git_dir_arg, "config", "core.preloadindex", "true"])
        .output()?;

    if !output.status.success() {
        bail!("git config failed: {}", String::from_utf8_lossy(&output.stdout));
    }
    Ok(())
}
