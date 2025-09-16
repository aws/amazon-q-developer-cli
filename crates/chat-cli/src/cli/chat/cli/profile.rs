use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

use clap::Subcommand;
use crossterm::style::{
    self,
    Attribute,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use dialoguer::{
    MultiSelect,
    Select,
};
use eyre::Result;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Style,
    ThemeSet,
};
use syntect::parsing::SyntaxSet;
use syntect::util::{
    LinesWithEndings,
    as_24_bit_terminal_escaped,
};

use crate::cli::agent::{
    Agent,
    Agents,
    McpServerConfig,
    create_agent,
};
use crate::cli::chat::conversation::McpServerInfo;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories::chat_global_agent_path;
use crate::util::{
    NullWriter,
    directories,
};

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    before_long_help = "Agents allow you to organize and manage different sets of context files for different projects or tasks.

Notes
• Launch q chat with a specific agent with --agent
• Construct an agent under ~/.aws/amazonq/cli-agents/ (accessible globally) or cwd/.amazonq/cli-agents (accessible in workspace)
• See example config under global directory
• Set default agent to assume with settings by running \"q settings chat.defaultAgent agent_name\"
• Each agent maintains its own set of context and customizations"
)]
/// Subcommands for managing agents in the chat CLI
pub enum AgentSubcommand {
    /// List all available agents
    List,
    /// Create a new agent with the specified name
    Create {
        /// Name of the agent to be created
        #[arg(long, short)]
        name: String,
        /// The directory where the agent will be saved. If not provided, the agent will be saved in
        /// the global agent directory
        #[arg(long, short)]
        directory: Option<String>,
        /// The name of an agent that shall be used as the starting point for the agent creation
        #[arg(long, short)]
        from: Option<String>,
    },
    /// Edit an existing agent configuration
    Edit {
        /// Name of the agent to edit
        #[arg(long, short)]
        name: String,
    },
    /// Generate an agent configuration using AI
    Generate {},
    /// Delete the specified agent
    #[command(hide = true)]
    Delete {
        /// Name of the agent to delete
        name: String,
    },
    /// Switch to the specified agent
    #[command(hide = true)]
    Set {
        /// Name of the agent to switch to
        name: String,
    },
    /// Show agent config schema
    Schema,
    /// Define a default agent to use when q chat launches
    SetDefault {
        /// Name of the agent to set as default
        #[arg(long, short)]
        name: String,
    },
    /// Swap to a new agent at runtime
    #[command(alias = "switch")]
    Swap {
        /// Optional name of the agent to swap to. If not provided, a selection dialog will be shown
        name: Option<String>,
    },
}

fn prompt_mcp_server_selection(servers: &[McpServerInfo]) -> eyre::Result<Option<Vec<&McpServerInfo>>> {
    let items: Vec<String> = servers
        .iter()
        .map(|server| format!("{} ({})", server.name, server.config.command))
        .collect();

    let selections = match MultiSelect::new()
        .with_prompt("Select MCP servers (use Space to toggle, Enter to confirm)")
        .items(&items)
        .interact_on_opt(&dialoguer::console::Term::stdout())
    {
        Ok(sel) => sel,
        Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
            return Ok(None);
        },
        Err(e) => return Err(eyre::eyre!("Failed to get MCP server selection: {e}")),
    };

    let selected_servers: Vec<&McpServerInfo> = selections
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| servers.get(i))
        .collect();

    Ok(Some(selected_servers))
}

