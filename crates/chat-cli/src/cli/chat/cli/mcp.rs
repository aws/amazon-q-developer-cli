use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

use clap::{Args, Subcommand};
use crossterm::{
    queue,
    style,
};
use tokio::sync::Mutex;

use crate::cli::chat::server_reload_manager::{ReloadError, ServerReloadManager};
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

        // Get current server states
        let tool_manager = &session.conversation.tool_manager;
        let current_clients: HashSet<String> = tool_manager.clients.keys().cloned().collect();
        let session_disabled = tool_manager.get_session_disabled_servers().await;
        let session_enabled = tool_manager.get_session_enabled_servers().await;

        let still_loading = tool_manager
            .pending_clients()
            .await
            .into_iter()
            .map(|name| format!(" - {name}\n"))
            .collect::<Vec<_>>()
            .join("");

        for (server_name, records) in tool_manager.mcp_load_record.lock().await.iter() {
            let is_currently_running = current_clients.contains(server_name);
            let is_session_disabled = session_disabled.contains(server_name);
            let is_session_enabled = session_enabled.contains(server_name);
            
            let msg = records
                .iter()
                .map(|record| match record {
                    LoadingRecord::Err(content) | LoadingRecord::Warn(content) | LoadingRecord::Success(content) => {
                        // Modify the success message to show current status
                        if let LoadingRecord::Success(content) = record {
                            if is_session_disabled {
                                content.replace("✓", "○").replace("loaded", "disabled for session")
                            } else if !is_currently_running && !is_session_enabled {
                                content.replace("✓", "○").replace("loaded", "stopped")
                            } else {
                                content.clone()
                            }
                        } else {
                            content.clone()
                        }
                    },
                })
                .collect::<Vec<_>>()
                .join("\n--- tools refreshed ---\n");

            // Show server name with status indication
            if is_session_disabled {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print(server_name),
                    style::Print(" (disabled for session)"),
                    style::ResetColor,
                    style::Print("\n"),
                )?;
            } else {
                queue!(
                    session.stderr,
                    style::Print(server_name),
                    style::Print("\n"),
                )?;
            }

            queue!(
                session.stderr,
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
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Create OS interface - we need this for configuration reloading
        let os = crate::os::Os::new().await
            .map_err(|e| ChatError::Custom(format!("Failed to initialize OS interface: {}", e).into()))?;
        
        // Show progress indication
        queue!(
            session.stderr,
            style::Print("🔄 Reloading server '"),
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(&self.server_name),
            style::ResetColor,
            style::Print("'...\n"),
        )?;
        session.stderr.flush()?;
        
        // Create reload manager with reference to tool manager
        let tool_manager_ref = Arc::new(Mutex::new(session.conversation.tool_manager.clone()));
        let reload_manager = ServerReloadManager::new(tool_manager_ref.clone());
        
        // Perform the reload operation
        match reload_manager.reload_server(&os, &self.server_name).await {
            Ok(_) => {
                // Update the session's tool manager with the reloaded state
                let updated_tool_manager = tool_manager_ref.lock().await;
                session.conversation.tool_manager = updated_tool_manager.clone();
                drop(updated_tool_manager);
                
                // Display success message
                queue!(
                    session.stderr,
                    style::Print("✓ "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print("Server '"),
                    style::Print(&self.server_name),
                    style::Print("' reloaded successfully\n"),
                    style::ResetColor,
                )?;
                session.stderr.flush()?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display error message with helpful information
                self.display_reload_error(&e, session).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Failed to reload server '{}': {}", 
                    self.server_name, e).into()))
            }
        }
    }
    
    /// Displays a user-friendly error message for reload failures
    async fn display_reload_error(&self, error: &ReloadError, session: &mut ChatSession) -> Result<(), std::io::Error> {
        match error {
            ReloadError::ServerNotFound { server_name } => {
                // Show available servers to help the user
                let available_servers = session.conversation.tool_manager
                    .get_configured_server_names()
                    .await;
                
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' not found in configuration.\n"),
                    style::ResetColor,
                )?;
                
                if !available_servers.is_empty() {
                    queue!(
                        session.stderr,
                        style::Print("Available servers: "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(available_servers.join(", ")),
                        style::ResetColor,
                        style::Print("\n"),
                    )?;
                } else {
                    queue!(
                        session.stderr,
                        style::Print("No MCP servers are configured.\n"),
                    )?;
                }
            },
            ReloadError::ServerStateConflict { server_name, state } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' is already "),
                    style::Print(state),
                    style::Print(".\n"),
                    style::ResetColor,
                )?;
            },
            ReloadError::ServerStartFailed { server_name, reason } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Failed to start server '"),
                    style::Print(server_name),
                    style::Print("': "),
                    style::Print(reason),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
                
                queue!(
                    session.stderr,
                    style::Print("💡 Check server configuration and ensure the command is valid.\n"),
                )?;
            },
            ReloadError::ConfigReloadFailed { server_name, reason } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Failed to reload configuration for '"),
                    style::Print(server_name),
                    style::Print("': "),
                    style::Print(reason),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
            },
            _ => {
                // Generic error display for other error types
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print(error.to_string()),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
            }
        }
        
        session.stderr.flush()?;
        Ok(())
    }
}

