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

use crate::cli::chat::capture::{
    Capture,
    CaptureManager,
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
pub enum CaptureSubcommand {
    /// Initialize captures manually
    Init,

    /// Restore workspace to a capture
    #[command(
        about = "Restore workspace to a capture",
        long_about = r#"Restore files to a capture <tag>. If <tag> is omitted, you'll pick one interactively.

Default mode:
  • Restores tracked file changes
  • Keeps new files created after the capture

With --hard:
  • Exactly matches the capture state
  • Removes files created after the capture"#
    )]
    Restore {
        /// Capture tag (e.g., 3 or 3.1). Leave empty to select interactively.
        tag: Option<String>,

        /// Exactly match capture state (removes newer files)
        #[arg(long)]
        hard: bool,
    },

    /// List all captures
    List {
        /// Limit number of results shown
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Delete the shadow repository
    Clean,

    /// Show details of a capture
    Expand {
        /// Capture tag to expand
        tag: String,
    },

    /// Show differences between captures
    Diff {
        /// First capture tag
        tag1: String,

        /// Second capture tag (defaults to current state)
        #[arg(required = false)]
        tag2: Option<String>,
    },
}

impl CaptureSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if capture is enabled
        if !os.database.settings.get_bool(Setting::EnabledCapture).unwrap_or(false) {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Red),
                style::Print("\nCapture is disabled. Enable it with: q settings chat.enableCapture true\n"),
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
        if session.conversation.capture_manager.is_some() {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Blue),
                style::Print(
                    "✓ Captures are already enabled for this session! Use /capture list to see current captures.\n"
                ),
                style::SetForegroundColor(Color::Reset)
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
                style::SetForegroundColor(Color::Blue),
                style::SetAttribute(Attribute::Bold),
                style::Print(format!(
                    "✓ Captures are enabled! (took {:.2}s)\n",
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
        let Some(manager) = session.conversation.capture_manager.take() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Captures not enabled. Use '/capture init' to enable.\n"),
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
            match gather_turn_captures(&manager) {
                Ok(entries) => {
                    if let Some(idx) = select_capture(&entries, "Select capture to restore:") {
                        Ok(entries[idx].tag.clone())
                    } else {
                        Err(())
                    }
                },
                Err(e) => {
                    session.conversation.capture_manager = Some(manager);
                    return Err(ChatError::Custom(format!("Failed to gather captures: {}", e).into()));
                },
            }
        };

        let tag = match tag_result {
            Ok(tag) => tag,
            Err(_) => {
                session.conversation.capture_manager = Some(manager);
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
                    style::Print(format!("✓ Restored to capture {}\n", tag)),
                    style::SetForegroundColor(Color::Reset),
                    style::SetAttribute(Attribute::Reset),
                )?;
                session.conversation.capture_manager = Some(manager);
            },
            Err(e) => {
                session.conversation.capture_manager = Some(manager);
                return Err(ChatError::Custom(format!("Failed to restore: {}", e).into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_list(session: &mut ChatSession, limit: Option<usize>) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.capture_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Captures not enabled. Use '/capture init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        print_captures(manager, &mut session.stderr, limit)
            .map_err(|e| ChatError::Custom(format!("Could not display all captures: {}", e).into()))?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn handle_clean(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.capture_manager.take() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ ️Captures not enabled.\n"),
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
                session.conversation.capture_manager = Some(manager);
                return Err(ChatError::Custom(format!("Failed to clean: {e}").into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_expand(session: &mut ChatSession, tag: String) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.capture_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ ️Captures not enabled. Use '/capture init' to enable.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        expand_capture(manager, &mut session.stderr, &tag)
            .map_err(|e| ChatError::Custom(format!("Failed to expand capture: {}", e).into()))?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn handle_diff(session: &mut ChatSession, tag1: String, tag2: Option<String>) -> Result<ChatState, ChatError> {
        let Some(manager) = session.conversation.capture_manager.as_ref() else {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️ Captures not enabled. Use '/capture init' to enable.\n"),
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
                    "⚠️ Capture '{}' not found! Use /capture list to see available captures\n",
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
                    "⚠️ Capture '{}' not found! Use /capture list to see available captures\n",
                    tag2
                )),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let header = if tag2 == "HEAD" {
            format!("Changes since capture {}:\n", tag1)
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

struct CaptureDisplay {
    tag: String,
    parts: Vec<StyledContent<String>>,
}

impl CaptureDisplay {
    fn from_capture(capture: &Capture, manager: &CaptureManager) -> Result<Self, eyre::Report> {
        let mut parts = Vec::new();

        // Tag
        parts.push(format!("[{}] ", capture.tag).blue());

        // Content
        if capture.is_turn {
            // Turn capture: show timestamp and description
            parts.push(
                format!(
                    "{} - {}",
                    capture.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    capture.description
                )
                .reset(),
            );

            // Add file stats if available
            if let Some(stats) = manager.file_stats_cache.get(&capture.tag) {
                let stats_str = format_stats(stats);
                if !stats_str.is_empty() {
                    parts.push(format!(" ({})", stats_str).dark_grey());
                }
            }
        } else {
            // Tool capture: show tool name and description
            let tool_name = capture.tool_name.clone().unwrap_or_else(|| "Tool".to_string());
            parts.push(format!("{}: ", tool_name).magenta());
            parts.push(capture.description.clone().reset());
        }

        Ok(Self {
            tag: capture.tag.clone(),
            parts,
        })
    }
}

impl std::fmt::Display for CaptureDisplay {
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

fn gather_turn_captures(manager: &CaptureManager) -> Result<Vec<CaptureDisplay>, eyre::Report> {
    manager
        .captures
        .iter()
        .filter(|c| c.is_turn)
        .map(|c| CaptureDisplay::from_capture(c, manager))
        .collect()
}

fn print_captures(manager: &CaptureManager, output: &mut impl Write, limit: Option<usize>) -> Result<(), eyre::Report> {
    let entries = gather_turn_captures(manager)?;
    let limit = limit.unwrap_or(entries.len());

    for entry in entries.iter().take(limit) {
        execute!(output, style::Print(&entry), style::Print("\n"))?;
    }

    Ok(())
}

fn expand_capture(manager: &CaptureManager, output: &mut impl Write, tag: &str) -> Result<(), eyre::Report> {
    let Some(&idx) = manager.tag_index.get(tag) else {
        execute!(
            output,
            style::SetForegroundColor(Color::Yellow),
            style::Print(format!("⚠️ capture '{}' not found\n", tag)),
            style::SetForegroundColor(Color::Reset),
        )?;
        return Ok(());
    };

    let capture = &manager.captures[idx];

    // Print main capture
    let display = CaptureDisplay::from_capture(capture, manager)?;
    execute!(output, style::Print(&display), style::Print("\n"))?;

    if !capture.is_turn {
        return Ok(());
    }

    // Print tool captures for this turn
    let mut tool_captures = Vec::new();
    for i in (0..idx).rev() {
        let c = &manager.captures[i];
        if c.is_turn {
            break;
        }
        tool_captures.push((i, CaptureDisplay::from_capture(c, manager)?));
    }

    for (capture_idx, display) in tool_captures.iter().rev() {
        // Compute stats for this tool
        let curr_tag = &manager.captures[*capture_idx].tag;
        let prev_tag = if *capture_idx > 0 {
            &manager.captures[capture_idx - 1].tag
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

fn select_capture(entries: &[CaptureDisplay], prompt: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
