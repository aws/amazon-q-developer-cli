use std::io::Write;
use std::path::PathBuf;

use clap::Subcommand;
use crossterm::style::{
    StyledContent,
    Stylize,
};
use crossterm::{
    execute,
    style,
};
use dialoguer::FuzzySelect;
use eyre::Result;

use crate::cli::chat::capture::{
    Capture, CaptureManager, SHADOW_REPO_DIR
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

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
    Clean,

    /// Display more information about a turn-level snapshot
    Expand { tag: String },
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
                let path = PathBuf::from(SHADOW_REPO_DIR).join(session.conversation.conversation_id());
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
                            display_entries[index].tag.to_string()
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
            Self::Clean => {
                match manager.clean(os).await {
                    Ok(()) => execute!(session.stderr, style::Print(format!("Deleted shadow repository.\n").blue().bold()))?,
                    Err(e) => {
                        session.conversation.capture_manager = None;
                        return Err(ChatError::Custom(format!("Could not delete shadow repo: {e}").into()));
                    }       
                }
                session.conversation.capture_manager = None;
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
            parts.push(format!("{}", value.message).reset());
        }

        Ok(Self {
            tag,
            display_parts: parts,
        })
    }
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
        displays.push(CaptureDisplayEntry::try_from(capture).unwrap());
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
    let display_entry = CaptureDisplayEntry::try_from(capture)?;
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
            display_vec.push(CaptureDisplayEntry::try_from(&manager.captures[i])?);
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

fn fuzzy_select_captures(entries: &Vec<CaptureDisplayEntry>, prompt_str: &str) -> Option<usize> {
    FuzzySelect::new()
        .with_prompt(prompt_str)
        .items(&entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
