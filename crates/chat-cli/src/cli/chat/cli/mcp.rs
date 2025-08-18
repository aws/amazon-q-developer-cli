use std::io::Write;

use clap::Args;
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::tool_manager::LoadingRecord;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct McpArgs;

impl McpArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let mcp_enabled = match os.client.is_mcp_enabled().await {
            Ok(enabled) => enabled,
            Err(err) => {
                tracing::warn!(?err, "Failed to check MCP configuration, defaulting to enabled");
                true
            },
        };

        if !mcp_enabled {
            queue!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                style::SetForegroundColor(Color::Reset),
                style::Print("MCP functionality has been disabled by your administrator.\n\n"),
            )?;
            session.stderr.flush()?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let terminal_width = session.terminal_width();
        let still_loading = session
            .conversation
            .tool_manager
            .pending_clients()
            .await
            .into_iter()
            .map(|name| format!(" - {name}\n"))
            .collect::<Vec<_>>()
            .join("");

        for (server_name, msg) in session.conversation.tool_manager.mcp_load_record.lock().await.iter() {
            let msg = msg
                .iter()
                .map(|record| match record {
                    LoadingRecord::Err(content) | LoadingRecord::Warn(content) | LoadingRecord::Success(content) => {
                        content.clone()
                    },
                })
                .collect::<Vec<_>>()
                .join("\n--- tools refreshed ---\n");

            queue!(
                session.stderr,
                style::Print(server_name),
                style::Print("\n"),
                style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                style::Print(msg),
                style::Print("\n")
            )?;
        }

        if !still_loading.is_empty() {
            queue!(
                session.stderr,
                style::Print("Still loading:\n"),
                style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                style::Print(still_loading),
                style::Print("\n")
            )?;
        }

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
