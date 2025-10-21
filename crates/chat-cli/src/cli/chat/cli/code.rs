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
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::os::Os;
use crate::theme::StyledText;

/// Code intelligence commands using LSP servers
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum CodeSubcommand {
    /// Show detected workspace, languages, and LSP status
    Status,
    /// Detect and initialize workspace, then show languages and LSP status
    Detect,
}

impl CodeSubcommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Detect => "detect",
        }
    }

    pub async fn execute(
        &self,
        os: &Os,
        session: &mut ChatSession,
    ) -> Result<ChatState, ChatError> {
        // Check if code intelligence experiment is enabled
        if !ExperimentManager::is_enabled(os, ExperimentName::CodeIntelligence) {
            Self::write_feature_disabled_message(session)?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        match self {
            Self::Status => self.execute_status(os, session).await,
            Self::Detect => self.execute_detect(os, session).await,
        }
    }

    fn write_feature_disabled_message(session: &mut ChatSession) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            StyledText::error_fg(),
            style::Print("\nCode intelligence is disabled. Enable it with: q settings chat.enableCodeIntelligence true\n"),
            StyledText::warning_fg(),
            style::Print("💡 Code intelligence provides LSP-based symbol search, references, and workspace analysis.\n\n"),
            StyledText::reset(),
        )?;
        session.stderr.flush()
    }

    async fn execute_status(
        &self,
        _os: &Os,
        session: &mut ChatSession,
    ) -> Result<ChatState, ChatError> {
        // Check if we have a code intelligence client
        if let Some(code_client) = &mut session.conversation.code_intelligence_client {
            // Use the SDK to detect workspace
            match code_client.detect_workspace() {
                Ok(workspace_info) => {
                    queue!(
                        session.stderr,
                        style::Print("📁 "),
                        style::SetForegroundColor(style::Color::Cyan),
                        style::Print("Workspace: "),
                        style::ResetColor,
                        style::Print(format!("{}\n", workspace_info.root_path.display())),
                    )?;

                    queue!(
                        session.stderr,
                        style::Print("🌐 "),
                        style::SetForegroundColor(style::Color::Green),
                        style::Print("Detected Languages: "),
                        style::ResetColor,
                        style::Print(format!("{:?}\n", workspace_info.detected_languages)),
                    )?;

                    queue!(
                        session.stderr,
                        style::Print("\n🔧 "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print("Available LSPs:\n"),
                        style::ResetColor,
                    )?;

                    for lsp in &workspace_info.available_lsps {
                        let status = if lsp.is_available { "✅" } else { "❌" };
                        queue!(
                            session.stderr,
                            style::Print(format!(
                                "  {} {} ({})\n",
                                status,
                                lsp.name,
                                lsp.languages.join(", ")
                            )),
                        )?;
                    }
                }
                Err(e) => {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(style::Color::Red),
                        style::Print("❌ Failed to detect workspace: "),
                        style::ResetColor,
                        style::Print(format!("{}\n", e)),
                    )?;
                }
            }
        } else {
            queue!(
                session.stderr,
                style::SetForegroundColor(style::Color::Yellow),
                style::Print("⚠️  Code intelligence client not initialized\n"),
                style::ResetColor,
                style::Print("   Use a code tool to initialize the client automatically\n"),
            )?;
        }

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn execute_detect(
        &self,
        _os: &Os,
        session: &mut ChatSession,
    ) -> Result<ChatState, ChatError> {
        if let Some(client) = &mut session.conversation.code_intelligence_client {
            queue!(
                session.stderr,
                style::SetForegroundColor(style::Color::Cyan),
                style::Print("🚀 Initializing workspace...\n"),
                style::ResetColor,
            )?;

            match client.initialize().await {
                Ok(_) => {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(style::Color::Green),
                        style::Print("✅ Workspace initialized\n\n"),
                        style::ResetColor,
                    )?;
                }
                Err(e) => {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(style::Color::Red),
                        style::Print("❌ Failed to initialize workspace: "),
                        style::ResetColor,
                        style::Print(format!("{}\n", e)),
                    )?;
                    session.stderr.flush()?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }
            }
        }

        self.execute_status(_os, session).await
    }
}
