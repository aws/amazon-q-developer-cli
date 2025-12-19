use std::io::Write;

use clap::{
    Args,
    Subcommand,
};
use crossterm::{
    execute,
    queue,
    style,
};
use dialoguer::Select;

use crate::cli::chat::tool_manager::LoadingRecord;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::constants::{
    CLI_NAME,
    DEFAULT_AGENT_NAME,
};
use crate::theme::StyledText;

/// Truncate server description to fit in terminal display
fn truncate_server_description(description: &str) -> String {
    if description.is_empty() {
        "(no description)".to_string()
    } else if description.len() > 50 {
        // Try to break at word boundary near the limit
        let truncated = &description[..47];
        if let Some(last_space) = truncated.rfind(' ') {
            if last_space > 30 {
                // Don't break too early
                format!("{}...", &description[..last_space])
            } else {
                format!("{truncated}...")
            }
        } else {
            format!("{truncated}...")
        }
    } else {
        description.to_string()
    }
}

/// Helper function to handle MCP disabled state with appropriate error messages
fn handle_mcp_disabled_state(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    // MCP is disabled by admin (toggle is Off)
    queue!(
        session.stderr,
        StyledText::warning_fg(),
        style::Print("\n⚠️  WARNING: "),
        StyledText::reset(),
        style::Print("MCP functionality has been disabled by your administrator.\n\n"),
    )?;
    session.stderr.flush()?;
    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

/// Helper function to check MCP prerequisites (enabled + registry configured)
/// Returns true if all checks pass, false if prerequisites are not met (and displays appropriate
/// error)
fn check_mcp_prerequisites(session: &mut ChatSession, command: &str) -> bool {
    // Check if MCP is enabled
    if !session.conversation.mcp_enabled {
        handle_mcp_disabled_state(session).ok();
        return false;
    }

    // Check if registry URL is configured
    if session.conversation.mcp_registry_url.is_none() {
        let cli_command = match command {
            "add" => format!("{CLI_NAME} mcp add --name <name> --command <command>"),
            "remove" => format!("{CLI_NAME} mcp remove --name <name>"),
            _ => format!("{CLI_NAME} mcp {command}"),
        };

        queue!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print("\n⚠️  WARNING: "),
            StyledText::reset(),
            style::Print("No MCP registry has been configured by your administrator.\n"),
            style::Print("To manage MCP servers manually, use: "),
            StyledText::brand_fg(),
            style::Print(&format!("{cli_command}\n\n")),
            StyledText::reset(),
        )
        .ok();

        session.stderr.flush().ok();
        return false;
    }

    true
}

/// Display registry error message based on error type
fn display_registry_error(
    session: &mut ChatSession,
    url: String,
    error_type: crate::mcp_registry::RegistryErrorType,
) -> Result<(), ChatError> {
    crate::mcp_registry::display_registry_error_to_writer(
        &mut session.stderr,
        &url,
        &error_type,
        "\n⚠️  ERROR: ",
        "MCP is disabled for this session",
    )
    .map_err(|e| ChatError::Custom(format!("Failed to display registry error: {e}").into()))?;
    Ok(())
}

/// Helper function to handle registry fetch failure
fn handle_registry_fetch_failure(session: &mut ChatSession) -> Result<(), ChatError> {
    let url = session
        .conversation
        .mcp_registry_url
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let error_type = session
        .conversation
        .mcp_registry_error_type
        .as_ref()
        .unwrap_or(&crate::mcp_registry::RegistryErrorType::NetworkConnectivity)
        .clone();
    display_registry_error(session, url, error_type)
}

/// Helper function to get currently enabled servers
fn get_enabled_servers(session: &ChatSession) -> std::collections::HashSet<String> {
    session
        .conversation
        .agents
        .get_active()
        .map(|agent| agent.mcp_servers.mcp_servers.keys().cloned().collect())
        .unwrap_or_default()
}