impl AgentSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let agents = &session.conversation.agents;

        macro_rules! _print_err {
            ($err:expr) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\nError: {}\n\n", $err)),
                    style::SetForegroundColor(Color::Reset)
                )?
            };
        }

        match self {
            Self::List => {
                let profiles = agents.agents.values().collect::<Vec<_>>();
                let active_profile = agents.get_active();

                for (i, profile) in profiles.iter().enumerate() {
                    if active_profile.is_some_and(|p| p == *profile) {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("* "),
                            style::Print(&profile.name),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    } else {
                        queue!(session.stderr, style::Print("  "), style::Print(&profile.name),)?;
                    }

                    if i < profiles.len().saturating_sub(1) {
                        queue!(session.stderr, style::Print("\n"))?;
                    }
                }
                execute!(session.stderr, style::Print("\n"))?;
            },
            Self::Schema => {
                use schemars::schema_for;

                let schema = schema_for!(Agent);
                let pretty = serde_json::to_string_pretty(&schema)
                    .map_err(|e| ChatError::Custom(format!("Failed to convert agent schema to string: {e}").into()))?;
                highlight_json(&mut session.stderr, pretty.as_str())
                    .map_err(|e| ChatError::Custom(format!("Error printing agent schema: {e}").into()))?;
            },
            Self::Create { name, directory, from } => {
                let mut agents = Agents::load(os, None, true, &mut session.stderr, session.conversation.mcp_enabled)
                    .await
                    .0;
                let path_with_file_name = create_agent(os, &mut agents, name.clone(), directory, from)
                    .await
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let mut cmd = std::process::Command::new(editor_cmd);

                let status = cmd.arg(&path_with_file_name).status()?;
                if !status.success() {
                    return Err(ChatError::Custom("Editor process did not exit with success".into()));
                }

                let new_agent = Agent::load(
                    os,
                    &path_with_file_name,
                    &mut None,
                    session.conversation.mcp_enabled,
                    &mut session.stderr,
                )
                .await;
                match new_agent {
                    Ok(agent) => {
                        session.conversation.agents.agents.insert(agent.name.clone(), agent);
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(&e),
                            style::Print("\n"),
                        )?;

                        return Err(ChatError::Custom(
                            format!("Post write validation failed for agent '{name}'. Malformed config detected: {e}")
                                .into(),
                        ));
                    },
                }

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("Agent "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(name),
                    style::SetForegroundColor(Color::Green),
                    style::Print(" has been created successfully"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Changes take effect on next launch"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },

            Self::Edit { name } => {
                let (_agent, path_with_file_name) = Agent::get_agent_by_name(os, &name)
                    .await
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let mut cmd = std::process::Command::new(editor_cmd);

                let status = cmd.arg(&path_with_file_name).status()?;
                if !status.success() {
                    return Err(ChatError::Custom("Editor process did not exit with success".into()));
                }

                let updated_agent = Agent::load(
                    os,
                    &path_with_file_name,
                    &mut None,
                    session.conversation.mcp_enabled,
                    &mut session.stderr,
                )
                .await;
                match updated_agent {
                    Ok(agent) => {
                        session.conversation.agents.agents.insert(agent.name.clone(), agent);
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(&e),
                            style::Print("\n"),
                        )?;

                        return Err(ChatError::Custom(
                            format!("Post edit validation failed for agent '{name}'. Malformed config detected: {e}")
                                .into(),
                        ));
                    },
                }

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("Agent "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(name),
                    style::SetForegroundColor(Color::Green),
                    style::Print(" has been edited successfully"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Changes take effect on next launch"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },

            Self::Generate {} => {
                let agent_name = match crate::util::input("Enter agent name: ", None) {
                    Ok(input) => input.trim().to_string(),
                    Err(_) => {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                };

                let agent_description = match crate::util::input("Enter agent description: ", None) {
                    Ok(input) => input.trim().to_string(),
                    Err(_) => {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                };

                let scope_options = vec!["Local (current workspace)", "Global (all workspaces)"];
                let scope_selection = match Select::with_theme(&crate::util::dialoguer_theme())
                    .with_prompt("Agent scope")
                    .items(&scope_options)
                    .default(0)
                    .interact_on_opt(&dialoguer::console::Term::stdout())
                {
                    Ok(sel) => {
                        let _ = crossterm::execute!(
                            std::io::stdout(),
                            crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
                        );
                        sel
                    },
                    // Ctrl‑C -> Err(Interrupted)
                    Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                    Err(e) => return Err(ChatError::Custom(format!("Failed to get scope selection: {e}").into())),
                };

                let scope_selection = match scope_selection {
                    Some(selection) => selection,
                    None => {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                };

                let is_global = scope_selection == 1;

                let mcp_servers = get_enabled_mcp_servers(os)
                    .await
                    .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                let selected_servers = if mcp_servers.is_empty() {
                    Vec::new()
                } else {
                    match prompt_mcp_server_selection(&mcp_servers)
                        .map_err(|e| ChatError::Custom(e.to_string().into()))?
                    {
                        Some(servers) => servers,
                        None => return Ok(ChatState::default()),
                    }
                };

                let mcp_servers_json = if !selected_servers.is_empty() {
                    let servers: std::collections::HashMap<String, serde_json::Value> = selected_servers
                        .iter()
                        .map(|server| {
                            (
                                server.name.clone(),
                                serde_json::to_value(&server.config).unwrap_or_default(),
                            )
                        })
                        .collect();
                    serde_json::to_string(&servers).unwrap_or_default()
                } else {
                    "{}".to_string()
                };
                use schemars::schema_for;
                let schema = schema_for!(Agent);
                let schema_string = serde_json::to_string_pretty(&schema)
                    .map_err(|e| ChatError::Custom(format!("Failed to serialize agent schema: {e}").into()))?;
                return session
                    .generate_agent_config(
                        os,
                        &agent_name,
                        &agent_description,
                        &mcp_servers_json,
                        &schema_string,
                        is_global,
                    )
                    .await;
            },
            Self::Set { .. } | Self::Delete { .. } => {
                // As part of the agent implementation, we are disabling the ability to
                // switch / create profile after a session has started.
                // TODO: perhaps revive this after we have a decision on profile create /
                // switch
                let global_path = if let Ok(path) = chat_global_agent_path(os) {
                    path.to_str().unwrap_or("default global agent path").to_string()
                } else {
                    "default global agent path".to_string()
                };
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!(
                        "To make changes or create agents, please do so via create the corresponding config in {}, where you would also find an example config for your reference.\nTo switch agent, launch another instance of q chat with --agent.\n\n",
                        global_path
                    )),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
            Self::SetDefault { name } => match session.conversation.agents.agents.get(&name) {
                Some(agent) => {
                    os.database
                        .settings
                        .set(Setting::ChatDefaultAgent, agent.name.clone())
                        .await
                        .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        style::Print("✓ Default agent set to '"),
                        style::Print(&agent.name),
                        style::Print("'. This will take effect the next time q chat is launched.\n"),
                        style::ResetColor,
                    )?;
                },
                None => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print("Error: "),
                        style::ResetColor,
                        style::Print(format!("No agent with name {name} found\n")),
                    )?;
                },
            },
            Self::Swap { name } => {
                if let Some(name) = name {
                    session.conversation.swap_agent(os, &mut session.stderr, &name).await?;
                } else {
                    let labels = session
                        .conversation
                        .agents
                        .agents
                        .keys()
                        .map(|name| name.as_str())
                        .collect::<Vec<_>>();

                    let name = {
                        let idx = match Select::with_theme(&crate::util::dialoguer_theme())
                            .with_prompt("Choose one of the following agents")
                            .items(&labels)
                            .default(1)
                            .interact_on_opt(&dialoguer::console::Term::stdout())
                        {
                            Ok(sel) => {
                                let _ = crossterm::execute!(
                                    std::io::stdout(),
                                    crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
                                );
                                sel
                            },
                            // Ctrl‑C -> Err(Interrupted)
                            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => None,
                            Err(e) => {
                                return Err(ChatError::Custom(
                                    format!("Dialog has failed to make a selection {e}").into(),
                                ));
                            },
                        };

                        idx.and_then(|idx| labels.get(idx).cloned().map(str::to_string))
                    };

                    if let Some(name) = name {
                        session.conversation.swap_agent(os, &mut session.stderr, &name).await?;
                    }
                }
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Create { .. } => "create",
            Self::Edit { .. } => "edit",
            Self::Generate { .. } => "generate",
            Self::Delete { .. } => "delete",
            Self::Set { .. } => "set",
            Self::Schema => "schema",
            Self::SetDefault { .. } => "set_default",
            Self::Swap { .. } => "swap",
        }
    }
}

fn highlight_json(output: &mut impl Write, json_str: &str) -> eyre::Result<()> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ps
        .find_syntax_by_extension("json")
        .ok_or(eyre::eyre!("No syntax found by extension"))?;
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    for line in LinesWithEndings::from(json_str) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps)?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        queue!(output, style::Print(escaped))?;
    }

    Ok(execute!(output, style::ResetColor)?)
}

