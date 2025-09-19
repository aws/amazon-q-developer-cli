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

/// CaptureManager manages a session-scoped "shadow" git repository that tracks
/// user workspace changes and snapshots them into tagged checkpoints.
/// The shadow repo is a bare repo; we use `--work-tree=.` to operate on the cwd.
///
/// Lifecycle:
/// - `auto_init` (preferred when inside a real git repo) or `manual_init`
/// - On each tool use that changes files => stage+commit+tag
/// - `list`/`expand` show checkpoints; `restore` can restore the workspace
/// - If `clean_on_drop` is true (auto init), the session directory is removed on Drop
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
    /// Track file changes for each capture (cached to avoid repeated git calls).
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
    /// Auto initialize capture manager when inside a git repo.
    /// Auto-initialized sessions are ephemeral (`clean_on_drop = true`).
    pub async fn auto_init(os: &Os, shadow_path: impl AsRef<Path>) -> Result<Self> {
        if !is_git_installed() {
            bail!("Captures could not be enabled because git is not installed.");
        }
        if !is_in_git_repo() {
            bail!(
                "Must be in a git repo for automatic capture initialization. Use /capture init to manually enable captures."
            );
        }
        let mut s = Self::manual_init(os, shadow_path).await?;
        s.clean_on_drop = true;
        Ok(s)
    }

    /// Manual initialization: creates a bare repo and tags the initial capture "0".
    pub async fn manual_init(os: &Os, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        os.fs.create_dir_all(path).await?;

        run_git(path, false, &["init", "--bare", &path.to_string_lossy()])?;

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

    /// Return diff stats for `tag` vs its previous tag.
    pub fn get_file_changes(&self, tag: &str) -> Result<FileChangeStats> {
        if tag == "0" {
            return Ok(FileChangeStats::default());
        }
        let prev = previous_tag(tag);
        self.get_file_changes_between(&prev, tag)
    }

    /// Return diff stats for `base..head` using `git diff --name-status`.
    pub fn get_file_changes_between(&self, base: &str, head: &str) -> Result<FileChangeStats> {
        let out = run_git(&self.shadow_repo_path, false, &["diff", "--name-status", base, head])?;
        let mut stats = FileChangeStats::default();

        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // `--name-status` format: "X<TAB>path", or "R100<TAB>old<TAB>new"
            let code = line.split('\t').next().unwrap_or("");
            match code.chars().next().unwrap_or('M') {
                'A' => stats.added += 1,
                'M' => stats.modified += 1,
                'D' => stats.deleted += 1,
                // Treat R/C (rename/copy) as "modified" for user-facing simplicity
                'R' | 'C' => stats.modified += 1,
                _ => {},
            }
        }
        Ok(stats)
    }

    /// Stage, commit, tag and record the stats (if possible).
    pub fn create_capture_with_stats(
        &mut self,
        tag: &str,
        commit_message: &str,
        history_index: usize,
        is_turn: bool,
        tool_name: Option<String>,
    ) -> Result<()> {
        self.create_capture(tag, commit_message, history_index, is_turn, tool_name)?;
        if let Ok(stats) = self.get_file_changes(tag) {
            self.file_changes.insert(tag.to_string(), stats);
        }
        Ok(())
    }

    /// Stage, commit and tag. Also record in-memory `captures` list.
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

    /// Restore files from a given tag.
    /// - soft: checkout changed tracked files
    /// - hard: reset --hard (removes files created since the checkpoint)
    pub fn restore_capture(&self, conversation: &mut ConversationState, tag: &str, hard: bool) -> Result<()> {
        let capture = self.get_capture(tag)?;
        let args = if !hard {
            vec!["checkout", tag, "--", "."]
        } else {
            vec!["reset", "--hard", tag]
        };

        // Use --work-tree=. to affect the real workspace
        let out = run_git(&self.shadow_repo_path, true, &args)?;
        if !out.status.success() {
            bail!("git reset failed: {}", String::from_utf8_lossy(&out.stdout));
        }

        // Trim conversation history back to the point of the capture
        for _ in capture.history_index..conversation.history().len() {
            conversation
                .pop_from_history()
                .ok_or(eyre!("Tried to pop from empty history"))?;
        }
        Ok(())
    }

    /// Remove the session's shadow repo directory.
    pub async fn clean(&self, os: &Os) -> eyre::Result<()> {
        let path = &self.shadow_repo_path;
        println!("Deleting path: {}", path.display());

        if !path.exists() {
            return Ok(());
        }
        os.fs.remove_dir_all(path).await?;
        Ok(())
    }

    /// Produce a user-friendly diff between two tags, including `--stat`.
    pub fn diff_detailed(&self, tag1: &str, tag2: &str) -> Result<String> {
        let out = run_git(&self.shadow_repo_path, false, &["diff", "--name-status", tag1, tag2])?;
        if !out.status.success() {
            bail!("Failed to get diff: {}", String::from_utf8_lossy(&out.stderr));
        }

        let mut result = String::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((status, file)) = line.split_once('\t') {
                match status.chars().next().unwrap_or('M') {
                    'A' => result.push_str(&format!("  + {} (added)\n", file).green().to_string()),
                    'M' => result.push_str(&format!("  ~ {} (modified)\n", file).yellow().to_string()),
                    'D' => result.push_str(&format!("  - {} (deleted)\n", file).red().to_string()),
                    // Treat rename/copy as modified for simplicity
                    'R' | 'C' => result.push_str(&format!("  ~ {} (modified)\n", file).yellow().to_string()),
                    _ => {},
                }
            }
        }

        let stat = run_git(&self.shadow_repo_path, false, &[
            "diff",
            tag1,
            tag2,
            "--stat",
            "--color=always",
        ])?;
        if stat.status.success() {
            result.push_str("\n");
            result.push_str(&String::from_utf8_lossy(&stat.stdout));
        }

        Ok(result)
    }

    fn get_capture(&self, tag: &str) -> Result<&Capture> {
        let Some(index) = self.tag_to_index.get(tag) else {
            bail!("No capture with tag {tag}");
        };
        Ok(&self.captures[*index])
    }

    /// Whether the real workspace has any tracked uncommitted changes
    /// from the perspective of the shadow repo.
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let out = run_git(&self.shadow_repo_path, true, &["status", "--porcelain"])?;
        if !out.status.success() {
            bail!("git status failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(!out.stdout.is_empty())
    }
}

