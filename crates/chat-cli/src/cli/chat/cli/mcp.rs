use std::io::Write;

use clap::Args;
use crossterm::{
    queue,
    style,
};

use crate::cli::chat::tool_manager::LoadingRecord;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::constants::PRODUCT_NAME;
use crate::theme::StyledText;

/// Arguments for the MCP (Model Context Protocol) command.
///
/// This struct handles MCP-related functionality, allowing users to view
/// the status of MCP servers and their loading progress.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct McpArgs;

impl McpArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if !session.conversation.mcp_enabled {
            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                StyledText::reset(),
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

        let mcp_load_record = session.conversation.tool_manager.mcp_load_record.lock().await;
        let has_servers = !mcp_load_record.is_empty() || !still_loading.is_empty();

        if !has_servers {
            queue!(
                session.stderr,
                style::Print("No MCP servers enabled. Go to "),
                StyledText::brand_fg(),
                style::Print(crate::constants::KIRO_MCP_DOCS_URL),
                StyledText::reset(),
                style::Print(&format!(" to learn how to add servers to {PRODUCT_NAME}\n\n")),
            )?;
        } else {
            for (server_name, msg) in mcp_load_record.iter() {
                let msg = msg
                    .iter()
                    .map(|record| match record {
                        LoadingRecord::Err(timestamp, content)
                        | LoadingRecord::Warn(timestamp, content)
                        | LoadingRecord::Success(timestamp, content) => format!("[{timestamp}]: {content}"),
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
        }

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
