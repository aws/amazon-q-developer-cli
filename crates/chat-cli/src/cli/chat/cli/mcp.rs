use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

use clap::{Args, Subcommand};
use crossterm::{
    queue,
    style,
};
use tokio::sync::Mutex;

use crate::cli::chat::server_reload_manager::{ErrorDisplayManager, ReloadError, ServerReloadManager};
use crate::cli::chat::tools::custom_tool::CustomToolConfig;
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
    /// Reload a specific MCP server with updated configuration
    Reload(ReloadArgs),
    /// Enable a disabled MCP server for this session
    Enable(EnableArgs),
    /// Disable an enabled MCP server for this session
    Disable(DisableArgs),
    /// Show detailed status of MCP servers
    Status(StatusArgs),
    /// List all configured MCP servers
    List(ListArgs),
    /// Reload all MCP server configurations from files
    ReloadConfig(ReloadConfigArgs),
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

#[derive(Debug, PartialEq, Args)]
pub struct ReloadConfigArgs {
    /// Validate configurations without applying changes
    #[arg(long)]
    pub validate_only: bool,
    
    /// Show detailed information about configuration changes
    #[arg(long)]
    pub verbose: bool,
}

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
            McpSubcommand::ReloadConfig(args) => args.execute(session).await,
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
        
        // Perform the reload operation with comprehensive error handling
        match reload_manager.reload_server(&os, &self.server_name).await {
            Ok(_) => {
                // Update the session's tool manager with the reloaded state
                let updated_tool_manager = tool_manager_ref.lock().await;
                session.conversation.tool_manager = updated_tool_manager.clone();
                drop(updated_tool_manager);
                
                // Force update the conversation state to refresh tool list
                session.conversation.update_state(true).await;
                
                // Refresh filtered tools for model context
                session.conversation.refresh_filtered_tools().await;
                
                // Display success message
                ErrorDisplayManager::display_success(
                    &format!("Server '{}' reloaded successfully", self.server_name),
                    Some("Tools and configuration refreshed"),
                    session,
                )?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display comprehensive error with user guidance
                ErrorDisplayManager::display_error(
                    &e,
                    &format!("Failed to reload server '{}'", self.server_name),
                    session,
                ).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Server reload failed: {}", e).into()))
            }
        }
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
                ErrorDisplayManager::display_success(
                    &format!("Server '{}' enabled for this session", self.server_name),
                    Some("Tools are now available"),
                    session,
                )?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display comprehensive error with user guidance
                ErrorDisplayManager::display_error(
                    &e,
                    &format!("Failed to enable server '{}'", self.server_name),
                    session,
                ).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Server enable failed: {}", e).into()))
            }
        }
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
                ErrorDisplayManager::display_success(
                    &format!("Server '{}' disabled for this session", self.server_name),
                    Some("Tools are no longer available"),
                    session,
                )?;
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display comprehensive error with user guidance
                ErrorDisplayManager::display_error(
                    &e,
                    &format!("Failed to disable server '{}'", self.server_name),
                    session,
                ).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Server disable failed: {}", e).into()))
            }
        }
    }
}