/// Searches all configuration sources for MCP servers and returns a deduplicated list.
/// Priority order: Agent configs > Workspace legacy > Global legacy
pub async fn get_all_available_mcp_servers(os: &mut Os) -> Result<Vec<McpServerInfo>> {
    let mut servers = HashMap::<String, McpServerInfo>::new();

    // 1. Load from agent configurations (highest priority)
    let mut null_writer = NullWriter;
    let (agents, _) = Agents::load(os, None, true, &mut null_writer, true).await;

    for (_, agent) in agents.agents {
        for (server_name, server_config) in agent.mcp_servers.mcp_servers {
            if !servers.values().any(|s| s.config.command == server_config.command) {
                servers.insert(server_name.clone(), McpServerInfo {
                    name: server_name,
                    config: server_config,
                });
            }
        }
    }

    // 2. Load from workspace legacy config (medium priority)
    if let Ok(workspace_path) = directories::chat_legacy_workspace_mcp_config(os) {
        if let Ok(workspace_config) = McpServerConfig::load_from_file(os, workspace_path).await {
            for (server_name, server_config) in workspace_config.mcp_servers {
                if !servers.values().any(|s| s.config.command == server_config.command) {
                    servers.insert(server_name.clone(), McpServerInfo {
                        name: server_name,
                        config: server_config,
                    });
                }
            }
        }
    }

    // 3. Load from global legacy config (lowest priority)
    if let Ok(global_path) = directories::chat_legacy_global_mcp_config(os) {
        if let Ok(global_config) = McpServerConfig::load_from_file(os, global_path).await {
            for (server_name, server_config) in global_config.mcp_servers {
                if !servers.values().any(|s| s.config.command == server_config.command) {
                    servers.insert(server_name.clone(), McpServerInfo {
                        name: server_name,
                        config: server_config,
                    });
                }
            }
        }
    }

    Ok(servers.into_values().collect())
}

/// Get only enabled MCP servers (excludes disabled ones)
pub async fn get_enabled_mcp_servers(os: &mut Os) -> Result<Vec<McpServerInfo>> {
    let all_servers = get_all_available_mcp_servers(os).await?;
    Ok(all_servers
        .into_iter()
        .filter(|server| !server.config.disabled)
        .collect())
}