/// Helper function to handle changes and reload
async fn handle_changes_and_reload(os: &mut crate::os::Os, session: &mut ChatSession) -> Result<(), ChatError> {
    queue!(session.stderr, style::Print("\n"),)?;
    session.stderr.flush()?;
    reload_mcp_servers(os, session).await?;
    Ok(())
}

/// MCP (Model Context Protocol) subcommands
#[derive(Debug, PartialEq, Subcommand)]
pub enum McpSubcommand {
    /// List all MCP servers (shows registry servers if configured by admin, or local configured
    /// servers)
    List,
    /// Add an MCP server from the registry (only available if a registry has been configured by
    /// admin)
    Add,
    /// Remove an enabled MCP server (only available if a registry has been configured by admin)
    Remove,
}

/// Arguments for the MCP (Model Context Protocol) command.
///
/// This struct handles MCP-related functionality, allowing users to view
/// the status of MCP servers and their loading progress.
#[derive(Debug, PartialEq, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub subcommand: Option<McpSubcommand>,
}

impl McpArgs {
    pub async fn execute(self, os: &mut crate::os::Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self.subcommand {
            Some(McpSubcommand::List) => execute_list(session).await,
            Some(McpSubcommand::Add) => execute_add(os, session).await,
            Some(McpSubcommand::Remove) => execute_remove(os, session).await,
            None => execute_status(session).await,
        }
    }
}

async fn execute_status(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    if !session.conversation.mcp_enabled {
        return handle_mcp_disabled_state(session);
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

    // Check if there are actually any enabled/running servers
    let enabled_servers = get_enabled_servers(session);
    let has_servers = !enabled_servers.is_empty() || !still_loading.is_empty();

    if !has_servers {
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::warning_fg(),
            style::Print("⚠ "),
            StyledText::reset(),
            style::Print("No MCP servers installed\n\n"),
            style::Print("To learn how to configure MCP servers, visit "),
            StyledText::brand_fg(),
            style::Print(crate::constants::KIRO_MCP_DOCS_URL),
            StyledText::reset(),
            style::Print("\n"),
        )?;
    } else {
        // Show all configured servers (filtered by registry if in registry mode)
        if let Some(agent) = session.conversation.agents.get_active() {
            let mut configured_servers: std::collections::HashSet<String> = agent
                .mcp_servers
                .mcp_servers
                .iter()
                .filter(|(_, config)| !config.disabled)  // Filter out disabled servers
                .map(|(name, _)| name.clone())
                .collect();

            // Filter servers based on registry mode
            if session.conversation.mcp_enabled && session.conversation.mcp_registry_url.is_some() {
                // Registry mode: only show registry-type servers that exist in registry
                if let Some(registry_cache) = &session.conversation.mcp_registry_cache {
                    let registry = &registry_cache.data;
                    configured_servers.retain(|server_name| {
                        let agent_config = agent.mcp_servers.mcp_servers.get(server_name);
                        if let Some(config) = agent_config {
                            // In registry mode: only show registry-type servers that exist in registry
                            if config.is_registry_type() {
                                registry.get_server(server_name).is_some()
                            } else {
                                // Non-registry servers are filtered out in registry mode
                                false
                            }
                        } else {
                            false
                        }
                    });
                }
            } else {
                // Non-registry mode: filter out registry-type servers (they can't run without a registry)
                configured_servers.retain(|server_name| {
                    let agent_config = agent.mcp_servers.mcp_servers.get(server_name);
                    if let Some(config) = agent_config {
                        !config.is_registry_type()
                    } else {
                        false
                    }
                });
            }

            for server_name in configured_servers {
                if let Some(msg) = mcp_load_record.get(&server_name) {
                    // Server has load record - show it normally
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
                        style::Print(&server_name),
                        style::Print("\n"),
                        style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                        style::Print(msg),
                        style::Print("\n")
                    )?;
                } else {
                    // Server is configured but no load record yet - show as loading
                    queue!(
                        session.stderr,
                        style::Print(&server_name),
                        style::Print("\n"),
                        style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                        StyledText::warning_fg(),
                        style::Print("[loading]: "),
                        StyledText::reset(),
                        style::Print("Server is still initializing...\n\n"),
                    )?;
                }
            }
        }
    }

    session.stderr.flush()?;

    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