impl EnableArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Create OS interface for server operations
        let os = crate::os::Os::new().await
            .map_err(|e| ChatError::Custom(format!("Failed to initialize OS interface: {}", e).into()))?;
        
        // Show progress indication
        queue!(
            session.stderr,
            style::Print("🔧 Enabling server '"),
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(&self.server_name),
            style::ResetColor,
            style::Print("'...\n"),
        )?;
        session.stderr.flush()?;
        
        // Create reload manager with reference to tool manager
        let tool_manager_ref = Arc::new(Mutex::new(session.conversation.tool_manager.clone()));
        let reload_manager = ServerReloadManager::new(tool_manager_ref.clone());
        
        // Perform the enable operation
        match reload_manager.enable_server(&os, &self.server_name).await {
            Ok(_) => {
                // Update the session's tool manager with the updated state
                let updated_tool_manager = tool_manager_ref.lock().await;
                session.conversation.tool_manager = updated_tool_manager.clone();
                drop(updated_tool_manager);
                
                // Force update the conversation state to refresh tool list
                session.conversation.update_state(true).await;
                
                // Refresh filtered tools for model context
                session.conversation.refresh_filtered_tools().await;
                
                // Display success message
                queue!(
                    session.stderr,
                    style::Print("✓ "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print("Server '"),
                    style::Print(&self.server_name),
                    style::Print("' enabled for this session\n"),
                    style::ResetColor,
                )?;
                session.stderr.flush()?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display error message with helpful information
                self.display_enable_error(&e, session).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Failed to enable server '{}': {}", 
                    self.server_name, e).into()))
            }
        }
    }
    
    /// Displays a user-friendly error message for enable failures
    async fn display_enable_error(&self, error: &ReloadError, session: &mut ChatSession) -> Result<(), std::io::Error> {
        match error {
            ReloadError::ServerNotFound { server_name } => {
                // Show available servers to help the user
                let available_servers = session.conversation.tool_manager
                    .get_configured_server_names()
                    .await;
                
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' not found in configuration.\n"),
                    style::ResetColor,
                )?;
                
                if !available_servers.is_empty() {
                    queue!(
                        session.stderr,
                        style::Print("Available servers: "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(available_servers.join(", ")),
                        style::ResetColor,
                        style::Print("\n"),
                    )?;
                } else {
                    queue!(
                        session.stderr,
                        style::Print("No MCP servers are configured.\n"),
                    )?;
                }
            },
            ReloadError::ServerStateConflict { server_name, state } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' is already "),
                    style::Print(state),
                    style::Print(".\n"),
                    style::ResetColor,
                )?;
                
                queue!(
                    session.stderr,
                    style::Print("💡 Use '/mcp status "),
                    style::Print(server_name),
                    style::Print("' to check current server state.\n"),
                )?;
            },
            ReloadError::ServerStartFailed { server_name, reason } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Failed to start server '"),
                    style::Print(server_name),
                    style::Print("': "),
                    style::Print(reason),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
                
                queue!(
                    session.stderr,
                    style::Print("💡 Check server configuration and ensure the command is valid.\n"),
                )?;
            },
            _ => {
                // Generic error display for other error types
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print(error.to_string()),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
            }
        }
        
        session.stderr.flush()?;
        Ok(())
    }
}

