use std::io::Write;

use clap::Subcommand;
use crossterm::style::{
    StyledContent,
    Stylize,
};
use crossterm::{
    execute,
    style,
};
use dialoguer::Select;
use eyre::Result;

use crate::cli::chat::capture::{
    Capture,
    CaptureManager,
    FileChangeStats,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::util::directories::get_shadow_repo_dir;

#[derive(Debug, PartialEq, Subcommand)]
pub enum CaptureSubcommand {
    /// Manually initialize captures
    Init,

    /// Revert to a specified checkpoint or the most recent if none specified
    // Hard will reset all files and delete files that were created since the
    // checkpoint
    // Not specifying hard only restores modifications/deletions of tracked files
    Restore {
        tag: Option<String>,
        #[arg(long)]
        hard: bool,
    },

    /// View all checkpoints
    List {
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Delete shadow repository
    Clean {
        /// Delete the entire captures root (all sessions)
        #[arg(long)]
        all: bool,
    },

    /// Display more information about a turn-level checkpoint
    Expand { tag: String },

    /// Display a diff between two checkpoints
    Diff {
        tag1: String,
        #[arg(required = false)]
        tag2: Option<String>,
    },
}

impl CaptureSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if let CaptureSubcommand::Init = self {
            if session.conversation.capture_manager.is_some() {
                execute!(
                    session.stderr,
                    style::Print(
                        "Captures are already enabled for this session! Use /capture list to see current captures.\n"
                            .blue()
                    )
                )?;
            } else {
                let path = get_shadow_repo_dir(os, session.conversation.conversation_id().to_string())
                    .map_err(|e| ChatError::Custom(e.to_string().into()))?;
                let start = std::time::Instant::now();
                session.conversation.capture_manager = Some(
                    CaptureManager::manual_init(os, path)
                        .await
                        .map_err(|e| ChatError::Custom(format!("Captures could not be initialized: {e}").into()))?,
                );
                execute!(
                    session.stderr,
                    style::Print(
                        format!("Captures are enabled! (took {:.2}s)\n", start.elapsed().as_secs_f32())
                            .blue()
                            .bold()
                    )
                )?;
            }
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let Some(manager) = session.conversation.capture_manager.take() else {
            execute!(
                session.stderr,
                style::Print("Captures are not enabled for this session\n".blue())
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        match self {
            Self::Init => (),
            Self::Restore { tag, hard } => {
                let tag = if let Some(tag) = tag {
                    tag
                } else {
                    // If the user doesn't provide a tag, allow them to fuzzy select a capture
                    let display_entries = match gather_all_turn_captures(&manager) {
                        Ok(entries) => entries,
                        Err(e) => {
                            session.conversation.capture_manager = Some(manager);
                            return Err(ChatError::Custom(format!("Error getting captures: {e}\n").into()));
                        },
                    };
                    if let Some(index) = fuzzy_select_captures(&display_entries, "Select a capture to restore:") {
                        if index < display_entries.len() {
                            display_entries[index].tag.clone()
                        } else {
                            session.conversation.capture_manager = Some(manager);
                            return Err(ChatError::Custom(
                                format!("Selecting capture with index {index} failed\n").into(),
                            ));
                        }
                    } else {
                        session.conversation.capture_manager = Some(manager);
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    }
                };
                let result = manager.restore_capture(&mut session.conversation, &tag, hard);
                match result {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::Print(format!("Restored capture: {tag}\n").blue().bold())
                        )?;
                    },
                    Err(e) => {
                        session.conversation.capture_manager = Some(manager);
                        return Err(ChatError::Custom(format!("Could not restore capture: {}", e).into()));
                    },
                }
            },
            Self::List { limit } => match print_turn_captures(&manager, &mut session.stderr, limit) {
                Ok(_) => (),
                Err(e) => {
                    session.conversation.capture_manager = Some(manager);
                    return Err(ChatError::Custom(format!("Could not display all captures: {e}").into()));
                },
            },
            Self::Clean { all } => {
                let res = if all {
                    manager.clean_all_sessions(os).await
                } else {
                    manager.clean(os).await
                };
                match res {
                    Ok(()) => execute!(
                        session.stderr,
                        style::Print(
                            if all {
                                "Deleted all session captures under the captures root.\n"
                            } else {
                                "Deleted shadow repository for this session.\n"
                            }
                            .blue()
                            .bold()
                        )
                    )?,
                    Err(e) => {
                        session.conversation.capture_manager = None;
                        return Err(ChatError::Custom(if all {
                            format!("Could not delete captures root: {e}").into()
                        } else {
                            format!("Could not delete shadow repo: {e}").into()
                        }));
                    },
                }
                session.conversation.capture_manager = None;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
            Self::Expand { tag } => match expand_capture(&manager, &mut session.stderr, tag.clone()) {
                Ok(_) => (),
                Err(e) => {
                    session.conversation.capture_manager = Some(manager);
                    return Err(ChatError::Custom(
                        format!("Could not expand checkpoint with tag {}: {e}", tag).into(),
                    ));
                },
            },
            Self::Diff { tag1, tag2 } => {
                // if only provide tag1, compare with current status
                let to_tag = tag2.unwrap_or_else(|| "HEAD".to_string());

                let comparison_text = if to_tag == "HEAD" {
                    format!("Comparing current state with checkpoint [{}]:\n", tag1)
                } else {
                    format!("Comparing checkpoint [{}] with [{}]:\n", tag1, to_tag)
                };

                match manager.diff_detailed(&tag1, &to_tag) {
                    Ok(diff) => {
                        execute!(session.stderr, style::Print(comparison_text.blue()), style::Print(diff))?;
                    },
                    Err(e) => return Err(ChatError::Custom(format!("Could not display diff: {e}").into())),
                }
            },
        }

        session.conversation.capture_manager = Some(manager);
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

pub struct CaptureDisplayEntry {
    pub tag: String,
    pub display_parts: Vec<StyledContent<String>>,
}

impl TryFrom<&Capture> for CaptureDisplayEntry {
    type Error = eyre::Report;

    fn try_from(value: &Capture) -> std::result::Result<Self, Self::Error> {
        let tag = value.tag.clone();
        let mut parts = Vec::new();
        if value.is_turn {
            parts.push(format!("[{tag}] ",).blue());
            parts.push(format!("{} - {}", value.timestamp.format("%Y-%m-%d %H:%M:%S"), value.message).reset());
        } else {
            parts.push(format!("[{tag}] ",).blue());
            parts.push(
                format!(
                    "{}: ",
                    value.tool_name.clone().unwrap_or("No tool provided".to_string())
                )
                .magenta(),
            );
            parts.push(value.message.clone().reset());
        }

        Ok(Self {
            tag,
            display_parts: parts,
        })
    }
}

impl CaptureDisplayEntry {
    fn with_file_stats(capture: &Capture, manager: &CaptureManager) -> Result<Self> {
        let mut entry = Self::try_from(capture)?;

        if let Some(stats) = manager.file_changes.get(&capture.tag) {
            let stats_str = format_file_stats(stats);
            if !stats_str.is_empty() {
                entry.display_parts.push(format!(" ({})", stats_str).dark_grey());
            }
        }

        Ok(entry)
    }
}

fn format_file_stats(stats: &FileChangeStats) -> String {
    let mut parts = Vec::new();

    if stats.added > 0 {
        parts.push(format!(
            "+{} file{}",
            stats.added,
            if stats.added == 1 { "" } else { "s" }
        ));
    }
    if stats.modified > 0 {
        parts.push(format!("modified {}", stats.modified));
    }
    if stats.deleted > 0 {
        parts.push(format!(
            "-{} file{}",
            stats.deleted,
            if stats.deleted == 1 { "" } else { "s" }
        ));
    }

    parts.join(", ")
}

impl std::fmt::Display for CaptureDisplayEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in self.display_parts.iter() {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

fn print_turn_captures(manager: &CaptureManager, output: &mut impl Write, limit: Option<usize>) -> Result<()> {
    let display_entries = gather_all_turn_captures(manager)?;
    for entry in display_entries.iter().take(limit.unwrap_or(display_entries.len())) {
        execute!(output, style::Print(entry), style::Print("\n"))?;
    }
    Ok(())
}

fn gather_all_turn_captures(manager: &CaptureManager) -> Result<Vec<CaptureDisplayEntry>> {
    let mut displays = Vec::new();
    for capture in manager.captures.iter() {
        if !capture.is_turn {
            continue;
        }
        displays.push(CaptureDisplayEntry::with_file_stats(capture, manager)?);
    }
    Ok(displays)
}

fn expand_capture(manager: &CaptureManager, output: &mut impl Write, tag: String) -> Result<()> {
    let capture_index = match manager.tag_to_index.get(&tag) {
        Some(i) => i,
        None => {
            execute!(output, style::Print(format!("Checkpoint with tag '{tag}' does not exist! Use /checkpoint list to see available checkpoints\n").blue()))?;
            return Ok(());
        },
    };
    let capture = &manager.captures[*capture_index];
    let display_entry = CaptureDisplayEntry::with_file_stats(capture, manager)?;
    execute!(output, style::Print(display_entry), style::Print("\n"))?;

    // If the user tries to expand a tool-level checkpoint, return early
    if !capture.is_turn {
        return Ok(());
    } else {
        let mut display_vec = Vec::new();
        for i in (0..*capture_index).rev() {
            let capture = &manager.captures[i];
            if capture.is_turn {
                break;
            }
            display_vec.push(CaptureDisplayEntry::with_file_stats(&manager.captures[i], manager)?);
        }

        for entry in display_vec.iter().rev() {
            execute!(
                output,
                style::Print(" └─ ".blue()),
                style::Print(entry),
                style::Print("\n")
            )?;
        }
    }

    Ok(())
}

fn fuzzy_select_captures(entries: &[CaptureDisplayEntry], prompt_str: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt_str)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
