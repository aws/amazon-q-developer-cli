use std::io::Write;

use clap::Subcommand;
use crossterm::style::{
    Attribute,
    Color,
    StyledContent,
    Stylize,
};
use crossterm::{
    execute,
    style,
};
use dialoguer::Select;

use crate::cli::chat::checkpoint::{
    Checkpoint,
    CheckpointManager,
    FileStats,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories::get_shadow_repo_dir;

#[derive(Debug, PartialEq, Subcommand)]
pub enum CheckpointSubcommand {
    /// Initialize checkpoints manually
    Init,

    /// Restore workspace to a checkpoint
    #[command(
        about = "Restore workspace to a checkpoint",
        long_about = r#"Restore files to a checkpoint <tag>. If <tag> is omitted, you'll pick one interactively.

Default mode:
  • Restores tracked file changes
  • Keeps new files created after the checkpoint

With --hard:
  • Exactly matches the checkpoint state
  • Removes files created after the checkpoint"#
    )]
    Restore {
        /// Checkpoint tag (e.g., 3 or 3.1). Leave empty to select interactively.
        tag: Option<String>,

        /// Exactly match checkpoint state (removes newer files)
        #[arg(long)]
        hard: bool,
    },

    /// List all checkpoints
    List {
        /// Limit number of results shown
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Delete the shadow repository
    Clean,

    /// Show details of a checkpoint
    Expand {
        /// Checkpoint tag to expand
        tag: String,
    },

    /// Show differences between checkpoints
    Diff {
        /// First checkpoint tag
        tag1: String,

        /// Second checkpoint tag (defaults to current state)
        #[arg(required = false)]
        tag2: Option<String>,
    },
}