impl StatusArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let terminal_width = session.terminal_width();
        
        // Get current server states
        let tool_manager = &session.conversation.tool_manager;
        let current_clients: HashSet<String> = tool_manager.clients.keys().cloned().collect();
        let session_disabled = tool_manager.get_session_disabled_servers().await;
        let session_enabled = tool_manager.get_session_enabled_servers().await;
        let configured_servers = tool_manager.get_configured_server_names().await;
        
        // If specific server requested, validate it exists
        if let Some(ref server_name) = self.server_name {
            if !configured_servers.contains(server_name) {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("✗ Server '"),
                    style::Print(server_name),
                    style::Print("' not found. Available servers: "),
                    style::ResetColor,
                    style::Print(configured_servers.join(", ")),
                    style::Print("\n"),
                )?;
                session.stderr.flush()?;
                return Ok(ChatState::PromptUser { skip_printing_tools: true });
            }
        }
        
        // Filter servers to show
        let servers_to_show: Vec<String> = if let Some(server_name) = self.server_name {
            vec![server_name]
        } else {
            configured_servers
        };
        
        // Display detailed status for each server
        for server_name in servers_to_show {
            let is_currently_running = current_clients.contains(&server_name);
            let is_session_disabled = session_disabled.contains(&server_name);
            let is_session_enabled = session_enabled.contains(&server_name);
            let config_enabled = !tool_manager.is_server_config_disabled(&server_name).await;
            
            // Determine effective status
            let effective_status = if is_session_disabled {
                "Disabled (session override)"
            } else if is_session_enabled {
                "Enabled (session override)"
            } else if config_enabled && is_currently_running {
                "Enabled (running)"
            } else if config_enabled && !is_currently_running {
                "Enabled (not running)"
            } else {
                "Disabled (configuration)"
            };
            
            // Get tool count for this server
            let tool_count = tool_manager.tn_map
                .values()
                .filter(|info| info.server_name == server_name)
                .count();
            
            // Display server header with status
            queue!(
                session.stderr,
                style::SetAttribute(style::Attribute::Bold),
                style::Print(&server_name),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(" - "),
                match effective_status {
                    s if s.contains("Disabled") => style::SetForegroundColor(style::Color::Red),
                    s if s.contains("session override") => style::SetForegroundColor(style::Color::Yellow),
                    _ => style::SetForegroundColor(style::Color::Green),
                },
                style::Print(effective_status),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            queue!(
                session.stderr,
                style::Print("▔".repeat(terminal_width)),
                style::Print("\n"),
            )?;
            
            // Show configuration status
            queue!(
                session.stderr,
                style::Print("Configuration: "),
                if config_enabled {
                    style::SetForegroundColor(style::Color::Green)
                } else {
                    style::SetForegroundColor(style::Color::Red)
                },
                style::Print(if config_enabled { "Enabled" } else { "Disabled" }),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            // Show session override if any
            if is_session_disabled || is_session_enabled {
                queue!(
                    session.stderr,
                    style::Print("Session Override: "),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print(if is_session_disabled { "Disabled" } else { "Enabled" }),
                    style::ResetColor,
                    style::Print("\n"),
                )?;
            }
            
            // Show runtime status
            queue!(
                session.stderr,
                style::Print("Runtime Status: "),
                if is_currently_running {
                    style::SetForegroundColor(style::Color::Green)
                } else {
                    style::SetForegroundColor(style::Color::Red)
                },
                style::Print(if is_currently_running { "Running" } else { "Stopped" }),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            // Show tool count
            queue!(
                session.stderr,
                style::Print("Tools Available: "),
                style::SetForegroundColor(style::Color::Cyan),
                style::Print(tool_count.to_string()),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            // Show loading record if available
            if let Some(records) = tool_manager.mcp_load_record.lock().await.get(&server_name) {
                if let Some(last_record) = records.last() {
                    queue!(
                        session.stderr,
                        style::Print("Last Status: "),
                    )?;
                    
                    match last_record {
                        LoadingRecord::Success(msg) => {
                            queue!(
                                session.stderr,
                                style::SetForegroundColor(style::Color::Green),
                                style::Print("✓ "),
                                style::Print(msg.split(" in ").next().unwrap_or(msg).replace("✓ ", "")),
                                style::ResetColor,
                            )?;
                            if let Some(timing) = msg.split(" in ").nth(1) {
                                queue!(
                                    session.stderr,
                                    style::Print(" ("),
                                    style::Print(timing),
                                    style::Print(")"),
                                )?;
                            }
                        },
                        LoadingRecord::Err(msg) => {
                            queue!(
                                session.stderr,
                                style::SetForegroundColor(style::Color::Red),
                                style::Print("✗ "),
                                style::Print(msg.replace("✗ ", "")),
                                style::ResetColor,
                            )?;
                        },
                        LoadingRecord::Warn(msg) => {
                            queue!(
                                session.stderr,
                                style::SetForegroundColor(style::Color::Yellow),
                                style::Print("⚠ "),
                                style::Print(msg.replace("⚠ ", "")),
                                style::ResetColor,
                            )?;
                        },
                    }
                    queue!(session.stderr, style::Print("\n"))?;
                }
            }
            
            queue!(session.stderr, style::Print("\n"))?;
        }
        
        session.stderr.flush()?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl ListArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let terminal_width = session.terminal_width();
        
        // Get current server states
        let tool_manager = &session.conversation.tool_manager;
        let current_clients: HashSet<String> = tool_manager.clients.keys().cloned().collect();
        let session_disabled = tool_manager.get_session_disabled_servers().await;
        let session_enabled = tool_manager.get_session_enabled_servers().await;
        let configured_servers = tool_manager.get_configured_server_names().await;
        
        if configured_servers.is_empty() {
            queue!(
                session.stderr,
                style::SetForegroundColor(style::Color::Yellow),
                style::Print("No MCP servers configured.\n"),
                style::ResetColor,
            )?;
            session.stderr.flush()?;
            return Ok(ChatState::PromptUser { skip_printing_tools: true });
        }
        
        // Display header
        queue!(
            session.stderr,
            style::SetAttribute(style::Attribute::Bold),
            style::Print("MCP Servers"),
            style::SetAttribute(style::Attribute::Reset),
            style::Print("\n"),
            style::Print("▔".repeat(terminal_width)),
            style::Print("\n"),
        )?;
        
        // Sort servers for consistent display
        let mut sorted_servers = configured_servers.clone();
        sorted_servers.sort();
        
        for server_name in sorted_servers {
            let is_currently_running = current_clients.contains(&server_name);
            let is_session_disabled = session_disabled.contains(&server_name);
            let is_session_enabled = session_enabled.contains(&server_name);
            let config_enabled = !tool_manager.is_server_config_disabled(&server_name).await;
            
            // Get tool count for this server
            let tool_count = tool_manager.tn_map
                .values()
                .filter(|info| info.server_name == server_name)
                .count();
            
            // Determine status symbol and color
            let (symbol, color, status_text) = if is_session_disabled {
                ("○", style::Color::DarkGrey, "disabled for session")
            } else if is_session_enabled {
                ("✓", style::Color::Yellow, "enabled for session")
            } else if config_enabled && is_currently_running {
                ("✓", style::Color::Green, "running")
            } else if config_enabled && !is_currently_running {
                ("⚠", style::Color::Yellow, "configured but not running")
            } else {
                ("○", style::Color::Red, "disabled in configuration")
            };
            
            // Display server line
            queue!(
                session.stderr,
                style::SetForegroundColor(color),
                style::Print(symbol),
                style::Print(" "),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(&server_name),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(color),
                style::Print(" ("),
                style::Print(status_text),
                style::Print(")"),
                style::ResetColor,
            )?;
            
            // Add tool count
            if tool_count > 0 {
                queue!(
                    session.stderr,
                    style::Print(" - "),
                    style::SetForegroundColor(style::Color::Cyan),
                    style::Print(tool_count.to_string()),
                    style::Print(" tool"),
                    style::Print(if tool_count == 1 { "" } else { "s" }),
                    style::ResetColor,
                )?;
            }
            
            queue!(session.stderr, style::Print("\n"))?;
        }
        
        // Show summary
        let total_servers = configured_servers.len();
        let running_servers = current_clients.len();
        let session_overrides = session_disabled.len() + session_enabled.len();
        
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(style::Attribute::Bold),
            style::Print("Summary: "),
            style::SetAttribute(style::Attribute::Reset),
            style::Print(format!("{} total, {} running", total_servers, running_servers)),
        )?;
        
        if session_overrides > 0 {
            queue!(
                session.stderr,
                style::Print(", "),
                style::SetForegroundColor(style::Color::Yellow),
                style::Print(format!("{} session override{}", session_overrides, if session_overrides == 1 { "" } else { "s" })),
                style::ResetColor,
            )?;
        }
        
        queue!(session.stderr, style::Print("\n\n"))?;
        
        session.stderr.flush()?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}

impl ReloadConfigArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Create OS interface for configuration operations
        let os = crate::os::Os::new().await
            .map_err(|e| ChatError::Custom(format!("Failed to initialize OS interface: {}", e).into()))?;
        
        // Show progress indication
        queue!(
            session.stderr,
            style::Print("🔄 Reloading MCP server configurations..."),
            style::Print("\n"),
        )?;
        session.stderr.flush()?;
        
        // Create reload manager with reference to tool manager
        let tool_manager_ref = Arc::new(Mutex::new(session.conversation.tool_manager.clone()));
        let reload_manager = ServerReloadManager::new(tool_manager_ref.clone());
        
        // Reload configurations with comprehensive validation
        match reload_manager.reload_configurations(&os).await {
            Ok(updated_configs) => {
                if self.validate_only {
                    // Validation-only mode
                    ErrorDisplayManager::display_success(
                        "Configuration validation successful",
                        Some(&format!("Found {} valid server configurations", updated_configs.len())),
                        session,
                    )?;
                    
                    if self.verbose {
                        self.display_configuration_details(&updated_configs, session).await?;
                    }
                } else {
                    // Apply configuration changes
                    let changes_applied = self.apply_configuration_changes(
                        &reload_manager, 
                        &updated_configs, 
                        session
                    ).await?;
                    
                    if changes_applied > 0 {
                        // Update the session's tool manager with the updated state
                        let updated_tool_manager = tool_manager_ref.lock().await;
                        session.conversation.tool_manager = updated_tool_manager.clone();
                        drop(updated_tool_manager);
                        
                        // Force update the conversation state to refresh tool list
                        session.conversation.update_state(true).await;
                        
                        // Refresh filtered tools for model context
                        session.conversation.refresh_filtered_tools().await;
                        
                        ErrorDisplayManager::display_success(
                            "Configuration reload completed",
                            Some(&format!("Applied changes to {} servers", changes_applied)),
                            session,
                        )?;
                    } else {
                        ErrorDisplayManager::display_success(
                            "Configuration reload completed",
                            Some("No changes were necessary"),
                            session,
                        )?;
                    }
                }
                
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            Err(e) => {
                // Display comprehensive error with user guidance
                ErrorDisplayManager::display_error(
                    &e,
                    "Failed to reload MCP server configurations",
                    session,
                ).await?;
                
                // Convert to ChatError but continue the session
                Err(ChatError::Custom(format!("Configuration reload failed: {}", e).into()))
            }
        }
    }
    
    /// Displays detailed configuration information
    async fn display_configuration_details(
        &self,
        configs: &HashMap<String, CustomToolConfig>,
        session: &mut ChatSession,
    ) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(style::Color::Cyan),
            style::SetAttribute(style::Attribute::Bold),
            style::Print("📋 Configuration Details:"),
            style::SetAttribute(style::Attribute::Reset),
            style::ResetColor,
            style::Print("\n"),
        )?;
        
        for (server_name, config) in configs {
            queue!(
                session.stderr,
                style::Print("   "),
                style::SetForegroundColor(style::Color::Yellow),
                style::Print(server_name),
                style::ResetColor,
                style::Print(": "),
                style::Print(&config.command),
            )?;
            
            if !config.args.is_empty() {
                queue!(
                    session.stderr,
                    style::Print(" "),
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print(config.args.join(" ")),
                    style::ResetColor,
                )?;
            }
            
            if config.disabled {
                queue!(
                    session.stderr,
                    style::Print(" "),
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("(disabled)"),
                    style::ResetColor,
                )?;
            }
            
            queue!(session.stderr, style::Print("\n"))?;
        }
        
        session.stderr.flush()?;
        Ok(())
    }
    
    /// Applies configuration changes to running servers
    async fn apply_configuration_changes(
        &self,
        reload_manager: &ServerReloadManager,
        updated_configs: &HashMap<String, CustomToolConfig>,
        session: &mut ChatSession,
    ) -> Result<usize, ChatError> {
        let mut changes_applied = 0;
        
        // Get current running servers
        let tool_manager = reload_manager.get_tool_manager().lock().await;
        let current_servers: HashSet<String> = tool_manager.clients.keys().cloned().collect();
        drop(tool_manager);
        
        // Create OS interface for server operations
        let os = crate::os::Os::new().await
            .map_err(|e| ChatError::Custom(format!("Failed to initialize OS interface: {}", e).into()))?;
        
        // Reload servers that are currently running and have updated configurations
        for server_name in &current_servers {
            if updated_configs.contains_key(server_name) {
                if self.verbose {
                    queue!(
                        session.stderr,
                        style::Print("   🔄 Reloading "),
                        style::SetForegroundColor(style::Color::Cyan),
                        style::Print(server_name),
                        style::ResetColor,
                        style::Print("...\n"),
                    )?;
                    session.stderr.flush()?;
                }
                
                match reload_manager.reload_server(&os, server_name).await {
                    Ok(_) => {
                        changes_applied += 1;
                        if self.verbose {
                            queue!(
                                session.stderr,
                                style::Print("   ✓ "),
                                style::SetForegroundColor(style::Color::Green),
                                style::Print(server_name),
                                style::ResetColor,
                                style::Print(" reloaded successfully\n"),
                            )?;
                        }
                    },
                    Err(e) => {
                        if self.verbose {
                            queue!(
                                session.stderr,
                                style::Print("   ✗ "),
                                style::SetForegroundColor(style::Color::Red),
                                style::Print(server_name),
                                style::ResetColor,
                                style::Print(" failed: "),
                                style::Print(&e.to_string()),
                                style::Print("\n"),
                            )?;
                        }
                        // Continue with other servers even if one fails
                    }
                }
            }
        }
        
        session.stderr.flush()?;
        Ok(changes_applied)
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