impl Drop for CaptureManager {
    fn drop(&mut self) {
        if !self.clean_on_drop {
            return;
        }
        let path = self.shadow_repo_path.clone();

        // Prefer spawning on an active Tokio runtime if available.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tokio::fs::remove_dir_all(path).await;
            });
            return;
        }

        // Fallback: detached thread.
        let _ = std::thread::Builder::new()
            .name("q-capture-cleaner".into())
            .spawn(move || {
                let _ = std::fs::remove_dir_all(&path);
            });
    }
}

pub const CAPTURE_MESSAGE_MAX_LENGTH: usize = 60;

/// Truncate a message on a word boundary (if possible), appending "…".
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

/// Quick checks for git environment presence
pub fn is_git_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn is_in_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Stage all changes in cwd via the shadow repo, create a commit, and tag it.
pub fn stage_commit_tag(shadow_path: &str, commit_message: &str, tag: &str) -> Result<()> {
    run_git(Path::new(shadow_path), true, &["add", "-A"])?;

    let out = run_git(Path::new(shadow_path), true, &[
        "commit",
        "--allow-empty",
        "--no-verify",
        "-m",
        commit_message,
    ])?;
    if !out.status.success() {
        bail!("git commit failed: {}", String::from_utf8_lossy(&out.stdout));
    }

    let out = run_git(Path::new(shadow_path), false, &["tag", tag])?;
    if !out.status.success() {
        bail!("git tag failed: {}", String::from_utf8_lossy(&out.stdout));
    }
    Ok(())
}

/// Configure the bare repo with a friendly user and preloading for speed.
pub fn config(shadow_path: &str) -> Result<()> {
    run_git(Path::new(shadow_path), false, &["config", "user.name", "Q"])?;
    run_git(Path::new(shadow_path), false, &["config", "user.email", "qcli@local"])?;
    run_git(Path::new(shadow_path), false, &["config", "core.preloadindex", "true"])?;
    Ok(())
}

// ------------------------------ Internal helpers ------------------------------

/// Build and run a git command with the session's bare repo.
///
/// - `with_work_tree = true` adds `--work-tree=.` so git acts on the real workspace.
/// - Always injects `--git-dir=<shadow_repo_path>`.
fn run_git(dir: &Path, with_work_tree: bool, args: &[&str]) -> Result<Output> {
    let git_dir_arg = format!("--git-dir={}", dir.display());
    let mut full_args: Vec<String> = vec![git_dir_arg];
    if with_work_tree {
        full_args.push("--work-tree=.".into());
    }
    full_args.extend(args.iter().map(|s| s.to_string()));

    let out = Command::new("git").args(&full_args).output()?;
    if !out.status.success() {
        // Keep stderr for diagnosis; many git errors only print to stderr.
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        if !err.is_empty() {
            bail!(err);
        }
    }
    Ok(out)
}

/// Compute previous tag for a given tag string:
/// - "X"      => (X-1) or "0" if X == 0
/// - "X.Y"    => same turn previous tool if Y>1, else "X"
/// - default  => "0"
fn previous_tag(tag: &str) -> String {
    if let Ok(turn) = tag.parse::<usize>() {
        return turn.saturating_sub(1).to_string();
    }
    if let Some((turn, tool)) = tag.split_once('.') {
        if let Ok(tool_num) = tool.parse::<usize>() {
            return if tool_num > 1 {
                format!("{}.{}", turn, tool_num - 1)
            } else {
                turn.to_string()
            };
        }
    }
    "0".to_string()
}
