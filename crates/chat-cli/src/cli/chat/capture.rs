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
pub const SHADOW_REPO_DIR: &str = "/Users/kiranbug/.amazonq/captures/";

// The maximum size in bytes of the cwd for automatically enabling captures
// Currently set to 4GB
// pub const AUTOMATIC_INIT_THRESHOLD: u64 = 4_294_967_296;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManager {
    pub shadow_repo_path: PathBuf,
    pub captures: Vec<Capture>,
    pub tag_to_index: HashMap<String, usize>,
    pub num_turns: usize,
    pub num_tools_this_turn: usize,

    pub last_user_message: Option<String>,
    pub user_message_lock: bool,
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

        let path = shadow_path.as_ref();
        os.fs.create_dir_all(path).await?;

        let repo_root = get_git_repo_root()?;
        let output = Command::new("git")
            .args([
                "clone",
                "--depth=1",
                &repo_root.to_string_lossy(),
                &path.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            bail!("git clone failed: {}", String::from_utf8_lossy(&output.stdout));
        }

        let cloned_git_dir = path.join(".git");

        // Remove remote origin to sever connection
        let output = Command::new("git")
            .args([
                &format!("--git-dir={}", cloned_git_dir.display()),
                "remote",
                "remove",
                "origin",
            ])
            .output()?;

        if !output.status.success() {
            bail!("git remote remove failed: {}", String::from_utf8_lossy(&output.stdout));
        }

        config(&cloned_git_dir.to_string_lossy())?;
        stage_commit_tag(&cloned_git_dir.to_string_lossy(), "Initial capture", "0")?;

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
            shadow_repo_path: cloned_git_dir,
            captures,
            tag_to_index,
            num_turns: 0,
            num_tools_this_turn: 0,
            last_user_message: None,
            user_message_lock: false,
        })
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
        })
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

    pub async fn clean(&self, os: &Os) -> Result<()> {
        let path = if self.shadow_repo_path.file_name().unwrap() == ".git" {
            self.shadow_repo_path.parent().unwrap()
        } else {
            self.shadow_repo_path.as_path()
        };
        println!("Deleting path: {}", path.display());
        os.fs.remove_dir_all(path).await?;
        Ok(())
    }

    pub fn diff(&self, tag1: &str, tag2: &str) -> Result<String> {
        let _ = self.get_capture(tag1)?;
        let _ = self.get_capture(tag2)?;
        let git_dir_arg = format!("--git-dir={}", self.shadow_repo_path.display());

        let output = Command::new("git")
            .args([&git_dir_arg, "diff", tag1, tag2, "--stat", "--color=always"])
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            bail!("Failed to get diff: {}", String::from_utf8_lossy(&output.stderr));
        }
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

pub fn get_git_repo_root() -> Result<PathBuf> {
    let output = Command::new("git").args(["rev-parse", "--show-toplevel"]).output()?;

    if !output.status.success() {
        bail!(
            "Failed to get git repo root: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    let root = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(root))
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