async fn execute_list(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    if !session.conversation.mcp_enabled {
        return handle_mcp_disabled_state(session);
    }

    // Check if we're in registry mode
    let is_registry_mode = session.conversation.mcp_registry_url.is_some();

    if is_registry_mode {
        // Registry mode: list all servers from registry with enabled status
        execute_list_registry(session).await
    } else {
        // Non-registry mode: list configured servers
        execute_list_configured(session).await
    }
}

async fn execute_list_registry(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    let terminal_width = session.terminal_width();
    queue!(
        session.stderr,
        style::Print("\n"),
        StyledText::brand_fg(),
        style::Print("📋 MCP Registry Servers\n"),
        StyledText::reset(),
        style::Print(format!("{}\n\n", "─".repeat(terminal_width))),
    )?;

    if let Some(registry_cache) = &session.conversation.mcp_registry_cache {
        let registry = &registry_cache.data;
        let active_agent = session.conversation.agents.get_active();
        let enabled_servers: std::collections::HashSet<String> = active_agent
            .map(|agent| agent.mcp_servers.mcp_servers.keys().cloned().collect())
            .unwrap_or_default();

        for server_entry in &registry.servers {
            let server = &server_entry.server;
            let is_enabled = enabled_servers.contains(&server.name);
            let status_icon = if is_enabled { "✓" } else { " " };

            if is_enabled {
                queue!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!("[{status_icon}] ")),
                    StyledText::brand_fg(),
                    style::Print(&server.name),
                    StyledText::reset(),
                    style::Print(format!(" (v{})\n", server.version)),
                    style::Print(format!("    {}\n", server.description)),
                )?;
            } else {
                queue!(
                    session.stderr,
                    style::Print(format!("[{status_icon}] ")),
                    StyledText::brand_fg(),
                    style::Print(&server.name),
                    StyledText::reset(),
                    style::Print(format!(" (v{})\n", server.version)),
                    style::Print(format!("    {}\n", server.description)),
                )?;
            }
        }
    } else {
        // Registry URL is configured but cache is None = registry fetch failed
        handle_registry_fetch_failure(session)?;
    }

    session.stderr.flush()?;
    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

async fn execute_list_configured(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    let terminal_width = session.terminal_width();
    queue!(
        session.stderr,
        style::Print("\n"),
        StyledText::brand_fg(),
        style::Print("📋 Configured MCP Servers\n"),
        StyledText::reset(),
        style::Print(format!("{}\n\n", "─".repeat(terminal_width))),
    )?;

    let active_agent = session.conversation.agents.get_active();
    if let Some(agent) = active_agent {
        if agent.mcp_servers.mcp_servers.is_empty() {
            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("⚠ "),
                StyledText::reset(),
                style::Print("No MCP servers configured.\n\n"),
                style::Print("To configure MCP servers, use "),
                StyledText::brand_fg(),
                style::Print(&format!("{CLI_NAME} mcp add")),
                StyledText::reset(),
                style::Print(" or visit "),
                StyledText::brand_fg(),
                style::Print(crate::constants::KIRO_MCP_DOCS_URL),
                StyledText::reset(),
                style::Print("\n\n"),
            )?;
        } else {
            for (name, config) in &agent.mcp_servers.mcp_servers {
                let status = if config.disabled { " (disabled)" } else { "" };
                queue!(
                    session.stderr,
                    StyledText::brand_fg(),
                    style::Print(name),
                    StyledText::reset(),
                    style::Print(format!("{status}\n")),
                    style::Print(format!("    Command: {}\n", config.command)),
                )?;
            }
            queue!(session.stderr, style::Print("\n"))?;
        }
    }

    session.stderr.flush()?;
    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

