use std::collections::{
    HashMap,
    HashSet,
};
use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;

use amzn_codewhisperer_client::meta;
use bstr::ByteSlice;
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
use walkdir::WalkDir;

use crate::cli::ConversationState;
use crate::os::Os;

// The shadow repo path that MUST be appended with a session-specific directory
pub const SHADOW_REPO_DIR: &str = "./.amazonq/captures";

pub const CAPTURE_TEST_DIR: &str = "/Users/kiranbug/.amazonq/captures/";

// The maximum size in bytes of the cwd for automatically enabling captures
// Currently set to 4GB
pub const AUTOMATIC_INIT_THRESHOLD: u64 = 4_294_967_296;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManager {
    shadow_repo_path: PathBuf,
    pub captures: Vec<Capture>,
    pub tag_to_index: HashMap<String, usize>,
    pub num_turns: usize,
    pub num_tools_this_turn: usize,
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
    pub fn auto_init(shadow_path: impl AsRef<Path>) -> Result<Self> {
        if !is_git_installed() {
            bail!(
                "Captures could not be enabled because git is not installed. Please install git to enable checkpointing features."
            );
        }
        if !is_in_git_repo() {
            bail!("Must be in a git repo for automatic capture initialization. Use /capture init to manually enable captures.");
        }

        let path = shadow_path.as_ref();

        let repo_root = get_git_repo_root()?;
        let output = Command::new("git")
            .args(&[
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
            .args(&[
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
        stage_commit_tag(&cloned_git_dir.to_string_lossy(), "Inital capture", "0")?;
        
        let mut captures = Vec::new();
        captures.push(Capture {
            tag: "0".to_string(),
            timestamp: Local::now(),
            message: "Initial capture".to_string(),
            history_index: 0,
            is_turn: true,
            tool_name: None,
        });

        let mut tag_to_index = HashMap::new();
        tag_to_index.insert("0".to_string(), 0);

        Ok(Self {
            shadow_repo_path: cloned_git_dir,
            captures,
            tag_to_index,
            num_turns: 0,
            num_tools_this_turn: 0,
        })
    }


    pub fn manual_init(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        let output = Command::new("git")
            .args(&["init", "--bare", &path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            bail!("git init failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        config(&path.to_string_lossy())?;
        stage_commit_tag(&path.to_string_lossy(), "Initial capture", "0")?;
        
        let mut captures = Vec::new();
        captures.push(Capture {
            tag: "0".to_string(),
            timestamp: Local::now(),
            message: "Initial capture".to_string(),
            history_index: 0,
            is_turn: true,
            tool_name: None,
        });

        let mut tag_to_index = HashMap::new();
        tag_to_index.insert("0".to_string(), 0);

        Ok(Self {
            shadow_repo_path: path.to_path_buf(),
            captures,
            tag_to_index,
            num_turns: 0,
            num_tools_this_turn: 0,
        })
    }

    pub fn create_capture(&mut self, tag: &str, commit_message: &str, history_index: usize, is_turn: bool, tool_name: Option<String>) -> Result<()> {
        stage_commit_tag(&self.shadow_repo_path.to_string_lossy(), commit_message, tag)?;

        self.captures.push(Capture {
            tag: tag.to_string(),
            timestamp: Local::now(),
            message: commit_message.to_string(),
            history_index,
            is_turn,
            tool_name
        });
        self.tag_to_index.insert(tag.to_string(), self.captures.len() - 1);

        Ok(())
    }

    pub fn restore_capture(&self, conversation: &mut ConversationState, tag: &str, hard: bool) -> Result<()> {
        let Some(index) = self.tag_to_index.get(tag) else {
            bail!("No capture with tag {tag}");
        };
        let capture = &self.captures[*index];
        let output = if !hard {
            Command::new("git")
                .args([
                    &format!("--git-dir={}", self.shadow_repo_path.display()),
                    "--work-tree=.",
                    "checkout",
                    tag,
                    "--",
                    ".",
                ])
                .output()?
        } else {
            Command::new("git")
                .args([
                    &format!("--git-dir={}", self.shadow_repo_path.display()),
                    "--work-tree=.",
                    "reset",
                    "--hard",
                    tag,
                ])
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

    pub fn has_uncommitted_changes(&self) -> bool {
        Command::new("git")
            .args([
                &format!("--git-dir={}", self.shadow_repo_path.display()),
                "--work-tree=.",
                "status",
                "--porcelain",
            ])
            .output()
            .map(|output| !output.stdout.is_empty())
            .unwrap_or(false)
    }
}

pub const CAPTURE_MESSAGE_MAX_LENGTH: usize = 20;

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
        .args(&["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn get_git_repo_root() -> Result<PathBuf> {
    let output = Command::new("git").args(&["rev-parse", "--show-toplevel"]).output()?;

    if !output.status.success() {
        bail!(
            "Failed to get git repo root: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    let root = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(root))
}

pub fn within_git_threshold(os: &Os) -> Result<bool> {
    let ignored_paths = get_ignored_paths()?;
    let mut total = 0;
    for entry in WalkDir::new(os.env.current_dir()?)
        .into_iter()
        .filter_entry(|e| !ignored_paths.contains(e.path()))
    {
        let entry = entry?;
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
            if total > AUTOMATIC_INIT_THRESHOLD {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

fn get_ignored_paths() -> Result<HashSet<PathBuf>> {
    let rev_parse_output = Command::new("git").args(&["rev-parse", "--show-toplevel"]).output()?;
    let repo_root = PathBuf::from(rev_parse_output.stdout.to_str()?);

    let output = Command::new("git")
        .args(&["ls-files", "--ignored", "--exclude-standard", "-o"])
        .output()?;

    let files = String::from_utf8(output.stdout)?
        .lines()
        .map(|s| repo_root.join(s))
        .collect();

    Ok(files)
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