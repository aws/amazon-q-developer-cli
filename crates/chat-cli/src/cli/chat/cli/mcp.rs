use std::io::Write;

use clap::{Args, Subcommand};
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

#[derive(Debug, PartialEq, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub subcommand: Option<McpSubcommand>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum McpSubcommand {
    /// Reload a specific MCP server
    Reload(ReloadArgs),
    /// Enable a disabled MCP server for this session
    Enable(EnableArgs),
    /// Disable an enabled MCP server for this session
    Disable(DisableArgs),
    /// Show detailed status of MCP servers
    Status(StatusArgs),
    /// List all configured MCP servers
    List(ListArgs),
}

#[derive(Debug, PartialEq, Args)]
pub struct ReloadArgs {
    /// Name of the server to reload
    pub server_name: String,
}

#[derive(Debug, PartialEq, Args)]
pub struct EnableArgs {
    /// Name of the server to enable
    pub server_name: String,
}

#[derive(Debug, PartialEq, Args)]
pub struct DisableArgs {
    /// Name of the server to disable
    pub server_name: String,
}

#[derive(Debug, PartialEq, Args)]
pub struct StatusArgs {
    /// Name of the server to show status for (optional)
    pub server_name: Option<String>,
}

#[derive(Debug, PartialEq, Args)]
pub struct ListArgs;

impl McpArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self.subcommand {
            Some(subcommand) => subcommand.execute(session).await,
            None => self.execute_default_behavior(session).await,
        }
    }

    /// Default behavior when no subcommand is provided - maintains backward compatibility
    async fn execute_default_behavior(&self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
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

impl McpSubcommand {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            McpSubcommand::Reload(args) => args.execute(session).await,
            McpSubcommand::Enable(args) => args.execute(session).await,
            McpSubcommand::Disable(args) => args.execute(session).await,
            McpSubcommand::Status(args) => args.execute(session).await,
            McpSubcommand::List(args) => args.execute(session).await,
        }
    }
}

// Placeholder implementations for each subcommand
impl ReloadArgs {
    pub async fn execute(self, _session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // TODO: Implement reload functionality
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl EnableArgs {
    pub async fn execute(self, _session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // TODO: Implement enable functionality
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl DisableArgs {
    pub async fn execute(self, _session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // TODO: Implement disable functionality
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl StatusArgs {
    pub async fn execute(self, _session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // TODO: Implement status functionality
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl ListArgs {
    pub async fn execute(self, _session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // TODO: Implement list functionality
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_args_structure() {
        // Test that we can create McpArgs with no subcommand
        let args = McpArgs { subcommand: None };
        assert!(args.subcommand.is_none());
    }

    #[test]
    fn test_reload_args_creation() {
        let reload_args = ReloadArgs {
            server_name: "test-server".to_string(),
        };
        assert_eq!(reload_args.server_name, "test-server");
    }

    #[test]
    fn test_enable_args_creation() {
        let enable_args = EnableArgs {
            server_name: "my-server".to_string(),
        };
        assert_eq!(enable_args.server_name, "my-server");
    }

    #[test]
    fn test_disable_args_creation() {
        let disable_args = DisableArgs {
            server_name: "old-server".to_string(),
        };
        assert_eq!(disable_args.server_name, "old-server");
    }

    #[test]
    fn test_status_args_creation() {
        let status_args_with_server = StatusArgs {
            server_name: Some("target-server".to_string()),
        };
        assert_eq!(status_args_with_server.server_name, Some("target-server".to_string()));

        let status_args_without_server = StatusArgs {
            server_name: None,
        };
        assert_eq!(status_args_without_server.server_name, None);
    }

    #[test]
    fn test_list_args_creation() {
        let _list_args = ListArgs;
        // ListArgs has no fields, just verify it can be created
    }

    #[test]
    fn test_subcommand_variants() {
        let reload_subcommand = McpSubcommand::Reload(ReloadArgs {
            server_name: "test".to_string(),
        });
        match reload_subcommand {
            McpSubcommand::Reload(args) => assert_eq!(args.server_name, "test"),
            _ => panic!("Expected Reload variant"),
        }

        let enable_subcommand = McpSubcommand::Enable(EnableArgs {
            server_name: "test".to_string(),
        });
        match enable_subcommand {
            McpSubcommand::Enable(args) => assert_eq!(args.server_name, "test"),
            _ => panic!("Expected Enable variant"),
        }

        let disable_subcommand = McpSubcommand::Disable(DisableArgs {
            server_name: "test".to_string(),
        });
        match disable_subcommand {
            McpSubcommand::Disable(args) => assert_eq!(args.server_name, "test"),
            _ => panic!("Expected Disable variant"),
        }

        let status_subcommand = McpSubcommand::Status(StatusArgs {
            server_name: Some("test".to_string()),
        });
        match status_subcommand {
            McpSubcommand::Status(args) => assert_eq!(args.server_name, Some("test".to_string())),
            _ => panic!("Expected Status variant"),
        }

        let list_subcommand = McpSubcommand::List(ListArgs);
        match list_subcommand {
            McpSubcommand::List(_) => {}, // Success
            _ => panic!("Expected List variant"),
        }
    }
}