/// Helper function to reload MCP servers after changes
async fn reload_mcp_servers(os: &mut crate::os::Os, session: &mut ChatSession) -> Result<(), ChatError> {
    // Save to disk if agent has a path
    let active_agent = session
        .conversation
        .agents
        .get_active_mut()
        .ok_or_else(|| ChatError::Custom("No active agent".into()))?;

    let mut saved_to_disk = false;
    if let Some(config_path) = &active_agent.path {
        let json = active_agent
            .to_str_pretty()
            .map_err(|e| ChatError::Custom(format!("Failed to serialize agent: {e}").into()))?;
        if let Err(e) = os.fs.write(config_path, json).await {
            tracing::warn!("Failed to save agent config: {}", e);
            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print(format!("\n⚠️  Warning: Could not save to disk: {e}\n")),
                StyledText::reset(),
                style::Print("Changes are in memory only and will be lost on restart.\n"),
            )?;
        } else {
            saved_to_disk = true;
        }
    } else {
        // Check if this is the default agent
        let is_default_agent = active_agent.name == DEFAULT_AGENT_NAME;

        queue!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print("\n⚠️  Note: "),
            StyledText::reset(),
        )?;

        if is_default_agent {
            queue!(
                session.stderr,
                style::Print(&format!(
                    "Changes to {DEFAULT_AGENT_NAME} agent only apply to current session.\n"
                )),
            )?;
        } else {
            queue!(
                session.stderr,
                style::Print("Agent has no config file.\n"),
                style::Print("Changes are in memory only and will be lost on restart.\n"),
            )?;
        }
    }

    queue!(session.stderr, style::Print("\n🔄 Reloading MCP servers...\n"),)?;
    session.stderr.flush()?;

    // Get the current agent name to swap back to it
    let agent_name = session
        .conversation
        .agents
        .get_active()
        .ok_or_else(|| ChatError::Custom("No active agent".into()))?
        .name
        .clone();

    // Use conversation.swap_agent to properly reload everything
    if let Err(e) = session
        .conversation
        .swap_agent(os, &mut session.stderr, &agent_name)
        .await
    {
        queue!(
            session.stderr,
            StyledText::error_fg(),
            style::Print(format!("\n⚠️  Error reloading servers: {e}\n")),
            StyledText::reset(),
        )?;
        if saved_to_disk {
            queue!(
                session.stderr,
                style::Print("Changes were saved to disk. Restart the session to apply them.\n\n"),
            )?;
        } else {
            queue!(session.stderr, style::Print("Changes were not saved.\n\n"),)?;
        }
        session.stderr.flush()?;
        return Err(ChatError::Custom(format!("Failed to reload servers: {e}").into()));
    }

    // Get the expected count after reload
    let expected_count = session
        .conversation
        .agents
        .get_active()
        .map_or(0, |agent| agent.mcp_servers.mcp_servers.len());

    // Wait for MCP servers to actually load and send their tools (up to 2 seconds)
    let max_wait = std::time::Duration::from_secs(2);
    let start = std::time::Instant::now();

    while start.elapsed() < max_wait {
        // Check if we have any pending clients
        let pending = session.conversation.tool_manager.pending_clients().await;

        // Update state to sync any new tools
        session.conversation.update_state(true).await;

        // Check if the load record has entries for all expected servers
        let load_record = session.conversation.tool_manager.mcp_load_record.lock().await;
        let loaded_count = load_record.len();
        let all_servers_processed = loaded_count >= expected_count;
        drop(load_record);

        if pending.is_empty() && all_servers_processed {
            // All clients initialized and load records are complete
            break;
        }

        if pending.is_empty() && expected_count == 0 {
            // No servers configured, nothing to wait for
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Final update to ensure everything is synced
    session.conversation.update_state(true).await;

    // Check if tools are actually loaded
    let mcp_tool_count = session
        .conversation
        .tool_manager
        .schema
        .values()
        .filter(|spec| matches!(spec.tool_origin, crate::cli::chat::tools::ToolOrigin::McpServer(_)))
        .count();

    if mcp_tool_count > 0 {
        queue!(
            session.stderr,
            StyledText::success_fg(),
            style::Print(format!(
                "✓ Servers reloaded successfully ({mcp_tool_count} MCP tools loaded)\n"
            )),
            StyledText::reset(),
        )?;
    } else {
        queue!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print("⚠ Servers reloaded but no tools were loaded\n"),
            StyledText::reset(),
            style::Print("   Check "),
            StyledText::brand_fg(),
            style::Print("/mcp"),
            StyledText::reset(),
            style::Print(" for server status and errors\n\n"),
        )?;
    }
    session.stderr.flush()?;

    Ok(())
}

