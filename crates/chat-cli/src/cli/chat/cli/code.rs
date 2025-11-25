use std::io::Write;

use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
};
use eyre::Result;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::feature_flags::FeatureFlags;
use crate::os::Os;
use crate::theme::StyledText;

/// Code intelligence commands using LSP servers
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum CodeSubcommand {
    /// Initialize workspace and show detected languages and LSP status
    Init {
        /// Force re-initialization even if already initialized
        #[arg(short, long)]
        force: bool,
    },
}

impl CodeSubcommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Init { .. } => "init",
        }
    }

    pub async fn execute(&self, _os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if code intelligence feature is enabled
        if !FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            use crossterm::style::{
                Color,
                Stylize,
            };
            let error_msg = format!(
                "{}: unrecognized subcommand {}",
                "error".with(Color::Red),
                format!("'{}'", self.name()).with(Color::Yellow)
            );
            return Err(ChatError::Custom(error_msg.into()));
        }

        match self {
            Self::Init { force } => self.show_workspace_status(session, *force).await,
        }
    }

    async fn show_workspace_status(&self, session: &mut ChatSession, force: bool) -> Result<ChatState, ChatError> {
        // Check if feature is enabled but client wasn't initialized at startup
        if session.conversation.code_intelligence_client.is_none() && FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            queue!(
                session.stderr,
                StyledText::error_fg(),
                style::Print("Code intelligence feature was enabled after chat started.\n"),
                style::Print("Please restart the chat session to use code intelligence features.\n"),
                StyledText::reset(),
            )?;
            session.stderr.flush()?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        if let Some(code_client) = &mut session.conversation.code_intelligence_client {
            // Force re-initialization if requested
            if force && code_client.is_initialized() {
                code_client.reset_initialization().await;
            }

            // Initialize if not already initialized
            if !code_client.is_initialized() {
                match code_client.initialize().await {
                    Ok(_) => {
                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Workspace initialized\n\n"),
                            StyledText::reset(),
                        )?;
                    },
                    Err(e) => {
                        queue!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print("Failed to initialize workspace: "),
                            StyledText::reset(),
                            style::Print(format!("{e}\n")),
                        )?;
                        session.stderr.flush()?;
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                }
            } else {
                queue!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("✓ Workspace already initialized\n\n"),
                    StyledText::reset(),
                )?;
            }

            // Show workspace info
            match code_client.detect_workspace() {
                Ok(workspace_info) => {
                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("Workspace: "),
                        StyledText::reset(),
                        style::Print(format!("{}\n", workspace_info.root_path.display())),
                    )?;

                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("Detected Languages: "),
                        StyledText::reset(),
                        style::Print(format!("{:?}\n", workspace_info.detected_languages)),
                    )?;

                    if !workspace_info.project_markers.is_empty() {
                        queue!(
                            session.stderr,
                            StyledText::brand_fg(),
                            style::Print("Project Markers: "),
                            StyledText::reset(),
                            style::Print(format!("{:?}\n", workspace_info.project_markers)),
                        )?;
                    }

                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("\nAvailable LSPs:\n"),
                        StyledText::reset(),
                    )?;

                    // Check which languages are detected in this workspace
                    let detected_langs: std::collections::HashSet<String> =
                        workspace_info.detected_languages.iter().cloned().collect();

                    for lsp in &workspace_info.available_lsps {
                        // Determine if this LSP is relevant (supports detected languages)
                        let is_relevant = lsp.languages.iter().any(|lang| detected_langs.contains(lang));

                        let (symbol, status, color) = if lsp.is_initialized {
                            ("✓", "initialized", StyledText::success_fg())
                        } else if lsp.is_available && is_relevant {
                            ("⚠", "not initialized", StyledText::warning_fg())
                        } else if !lsp.is_available {
                            ("✗", "not installed", StyledText::secondary_fg())
                        } else {
                            ("○", "available", StyledText::secondary_fg())
                        };

                        queue!(
                            session.stderr,
                            style::Print(format!("{symbol} ")),
                            style::Print(format!("{} ", lsp.name)),
                            StyledText::secondary_fg(),
                            style::Print(format!("({})", lsp.languages.join(", "))),
                            StyledText::reset(),
                            style::Print(" - "),
                            color,
                            style::Print(status),
                            StyledText::reset(),
                            style::Print("\n"),
                        )?;
                    }
                },
                Err(e) => {
                    queue!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print("Failed to detect workspace: "),
                        StyledText::reset(),
                        style::Print(format!("{e}\n")),
                    )?;
                },
            }
        } else {
            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("Code intelligence client not initialized\n"),
                StyledText::reset(),
                style::Print("Use a code tool to initialize the client automatically\n"),
            )?;
        }

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