impl CheckpointSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if checkpoint is enabled
        if !os
            .database
            .settings
            .get_bool(Setting::EnabledCheckpoint)
            .unwrap_or(false)
        {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Red),
                style::Print("\nCheckpoint is disabled. Enable it with: q settings chat.enableCheckpoint true\n"),
                style::SetForegroundColor(Color::Reset)
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }
        match self {
            Self::Init => self.handle_init(os, session).await,
            Self::Restore { ref tag, hard } => self.handle_restore(session, tag.clone(), hard).await,
            Self::List { limit } => Self::handle_list(session, limit),
            Self::Clean => self.handle_clean(os, session).await,
            Self::Expand { ref tag } => Self::handle_expand(session, tag.clone()),
            Self::Diff { ref tag1, ref tag2 } => Self::handle_diff(session, tag1.clone(), tag2.clone()),
        }
    }

    async fn handle_init(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if session.conversation.checkpoint_manager.is_some() {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Blue),
                style::Print(
                    "✓ Checkpoints are already enabled for this session! Use /checkpoint list to see current checkpoints.\n"
                ),
                style::SetForegroundColor(Color::Reset)
            )?;
        } else {
            let path = get_shadow_repo_dir(os, session.conversation.conversation_id().to_string())
                .map_err(|e| ChatError::Custom(e.to_string().into()))?;

            let start = std::time::Instant::now();
            session.conversation.checkpoint_manager = Some(
                CheckpointManager::manual_init(os, path)
                    .await
                    .map_err(|e| ChatError::Custom(format!("Checkpoints could not be initialized: {e}").into()))?,
            );

            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Blue),
                style::SetAttribute(Attribute::Bold),
                style::Print(format!(
                    "📷  Checkpoints are enabled! (took {:.2}s)\n",
                    start.elapsed().as_secs_f32()
                )),
                style::SetForegroundColor(Color::Reset),
                style::SetAttribute(Attribute::Reset),
            )?;
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn handle_restore(
        &self,
        session: &mut ChatSession,
        tag: Option<String>,
        hard: bool,
    ) -> Result<ChatState, ChatError> {
        // Take manager out temporarily to avoid borrow issues
        let Some(manager) = session.conversation.checkpoint_manager.take() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Checkpoints not enabled. Use '/checkpoint init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let tag_result = if let Some(tag) = tag {
            Ok(tag)
        } else {
            // Interactive selection
            match gather_turn_checkpoints(&manager) {
                Ok(entries) => {
                    if let Some(idx) = select_checkpoint(&entries, "Select checkpoint to restore:") {
                        Ok(entries[idx].tag.clone())
                    } else {
                        Err(())
                    }
                },
                Err(e) => {
                    session.conversation.checkpoint_manager = Some(manager);
                    return Err(ChatError::Custom(format!("Failed to gather checkpoints: {}", e).into()));
                },
            }
        };

        let tag = match tag_result {
            Ok(tag) => tag,
            Err(_) => {
                session.conversation.checkpoint_manager = Some(manager);
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        match manager.restore(&mut session.conversation, &tag, hard) {
            Ok(_) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Blue),
                    style::SetAttribute(Attribute::Bold),
                    style::Print(format!("✓ Restored to checkpoint {}\n", tag)),
                    style::SetForegroundColor(Color::Reset),
                    style::SetAttribute(Attribute::Reset),
                )?;
                session.conversation.checkpoint_manager = Some(manager);
            },
            Err(e) => {
                session.conversation.checkpoint_manager = Some(manager);
                return Err(ChatError::Custom(format!("Failed to restore: {}", e).into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_list(session: &mut ChatSession, limit: Option<usize>) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.checkpoint_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Checkpoints not enabled. Use '/checkpoint init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        print_checkpoints(manager, &mut session.stderr, limit)
            .map_err(|e| ChatError::Custom(format!("Could not display all checkpoints: {}", e).into()))?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn handle_clean(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.checkpoint_manager.take() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ ️Checkpoints not enabled.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        // Print the path that will be deleted
        execute!(
            session.stderr,
            style::Print(format!("Deleting: {}\n", manager.shadow_repo_path.display()))
        )?;

        match manager.cleanup(os).await {
            Ok(()) => {
                execute!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print("✓ Deleted shadow repository for this session.\n"),
                    style::SetAttribute(Attribute::Reset),
                )?;
            },
            Err(e) => {
                session.conversation.checkpoint_manager = Some(manager);
                return Err(ChatError::Custom(format!("Failed to clean: {e}").into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_expand(session: &mut ChatSession, tag: String) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.checkpoint_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ ️Checkpoints not enabled. Use '/checkpoint init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        expand_checkpoint(manager, &mut session.stderr, &tag)
            .map_err(|e| ChatError::Custom(format!("Failed to expand checkpoint: {}", e).into()))?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_diff(session: &mut ChatSession, tag1: String, tag2: Option<String>) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.checkpoint_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Checkpoints not enabled. Use '/checkpoint init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let tag2 = tag2.unwrap_or_else(|| "HEAD".to_string());

        // Validate tags exist
        if tag1 != "HEAD" && !manager.tag_index.contains_key(&tag1) {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!(
                    "⚠️ Checkpoint '{}' not found! Use /checkpoint list to see available checkpoints\n",
                    tag1
                )),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        if tag2 != "HEAD" && !manager.tag_index.contains_key(&tag2) {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!(
                    "⚠️ Checkpoint '{}' not found! Use /checkpoint list to see available checkpoints\n",
                    tag2
                )),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let header = if tag2 == "HEAD" {
            format!("Changes since checkpoint {}:\n", tag1)
        } else {
            format!("Changes from {} to {}:\n", tag1, tag2)
        };

        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Blue),
            style::Print(header),
            style::SetForegroundColor(Color::Reset),
        )?;

        match manager.diff(&tag1, &tag2) {
            Ok(diff) => {
                if diff.trim().is_empty() {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("No changes.\n"),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                } else {
                    execute!(session.stderr, style::Print(diff))?;
                }
            },
            Err(e) => {
                return Err(ChatError::Custom(format!("Failed to generate diff: {e}").into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

// Display helpers

struct CheckpointDisplay {
    tag: String,
    parts: Vec<StyledContent<String>>,
}

impl CheckpointDisplay {
    fn from_checkpoint(checkpoint: &Checkpoint, manager: &CheckpointManager) -> Result<Self, eyre::Report> {
        let mut parts = Vec::new();

        // Tag
        parts.push(format!("[{}] ", checkpoint.tag).blue());

        // Content
        if checkpoint.is_turn {
            // Turn checkpoint: show timestamp and description
            parts.push(
                format!(
                    "{} - {}",
                    checkpoint.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    checkpoint.description
                )
                .reset(),
            );

            // Add file stats if available
            if let Some(stats) = manager.file_stats_cache.get(&checkpoint.tag) {
                let stats_str = format_stats(stats);
                if !stats_str.is_empty() {
                    parts.push(format!(" ({})", stats_str).dark_grey());
                }
            }
        } else {
            // Tool checkpoint: show tool name and description
            let tool_name = checkpoint.tool_name.clone().unwrap_or_else(|| "Tool".to_string());
            parts.push(format!("{}: ", tool_name).magenta());
            parts.push(checkpoint.description.clone().reset());
        }

        Ok(Self {
            tag: checkpoint.tag.clone(),
            parts,
        })
    }
}

impl std::fmt::Display for CheckpointDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.parts {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

fn format_stats(stats: &FileStats) -> String {
    let mut parts = Vec::new();

    if stats.added > 0 {
        parts.push(format!("+{}", stats.added));
    }
    if stats.modified > 0 {
        parts.push(format!("~{}", stats.modified));
    }
    if stats.deleted > 0 {
        parts.push(format!("-{}", stats.deleted));
    }

    parts.join(" ")
}

fn gather_turn_checkpoints(manager: &CheckpointManager) -> Result<Vec<CheckpointDisplay>, eyre::Report> {
    manager
        .checkpoints
        .iter()
        .filter(|c| c.is_turn)
        .map(|c| CheckpointDisplay::from_checkpoint(c, manager))
        .collect()
}

fn print_checkpoints(
    manager: &CheckpointManager,
    output: &mut impl Write,
    limit: Option<usize>,
) -> Result<(), eyre::Report> {
    let entries = gather_turn_checkpoints(manager)?;
    let limit = limit.unwrap_or(entries.len());

    for entry in entries.iter().take(limit) {
        execute!(output, style::Print(&entry), style::Print("\n"))?;
    }

    Ok(())
}

fn expand_checkpoint(manager: &CheckpointManager, output: &mut impl Write, tag: &str) -> Result<(), eyre::Report> {
    let Some(&idx) = manager.tag_index.get(tag) else {
        execute!(
            output,
            style::SetForegroundColor(Color::Yellow),
            style::Print(format!("⚠️ checkpoint '{}' not found\n", tag)),
            style::SetForegroundColor(Color::Reset),
        )?;
        return Ok(());
    };

    let checkpoint = &manager.checkpoints[idx];

    // Print main checkpoint
    let display = CheckpointDisplay::from_checkpoint(checkpoint, manager)?;
    execute!(output, style::Print(&display), style::Print("\n"))?;

    if !checkpoint.is_turn {
        return Ok(());
    }

    // Print tool checkpoints for this turn
    let mut tool_checkpoints = Vec::new();
    for i in (0..idx).rev() {
        let c = &manager.checkpoints[i];
        if c.is_turn {
            break;
        }
        tool_checkpoints.push((i, CheckpointDisplay::from_checkpoint(c, manager)?));
    }

    for (checkpoint_idx, display) in tool_checkpoints.iter().rev() {
        // Compute stats for this tool
        let curr_tag = &manager.checkpoints[*checkpoint_idx].tag;
        let prev_tag = if *checkpoint_idx > 0 {
            &manager.checkpoints[checkpoint_idx - 1].tag
        } else {
            "0"
        };

        let stats_str = manager
            .compute_stats_between(prev_tag, curr_tag)
            .map(|s| format_stats(&s))
            .unwrap_or_default();

        execute!(
            output,
            style::SetForegroundColor(Color::Blue),
            style::Print(" └─ "),
            style::Print(display),
            style::SetForegroundColor(Color::Reset),
        )?;

        if !stats_str.is_empty() {
            execute!(
                output,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(format!(" ({})", stats_str)),
                style::SetForegroundColor(Color::Reset),
            )?;
        }

        execute!(output, style::Print("\n"))?;
    }

    Ok(())
}

fn select_checkpoint(entries: &[CheckpointDisplay], prompt: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