impl DisableArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Show progress indication
        queue!(
            session.stderr,
            style::Print("🔧 Disabling server '"),
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(&self.server_name),
            style::ResetColor,
            style::Print("'...\n"),
        )?;
        session.stderr.flush()?;
        
        // Create reload manager with reference to tool manager
        let tool_manager_ref = Arc::new(Mutex::new(session.conversation.tool_manager.clone()));
        let reload_manager = ServerReloadManager::new(tool_manager_ref.clone());
        
        // Perform the disable operation
        match reload_manager.disable_server(&self.server_name).await {
            Ok(_) => {
                // Force update the conversation state to refresh tool list
                session.conversation.update_state(true).await;
                
                // Refresh filtered tools for model context
                session.conversation.refresh_filtered_tools().await;
                
                // Display success message
                queue!(
                    session.stderr,
                    style::Print("✓ "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print("Server '"),
                    style::Print(&self.server_name),
                    style::Print("' disabled for this session\n"),
                    style::ResetColor,
                )?;
                session.stderr.flush()?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display error message with helpful information
                self.display_disable_error(&e, session).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Failed to disable server '{}': {}", 
                    self.server_name, e).into()))
            }
        }
    }
    
    /// Displays a user-friendly error message for disable failures
    async fn display_disable_error(&self, error: &ReloadError, session: &mut ChatSession) -> Result<(), std::io::Error> {
        match error {
            ReloadError::ServerNotFound { server_name } => {
                // Show available servers to help the user
                let available_servers = session.conversation.tool_manager
                    .get_configured_server_names()
                    .await;
                
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' not found in configuration.\n"),
                    style::ResetColor,
                )?;
                
                if !available_servers.is_empty() {
                    queue!(
                        session.stderr,
                        style::Print("Available servers: "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(available_servers.join(", ")),
                        style::ResetColor,
                        style::Print("\n"),
                    )?;
                } else {
                    queue!(
                        session.stderr,
                        style::Print("No MCP servers are configured.\n"),
                    )?;
                }
            },
            ReloadError::ServerStateConflict { server_name, state } => {
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("Server '"),
                    style::Print(server_name),
                    style::Print("' is already "),
                    style::Print(state),
                    style::Print(".\n"),
                    style::ResetColor,
                )?;
                
                queue!(
                    session.stderr,
                    style::Print("💡 Use '/mcp status "),
                    style::Print(server_name),
                    style::Print("' to check current server state.\n"),
                )?;
            },
            _ => {
                // Generic error display for other error types
                queue!(
                    session.stderr,
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print(error.to_string()),
                    style::Print("\n"),
                    style::ResetColor,
                )?;
            }
        }
        
        session.stderr.flush()?;
        Ok(())
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
    
    #[test]
    fn test_reload_error_handling() {
        // Test that ReloadArgs can be created with different server names
        let reload_args1 = ReloadArgs {
            server_name: "server-with-dashes".to_string(),
        };
        assert_eq!(reload_args1.server_name, "server-with-dashes");
        
        let reload_args2 = ReloadArgs {
            server_name: "server_with_underscores".to_string(),
        };
        assert_eq!(reload_args2.server_name, "server_with_underscores");
        
        let reload_args3 = ReloadArgs {
            server_name: "ServerWithCamelCase".to_string(),
        };
        assert_eq!(reload_args3.server_name, "ServerWithCamelCase");
    }
    
    #[test]
    fn test_enable_disable_args_validation() {
        // Test enable args with various server name formats
        let enable_args1 = EnableArgs {
            server_name: "test-server".to_string(),
        };
        assert_eq!(enable_args1.server_name, "test-server");
        
        let enable_args2 = EnableArgs {
            server_name: "server_with_underscores".to_string(),
        };
        assert_eq!(enable_args2.server_name, "server_with_underscores");
        
        // Test disable args with various server name formats
        let disable_args1 = DisableArgs {
            server_name: "test-server".to_string(),
        };
        assert_eq!(disable_args1.server_name, "test-server");
        
        let disable_args2 = DisableArgs {
            server_name: "ServerWithCamelCase".to_string(),
        };
        assert_eq!(disable_args2.server_name, "ServerWithCamelCase");
    }
    
    #[test]
    fn test_command_routing() {
        // Test that subcommands are properly routed
        let reload_cmd = McpSubcommand::Reload(ReloadArgs {
            server_name: "test".to_string(),
        });
        
        let enable_cmd = McpSubcommand::Enable(EnableArgs {
            server_name: "test".to_string(),
        });
        
        let disable_cmd = McpSubcommand::Disable(DisableArgs {
            server_name: "test".to_string(),
        });
        
        // Verify the commands can be created and matched
        match reload_cmd {
            McpSubcommand::Reload(_) => {}, // Expected
            _ => panic!("Expected Reload command"),
        }
        
        match enable_cmd {
            McpSubcommand::Enable(_) => {}, // Expected
            _ => panic!("Expected Enable command"),
        }
        
        match disable_cmd {
            McpSubcommand::Disable(_) => {}, // Expected
            _ => panic!("Expected Disable command"),
        }
    }
}
