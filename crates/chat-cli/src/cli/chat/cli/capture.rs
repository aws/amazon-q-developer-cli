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
    /// --hard: reset all files and delete any created since the checkpoint
    Restore {
        tag: Option<String>,
        #[arg(long)]
        hard: bool,
    },

    /// View all checkpoints (turn-level only)
    List {
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Delete shadow repository or the whole captures root (--all)
    Clean {
        /// Delete the entire captures root (all sessions)
        #[arg(long)]
        all: bool,
    },

    /// Display more information about a turn-level checkpoint
    Expand { tag: String },

    /// Display a diff between two checkpoints (default tag2=HEAD)
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

                let tag_missing = |t: &str| t != "HEAD" && !manager.tag_to_index.contains_key(t);
                if tag_missing(&tag1) {
                    execute!(
                        session.stderr,
                        style::Print(
                            format!(
                                "Capture with tag '{}' does not exist! Use /capture list to see available captures\n",
                                tag1
                            )
                            .blue()
                        )
                    )?;
                    session.conversation.capture_manager = Some(manager);
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }
                if tag_missing(&to_tag) {
                    execute!(
                        session.stderr,
                        style::Print(
                            format!(
                                "Capture with tag '{}' does not exist! Use /capture list to see available captures\n",
                                to_tag
                            )
                            .blue()
                        )
                    )?;
                    session.conversation.capture_manager = Some(manager);
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }

                let comparison_text = if to_tag == "HEAD" {
                    format!("Comparing current state with checkpoint [{}]:\n", tag1)
                } else {
                    format!("Comparing checkpoint [{}] with [{}]:\n", tag1, to_tag)
                };
                execute!(session.stderr, style::Print(comparison_text.blue()))?;
                match manager.diff_detailed(&tag1, &to_tag) {
                    Ok(diff) => {
                        if diff.trim().is_empty() {
                            execute!(session.stderr, style::Print("No differences.\n".dark_grey()))?;
                        } else {
                            execute!(session.stderr, style::Print(diff))?;
                        }
                    },
                    Err(e) => {
                        return {
                            session.conversation.capture_manager = Some(manager);
                            Err(ChatError::Custom(format!("Could not display diff: {e}").into()))
                        };
                    },
                }
            },
        }

        session.conversation.capture_manager = Some(manager);
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

// ------------------------------ formatting helpers ------------------------------
pub struct CaptureDisplayEntry {
    pub tag: String,
    pub display_parts: Vec<StyledContent<String>>,
}

impl TryFrom<&Capture> for CaptureDisplayEntry {
    type Error = eyre::Report;

    fn try_from(value: &Capture) -> std::result::Result<Self, Self::Error> {
        let tag = value.tag.clone();
        let mut parts = Vec::new();
        // Keep exact original UX: turn lines start with "[tag] TIMESTAMP - message"
        // tool lines start with "[tag] TOOL_NAME: message"
        parts.push(format!("[{tag}] ",).blue());
        if value.is_turn {
            parts.push(format!("{} - {}", value.timestamp.format("%Y-%m-%d %H:%M:%S"), value.message).reset());
        } else {
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
    /// Attach cached or computed file stats to a *turn-level* display line.
    /// (For `/capture list` we append stats to turn rows only, keeping original UX.)
    fn with_file_stats(capture: &Capture, manager: &CaptureManager) -> Result<Self> {
        let mut entry = Self::try_from(capture)?;

        let stats_opt = manager
            .file_changes
            .get(&capture.tag)
            .cloned()
            .or_else(|| manager.get_file_changes(&capture.tag).ok());

        if let Some(stats) = stats_opt.as_ref() {
            let stats_str = format_file_stats(stats);
            if !stats_str.is_empty() {
                entry.display_parts.push(format!(" ({})", stats_str).dark_grey());
            }
        }
        Ok(entry)
    }
}

fn format_file_stats(stats: &FileChangeStats) -> String {
    // Keep wording to avoid UX drift:
    // "+N files, modified M, -K files"
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

/// Expand a turn-level checkpoint:
fn expand_capture(manager: &CaptureManager, output: &mut impl Write, tag: String) -> Result<()> {
    let capture_index = match manager.tag_to_index.get(&tag) {
        Some(i) => i,
        None => {
            execute!(
                output,
                style::Print(
                    format!("Capture with tag '{tag}' does not exist! Use /capture list to see available captures\n")
                        .blue()
                )
            )?;
            return Ok(());
        },
    };
    let capture = &manager.captures[*capture_index];
    // Turn header: do NOT show file stats here
    let display_entry = CaptureDisplayEntry::try_from(capture)?;
    execute!(output, style::Print(display_entry), style::Print("\n"))?;

    // If the user tries to expand a tool-level checkpoint, return early
    if !capture.is_turn {
        return Ok(());
    } else {
        // Collect tool-level entries with their indices so we can diff against the previous capture.
        let mut items: Vec<(usize, CaptureDisplayEntry)> = Vec::new();
        for i in (0..*capture_index).rev() {
            let c = &manager.captures[i];
            if c.is_turn {
                break;
            }
            items.push((i, CaptureDisplayEntry::try_from(c)?));
        }

        for (idx, entry) in items.iter().rev() {
            // previous capture in creation order (or itself if 0)
            let base_idx = idx.saturating_sub(1);
            let base_tag = &manager.captures[base_idx].tag;
            let curr_tag = &manager.captures[*idx].tag;
            // compute stats between previous capture -> this tool capture
            let badge = manager
                .get_file_changes_between(base_tag, curr_tag)
                .map_or_else(|_| String::new(), |s| format_file_stats(&s));

            if badge.is_empty() {
                execute!(
                    output,
                    style::Print(" └─ ".blue()),
                    style::Print(entry),
                    style::Print("\n")
                )?;
            } else {
                execute!(
                    output,
                    style::Print(" └─ ".blue()),
                    style::Print(entry),
                    style::Print(format!(" ({})", badge).dark_grey()),
                    style::Print("\n")
                )?;
            }
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