async fn execute_add(os: &mut crate::os::Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
    use crate::cli::chat::tools::custom_tool::CustomToolConfig;

    // Check MCP prerequisites (enabled + registry configured)
    if !check_mcp_prerequisites(session, "add") {
        return Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        });
    }

    // Ensure MCP data is fresh and handle server changes
    let registry = match session.ensure_fresh_mcp_data(os).await? {
        Some(registry) => registry,
        None => {
            return handle_mcp_disabled_state(session);
        },
    };

    let mut changes_made = false;

    // Loop to allow adding multiple servers
    loop {
        // Get currently enabled servers (refresh each iteration)
        let enabled_servers = get_enabled_servers(session);

        // Build the selection list - only show disabled servers
        let mut server_labels = Vec::new();
        let mut server_names = Vec::new();

        let _registry_url = session.conversation.mcp_registry_url.as_deref().unwrap_or("unknown");

        for server_entry in &registry.servers {
            let server = &server_entry.server;
            let is_enabled = enabled_servers.contains(&server.name);

            // Only include disabled servers for the "add" command
            if !is_enabled {
                server_names.push(server.name.clone());

                let truncated_description = truncate_server_description(&server.description);
                let label = format!(
                    "{:<25} {}",
                    server.name.as_str(),
                    style::Stylize::dark_grey(truncated_description)
                );
                server_labels.push(label);
            }
        }

        if server_labels.is_empty() {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::success_fg(),
                style::Print("✓ "),
                StyledText::reset(),
                style::Print("All registry servers are already enabled.\n\n"),
            )?;
            session.stderr.flush()?;
            break;
        }

        let interact_result = Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt(format!(
                "{}({}) {} · {}({}) {}",
                StyledText::secondary("Enter"),
                StyledText::current_item("⏎"),
                StyledText::secondary("to add"),
                StyledText::secondary("Esc"),
                StyledText::current_item("⎋"),
                StyledText::secondary("to save")
            ))
            .items(&server_labels)
            .default(0)
            .report(false)
            .interact_on_opt(&dialoguer::console::Term::stdout());

        let selection: Option<_> = match interact_result {
            Ok(sel) => sel,
            // EINTR (Interrupted system call) - retry the menu
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
                continue;
            },
            Err(e) => {
                return Err(ChatError::Custom(format!("Failed to choose server: {e}").into()));
            },
        };

        queue!(session.stderr, StyledText::reset())?;

        if let Some(index) = selection {
            if let Some(server_name) = server_names.get(index) {
                // Update the in-memory agent
                let active_agent = session
                    .conversation
                    .agents
                    .get_active_mut()
                    .ok_or_else(|| ChatError::Custom("No active agent".into()))?;

                {
                    // Verify server exists in registry
                    if !registry.servers.iter().any(|s| &s.server.name == server_name) {
                        return Err(ChatError::Custom("Your administrator has configured an MCP registry, and you can only install servers from that registry. Use 'mcp add' to install registry servers.".to_string().into()));
                    }

                    // Create a minimal registry reference config with type: "registry"
                    let server_config = CustomToolConfig::minimal_registry();

                    active_agent
                        .mcp_servers
                        .mcp_servers
                        .insert(server_name.clone(), server_config);
                }

                // Also add the server to the tools list (unless "*" is present)
                // If "*" is present, it means "allow all tools" and we shouldn't add specific entries
                if !active_agent.tools.contains(&"*".to_string()) {
                    let tool_name = format!("@{server_name}");
                    if !active_agent.tools.contains(&tool_name) {
                        active_agent.tools.push(tool_name);
                    }
                }

                changes_made = true;

                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print(format!("✓ {server_name} added\n")),
                    StyledText::reset(),
                )?;
                session.stderr.flush()?;
            }
        } else {
            // ESC was pressed - clear everything from cursor down
            queue!(
                session.stderr,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
            )?;
            session.stderr.flush()?;
            break;
        }
    } // end loop

    execute!(session.stderr, StyledText::reset())?;

    // Reload servers if any changes were made
    if changes_made {
        handle_changes_and_reload(os, session).await?;
    }

    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

async fn execute_remove(os: &mut crate::os::Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
    // Check MCP prerequisites (enabled + registry configured)
    if !check_mcp_prerequisites(session, "remove") {
        return Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        });
    }

    // Ensure MCP data is fresh and handle server changes
    let registry = match session.ensure_fresh_mcp_data(os).await? {
        Some(registry) => registry,
        None => {
            return handle_mcp_disabled_state(session);
        },
    };

    let mut changes_made = false;

    // Loop to allow removing multiple servers
    loop {
        // Get currently enabled servers (refresh each iteration)
        let enabled_servers = get_enabled_servers(session);

        // Build the selection list - only show enabled servers
        let mut server_labels = Vec::new();
        let mut server_names = Vec::new();

        for server_entry in &registry.servers {
            let server = &server_entry.server;
            let is_enabled = enabled_servers.contains(&server.name);

            // Only include enabled servers for the "remove" command
            if is_enabled {
                server_names.push(server.name.clone());

                let truncated_description = truncate_server_description(&server.description);
                let label = format!(
                    "{:<25} {}",
                    server.name.as_str(),
                    style::Stylize::dark_grey(truncated_description)
                );
                server_labels.push(label);
            }
        }

        if server_labels.is_empty() {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("⚠ "),
                StyledText::reset(),
                style::Print("No registry servers are currently enabled.\n\n"),
            )?;
            session.stderr.flush()?;
            break;
        }

        session.stderr.flush()?;
        session.stdout.flush()?;

        let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt(format!(
                "{}({}) {} · {}({}) {}",
                StyledText::secondary("Enter"),
                StyledText::current_item("⏎"),
                StyledText::secondary("to remove"),
                StyledText::secondary("Esc"),
                StyledText::current_item("⎋"),
                StyledText::secondary("to save")
            ))
            .items(&server_labels)
            .default(0)
            .report(false)
            .interact_on_opt(&dialoguer::console::Term::stdout())
        {
            Ok(sel) => sel,
            // EINTR (Interrupted system call) - retry the menu
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
                continue;
            },
            Err(e) => return Err(ChatError::Custom(format!("Failed to choose server: {e}").into())),
        };

        queue!(session.stderr, StyledText::reset())?;

        if let Some(index) = selection {
            if let Some(server_name) = server_names.get(index) {
                // Update the in-memory agent - remove the server
                let active_agent = session
                    .conversation
                    .agents
                    .get_active_mut()
                    .ok_or_else(|| ChatError::Custom("No active agent".into()))?;

                active_agent.mcp_servers.mcp_servers.remove(server_name);

                // Also remove the server from the tools whitelist (if not using "*")
                // If "*" is present, we never added specific entries, so nothing to remove
                if !active_agent.tools.contains(&"*".to_string()) {
                    let tool_name = format!("@{server_name}");
                    active_agent.tools.retain(|t| t != &tool_name);
                }

                changes_made = true;

                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print(format!("✓ {server_name} removed\n")),
                    StyledText::reset(),
                )?;
                session.stderr.flush()?;

                // Continue loop to show menu again
            }
        } else {
            // ESC was pressed - clear everything from cursor down
            queue!(
                session.stderr,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
            )?;
            session.stderr.flush()?;
            break;
        }
    } // end loop

    execute!(session.stderr, StyledText::reset())?;

    // Reload servers if any changes were made
    if changes_made {
        handle_changes_and_reload(os, session).await?;
    }

    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}
