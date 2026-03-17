use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use clap::Subcommand;
use crossterm::style::{
    self,
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
    AgentListDisplayInfo,
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
use crate::theme::StyledText;
use crate::util::NullWriter;

/// Represents the target directory scope for agent creation
#[derive(Debug, Clone, PartialEq)]
pub enum AgentDirectory {
    /// Local workspace directory (.kiro/agents/)
    Workspace,
    /// Global directory (~/.kiro/agents/)
    Global,
    /// Custom path specified by user
    Custom(PathBuf),
}

impl AgentDirectory {
    /// Parse a directory string argument into an AgentDirectory enum
    pub fn from_arg(arg: Option<&str>) -> Option<Self> {
        match arg {
            Some("workspace") => Some(Self::Workspace),
            Some("global") => Some(Self::Global),
            Some(path) => Some(Self::Custom(PathBuf::from(path))),
            None => None,
        }
    }

    /// Resolve the directory to an actual path string
    pub fn resolve(&self, os: &Os) -> Result<PathBuf, ChatError> {
        match self {
            Self::Workspace => os
                .path_resolver()
                .workspace()
                .agents_dir()
                .map_err(|e| ChatError::Custom(format!("Failed to resolve workspace agents directory: {e}").into())),
            Self::Global => os
                .path_resolver()
                .global()
                .agents_dir()
                .map_err(|e| ChatError::Custom(format!("Failed to resolve global agents directory: {e}").into())),
            Self::Custom(path) => Ok(path.clone()),
        }
    }

    /// Convert to Option<String> for create_agent function compatibility
    pub fn to_path_string(&self, os: &Os) -> Result<Option<String>, ChatError> {
        let path = self.resolve(os)?;
        Ok(Some(
            path.to_str()
                .ok_or_else(|| ChatError::Custom("Invalid path encoding".into()))?
                .to_string(),
        ))
    }
}

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    before_long_help = "Agents allow you to organize and manage different sets of context files for different projects or tasks.

Notes
• Launch kiro-cli chat with a specific agent with --agent
• Construct an agent under ~/.kiro/agents/ (accessible globally) or cwd/.kiro/agents (accessible in workspace)
• See example config under global directory
• Set default agent to assume with settings by running \"kiro-cli settings chat.defaultAgent agent_name\"
• Each agent maintains its own set of context and customizations"
)]
/// Subcommands for managing agents in the chat CLI
pub enum AgentSubcommand {
    /// List all available agents
    List,
    /// Create a new agent with AI assistance (or use --manual for simple creation)
    #[command(alias = "generate")]
    Create {
        /// Name of the agent to be created (optional - will prompt if not provided)
        name: Option<String>,
        /// Description of the agent (optional - will prompt if not provided)
        #[arg(long, short = 'D', conflicts_with_all = ["manual", "from"])]
        description: Option<String>,
        /// Directory for agent: "workspace", "global", or a custom path
        /// (optional - will prompt if not provided)
        #[arg(long, short)]
        directory: Option<String>,
        /// MCP server to include (can be used multiple times)
        #[arg(long, short = 'm', conflicts_with_all = ["manual", "from"])]
        mcp_server: Vec<String>,
        /// Name of an agent to use as template (implies --manual)
        #[arg(long, short)]
        from: Option<String>,
        /// Use simple creation mode (opens editor instead of AI generation)
        #[arg(long)]
        manual: bool,
    },
    /// Edit an existing agent configuration
    Edit {
        /// Name of the agent to edit
        name: Option<String>,
        /// Path to the agent config file to edit
        #[arg(long)]
        path: Option<String>,
    },
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
    /// Define a default agent to use when kiro-cli chat launches
    SetDefault {
        /// Optional name of the agent to set as default. If not provided, a selection dialog will
        /// be shown
        name: Option<String>,
    },
    /// Swap to a new agent at runtime
    #[command(alias = "switch")]
    Swap {
        /// Optional name of the agent to swap to. If not provided, a selection dialog will be shown
        name: Option<String>,
    },
}

/// Options for the agent selector
pub struct AgentSelectorOptions<'a> {
    /// Prompt to display in the selector
    pub prompt: &'a str,
    /// Whether to exclude built-in agents from the list
    pub exclude_builtins: bool,
}

impl Default for AgentSelectorOptions<'_> {
    fn default() -> Self {
        Self {
            prompt: "Select agent (type to search): ",
            exclude_builtins: false,
        }
    }
}

/// Launch a fuzzy selector to choose an agent from the available agents.
/// Returns the selected agent name, or None if the user cancelled.
fn prompt_agent_selection(agents: &Agents, options: AgentSelectorOptions<'_>) -> Result<Option<String>, ChatError> {
    let active_agent_name = &agents.active_idx;
    let mut agent_infos: Vec<AgentListDisplayInfo> = agents
        .agents
        .iter()
        .filter(|(_, agent)| !options.exclude_builtins || !agent.is_builtin())
        .map(|(name, agent)| {
            let is_active = name == active_agent_name;
            AgentListDisplayInfo::new(
                name.clone(),
                agent.source_location,
                agent.description.clone(),
                is_active,
            )
        })
        .collect();

    if agent_infos.is_empty() {
        return if options.exclude_builtins {
            Err(ChatError::Custom(
                "No editable agents found. Create a new agent with '/agent create'".into(),
            ))
        } else {
            Err(ChatError::Custom("No agents available".into()))
        };
    }

    AgentListDisplayInfo::sort_list(&mut agent_infos);
    let formatted_items = AgentListDisplayInfo::format_for_selector(&agent_infos);

    // Platform-specific selector
    #[cfg(unix)]
    let selected_idx = {
        let selected =
            super::super::skim_integration::launch_skim_selector_inline(&formatted_items, options.prompt, false)
                .map_err(|e| ChatError::Custom(format!("Failed to launch agent selector: {e}").into()))?;

        if let Some(selections) = selected
            && let Some(selected_line) = selections.first()
        {
            formatted_items.iter().position(|item| item == selected_line)
        } else {
            None
        }
    };

    #[cfg(windows)]
    let selected_idx = {
        use dialoguer::Select;

        match Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt(options.prompt)
            .items(&formatted_items)
            .default(0)
            .interact_on_opt(&dialoguer::console::Term::stdout())
        {
            Ok(sel) => sel,
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => None,
            Err(e) => return Err(ChatError::Custom(format!("Failed to get agent selection: {e}").into())),
        }
    };

    if let Some(idx) = selected_idx {
        Ok(Some(agent_infos[idx].name.clone()))
    } else {
        // User cancelled selection
        Ok(None)
    }
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
                    StyledText::error_fg(),
                    style::Print(format!("\nError: {}\n\n", $err)),
                    StyledText::reset(),
                )?
            };
        }

        match self {
            Self::List => {
                let profiles = agents.agents.values().collect::<Vec<_>>();
                let active_profile = agents.get_active();

                // Print directory header
                AgentListDisplayInfo::render_directory_header(&mut session.stderr, os)?;

                let mut agent_infos: Vec<AgentListDisplayInfo> = profiles
                    .iter()
                    .map(|profile| {
                        let is_active = active_profile.is_some_and(|p| p == *profile);
                        AgentListDisplayInfo::new(
                            profile.name.clone(),
                            profile.source_location,
                            profile.description.clone(),
                            is_active,
                        )
                    })
                    .collect();

                AgentListDisplayInfo::sort_list(&mut agent_infos);
                AgentListDisplayInfo::render_list(&mut session.stderr, &agent_infos, true)?;
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
            Self::Create {
                name,
                description,
                directory,
                mcp_server,
                from,
                manual,
            } => {
                let directory_arg = AgentDirectory::from_arg(directory.as_deref());

                // If --from is provided, automatically use manual mode
                if manual || from.is_some() {
                    return create_agent_manual(os, session, name, directory_arg, from).await;
                } else {
                    return create_agent_ai_assisted(os, session, name, description, directory_arg, mcp_server).await;
                }
            },

            Self::Edit { name, path } => {
                use std::path::PathBuf;

                // Helper to check if agent is built-in and return error if so
                let check_not_builtin = |agent_name: &str| -> Result<(), ChatError> {
                    if let Some(agent) = session.conversation.agents.agents.get(agent_name)
                        && agent.is_builtin()
                    {
                        return Err(ChatError::Custom(
                            format!(
                                "Cannot edit built-in agent '{}'. Create a new agent with '/agent create'",
                                agent_name
                            )
                            .into(),
                        ));
                    }
                    Ok(())
                };

                let mut show_both_params_warning = false;
                let (agent_name, path_with_file_name) = match (name, path) {
                    (Some(name), None) => {
                        check_not_builtin(&name)?;
                        let (_agent, path) = Agent::get_agent_by_name(os, &name)
                            .await
                            .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                        (name, path)
                    },
                    (None, Some(path_arg)) => {
                        let path = PathBuf::from(&path_arg);
                        if !os.fs.exists(&path) {
                            return Err(ChatError::Custom(
                                format!("Agent config file not found at path: {}", path.display()).into(),
                            ));
                        }
                        let content = os
                            .fs
                            .read(&path)
                            .await
                            .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                        let agent = serde_json::from_slice::<Agent>(&content)
                            .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                        (agent.name.clone(), path)
                    },
                    (Some(name), Some(path_arg)) => {
                        check_not_builtin(&name)?;
                        // --name takes priority, but warn if --path points to a different agent
                        let (_agent, path) = Agent::get_agent_by_name(os, &name)
                            .await
                            .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                        let file_path = PathBuf::from(&path_arg);
                        if os.fs.exists(&file_path) && file_path != path {
                            show_both_params_warning = true;
                        }

                        (name, path)
                    },
                    (None, None) => {
                        // Show fuzzy selector to choose an agent to edit
                        let selected_name =
                            prompt_agent_selection(&session.conversation.agents, AgentSelectorOptions {
                                prompt: "Select agent to edit (type to search): ",
                                exclude_builtins: true,
                            })?;

                        if let Some(agent_name) = selected_name {
                            let (_agent, path) = Agent::get_agent_by_name(os, &agent_name)
                                .await
                                .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                            (agent_name, path)
                        } else {
                            // User cancelled selection
                            return Ok(ChatState::PromptUser {
                                skip_printing_tools: true,
                            });
                        }
                    },
                };

                // Create a temporary copy for editing
                let original_content = os
                    .fs
                    .read(&path_with_file_name)
                    .await
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                let temp_file = tempfile::Builder::new()
                    .prefix(&format!("{}_", agent_name))
                    .suffix(".json")
                    .tempfile()
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                let temp_path = temp_file.path().to_path_buf();

                os.fs
                    .write(&temp_path, &original_content)
                    .await
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                loop {
                    crate::util::editor::launch_editor(&temp_path)
                        .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                    let updated_agent = Agent::load(
                        os,
                        &temp_path,
                        &mut None,
                        session.conversation.mcp_enabled,
                        &mut session.stderr,
                    )
                    .await;

                    match updated_agent {
                        Ok(_agent) => {
                            // Validation succeeded - copy temp file to actual location
                            let temp_content = os
                                .fs
                                .read(&temp_path)
                                .await
                                .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                            os.fs
                                .write(&path_with_file_name, &temp_content)
                                .await
                                .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                            // Reload from actual path to get correct source_location
                            let final_agent = Agent::load(
                                os,
                                &path_with_file_name,
                                &mut None,
                                session.conversation.mcp_enabled,
                                &mut session.stderr,
                            )
                            .await
                            .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                            session
                                .conversation
                                .agents
                                .agents
                                .insert(final_agent.name.clone(), final_agent);
                            break;
                        },
                        Err(e) => {
                            execute!(
                                session.stderr,
                                StyledText::error_fg(),
                                style::Print("Error: "),
                                StyledText::reset(),
                                style::Print(&e),
                                style::Print("\n\n"),
                            )?;

                            let choices = vec!["Continue editing", "Cancel"];
                            let selection = Select::new()
                                .with_prompt("What would you like to do?")
                                .items(&choices)
                                .default(0)
                                .interact_opt()
                                .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

                            if matches!(selection, Some(1) | None) {
                                // Cancel - temp file will be cleaned up automatically
                                execute!(
                                    session.stderr,
                                    StyledText::warning_fg(),
                                    style::Print("✓ Edit cancelled, original file unchanged\n"),
                                    StyledText::reset(),
                                )?;

                                return Ok(ChatState::PromptUser {
                                    skip_printing_tools: true,
                                });
                            }
                            // Continue editing (loop again)
                        },
                    }
                }

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("Agent "),
                    StyledText::brand_fg(),
                    style::Print(&agent_name),
                    StyledText::success_fg(),
                    style::Print(" has been edited successfully"),
                    StyledText::reset(),
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("Changes take effect on next launch"),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;

                if show_both_params_warning {
                    execute!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print("⚠ Warning: "),
                        StyledText::reset(),
                        style::Print(format!(
                            "Both --name and --path were provided. Used agent '{agent_name}' (ignored --path)\n"
                        )),
                    )?;
                }
            },

            Self::Set { .. } | Self::Delete { .. } => {
                // As part of the agent implementation, we are disabling the ability to
                // switch / create profile after a session has started.
                // TODO: perhaps revive this after we have a decision on profile create /
                // switch
                let global_path = if let Ok(path) = os.path_resolver().global().agents_dir() {
                    path.to_str().unwrap_or("default global agent path").to_string()
                } else {
                    "default global agent path".to_string()
                };
                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(format!(
                        "To make changes or create agents, please do so via create the corresponding config in {global_path}, where you would also find an example config for your reference.\nTo switch agent, launch another instance of kiro-cli chat with --agent.\n\n"
                    )),
                    StyledText::reset_attributes()
                )?;
            },
            Self::SetDefault { name } => {
                let agent_name = if let Some(name) = name {
                    name
                } else {
                    // Show fuzzy selector to choose an agent to set as default
                    let selected_name = prompt_agent_selection(&session.conversation.agents, AgentSelectorOptions {
                        prompt: "Select default agent (type to search): ",
                        exclude_builtins: false,
                    })?;

                    match selected_name {
                        Some(name) => name,
                        None => {
                            // User cancelled selection
                            return Ok(ChatState::PromptUser {
                                skip_printing_tools: true,
                            });
                        },
                    }
                };

                match session.conversation.agents.agents.get(&agent_name) {
                    Some(agent) => {
                        os.database
                            .settings
                            .set(Setting::ChatDefaultAgent, agent.name.clone(), None)
                            .await
                            .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                        execute!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Default agent set to '"),
                            style::Print(&agent.name),
                            style::Print("'. This will take effect the next time kiro-cli chat is launched.\n"),
                            StyledText::reset(),
                        )?;
                    },
                    None => {
                        execute!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print("Error: "),
                            StyledText::reset(),
                            style::Print(format!("No agent with name {agent_name} found\n")),
                        )?;
                    },
                }
            },
            Self::Swap { name } => {
                let agent_name = if let Some(name) = name {
                    name
                } else {
                    let active_agent_name = &agents.active_idx;
                    let mut agent_infos: Vec<AgentListDisplayInfo> = agents
                        .agents
                        .iter()
                        .map(|(name, agent)| {
                            let is_active = name == active_agent_name;
                            AgentListDisplayInfo::new(
                                name.clone(),
                                agent.source_location,
                                agent.description.clone(),
                                is_active,
                            )
                        })
                        .collect();

                    AgentListDisplayInfo::sort_list(&mut agent_infos);
                    let formatted_items = AgentListDisplayInfo::format_for_selector(&agent_infos);

                    // Platform-specific selector
                    #[cfg(unix)]
                    let selected_idx = {
                        // Launch fuzzy selector (inline mode)
                        let selected = super::super::skim_integration::launch_skim_selector_inline(
                            &formatted_items,
                            "Select agent (type to search): ",
                            false,
                        )
                        .map_err(|e| ChatError::Custom(format!("Failed to launch agent selector: {e}").into()))?;

                        if let Some(selections) = selected
                            && let Some(selected_line) = selections.first()
                        {
                            formatted_items.iter().position(|item| item == selected_line)
                        } else {
                            None
                        }
                    };

                    #[cfg(windows)]
                    let selected_idx = {
                        use dialoguer::Select;

                        match Select::with_theme(&crate::util::dialoguer_theme())
                            .with_prompt("Select agent")
                            .items(&formatted_items)
                            .default(0)
                            .interact_on_opt(&dialoguer::console::Term::stdout())
                        {
                            Ok(sel) => sel,
                            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => None,
                            Err(e) => {
                                return Err(ChatError::Custom(format!("Failed to get agent selection: {e}").into()));
                            },
                        }
                    };

                    if let Some(idx) = selected_idx {
                        agent_infos[idx].name.clone()
                    } else {
                        // User cancelled selection
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    }
                };

                session
                    .conversation
                    .swap_agent(os, &mut session.stderr, &agent_name)
                    .await?;
                session
                    .input_source
                    .agent_swap_state()
                    .set_current_agent(agent_name.clone());

                // Display welcome message if the agent has one
                if let Some(agent) = session.conversation.agents.get_active() {
                    agent.print_welcome_message(&mut session.stderr)?;
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

    Ok(execute!(output, StyledText::reset())?)
}

/// Searches all configuration sources for MCP servers and returns a deduplicated list.
/// Priority order: Agent configs > Workspace legacy > Global legacy
pub async fn get_all_available_mcp_servers(os: &mut Os) -> Result<Vec<McpServerInfo>> {
    let mut servers = HashMap::<String, McpServerInfo>::new();

    // 1. Load from agent configurations (highest priority)
    let mut null_writer = NullWriter;
    let (agents, _) = Agents::load(os, None, true, &mut null_writer, true, false).await;

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

    let resolver = os.path_resolver();

    // 2. Load from workspace legacy config (medium priority)
    if let Ok(workspace_path) = resolver.workspace().mcp_config()
        && let Ok(workspace_config) = McpServerConfig::load_from_file(os, workspace_path).await
    {
        for (server_name, server_config) in workspace_config.mcp_servers {
            if !servers.values().any(|s| s.config.command == server_config.command) {
                servers.insert(server_name.clone(), McpServerInfo {
                    name: server_name,
                    config: server_config,
                });
            }
        }
    }

    // 3. Load from global legacy config (lowest priority)
    if let Ok(global_path) = resolver.global().mcp_config()
        && let Ok(global_config) = McpServerConfig::load_from_file(os, global_path).await
    {
        for (server_name, server_config) in global_config.mcp_servers {
            if !servers.values().any(|s| s.config.command == server_config.command) {
                servers.insert(server_name.clone(), McpServerInfo {
                    name: server_name,
                    config: server_config,
                });
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

/// Create an agent using manual mode (opens editor for configuration).
/// This provides a simple creation flow similar to the original `/agent create` command.
async fn create_agent_manual(
    os: &mut Os,
    session: &mut ChatSession,
    name: Option<String>,
    directory: Option<AgentDirectory>,
    from: Option<String>,
) -> Result<ChatState, ChatError> {
    let agent_name = match name {
        Some(n) => n,
        None => match crate::util::input("Enter agent name: ", None) {
            Ok(input) => input.trim().to_string(),
            Err(_) => {
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        },
    };

    // Clone the session's already-loaded agents instead of reloading from disk
    // (Agents::load scans all directories and parses every JSON file, which is slow)
    let mut agents = session.conversation.agents.clone();

    // Resolve directory using the enum
    let resolved_directory = match &directory {
        Some(dir) => dir.to_path_string(os)?,
        None => None,
    };

    let path_with_file_name = create_agent(os, &mut agents, agent_name.clone(), resolved_directory, from)
        .await
        .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

    crate::util::editor::launch_editor(&path_with_file_name)
        .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;

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
            // Only load the agent into the current session if it's in a recognized location
            // Custom paths are not part of the session's agent search paths
            if agent.source_location != crate::cli::agent::AgentSourceLocation::BuiltIn {
                session.conversation.agents.agents.insert(agent.name.clone(), agent);
            }
        },
        Err(e) => {
            execute!(
                session.stderr,
                StyledText::error_fg(),
                style::Print("Error: "),
                StyledText::reset(),
                style::Print(&e),
                style::Print("\n"),
            )?;

            return Err(ChatError::Custom(
                format!(
                    "Post write validation failed for agent '{}'. Malformed config detected: {e}",
                    agent_name
                )
                .into(),
            ));
        },
    }

    execute!(
        session.stderr,
        StyledText::success_fg(),
        style::Print("Agent "),
        StyledText::brand_fg(),
        style::Print(&agent_name),
        StyledText::success_fg(),
        style::Print(" has been created successfully"),
        StyledText::reset(),
        style::Print("\n"),
    )?;

    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

/// Create an agent using AI-assisted mode (default).
/// Prompts for any missing values and uses AI to generate the agent configuration.
async fn create_agent_ai_assisted(
    os: &mut Os,
    session: &mut ChatSession,
    name: Option<String>,
    description: Option<String>,
    directory: Option<AgentDirectory>,
    mcp_server: Vec<String>,
) -> Result<ChatState, ChatError> {
    // Get agent name (prompt if not provided)
    let agent_name = match name {
        Some(n) => n,
        None => match crate::util::input("Enter agent name: ", None) {
            Ok(input) => input.trim().to_string(),
            Err(_) => {
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        },
    };

    // Get agent description (prompt if not provided)
    let agent_description = match description {
        Some(d) => d,
        None => match crate::util::input("Enter agent description: ", None) {
            Ok(input) => input.trim().to_string(),
            Err(_) => {
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        },
    };

    // Get scope/directory (use global default if not provided)
    let save_path: Option<PathBuf> = match directory {
        Some(ref dir) => Some(dir.resolve(os)?),
        None => {
            // Use global agents directory as default (same as manual creation)
            Some(
                os.path_resolver()
                    .global()
                    .agents_dir_for_create()
                    .map_err(|e| ChatError::Custom(format!("Failed to resolve global agents directory: {e}").into()))?,
            )
        },
    };

    // Get MCP servers (use provided or prompt for selection)
    let mcp_servers = get_enabled_mcp_servers(os)
        .await
        .map_err(|e| ChatError::Custom(e.to_string().into()))?;

    let selected_servers: Vec<&McpServerInfo> = if !mcp_server.is_empty() {
        // Validate and filter provided MCP server names
        let mut selected = Vec::new();
        for server_name in &mcp_server {
            if let Some(server) = mcp_servers.iter().find(|s| &s.name == server_name) {
                selected.push(server);
            } else {
                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(format!("Warning: MCP server '{}' not found, skipping.\n", server_name)),
                    StyledText::reset(),
                )?;
            }
        }
        selected
    } else if mcp_servers.is_empty() {
        Vec::new()
    } else {
        // Prompt for MCP server selection
        match prompt_mcp_server_selection(&mcp_servers).map_err(|e| ChatError::Custom(e.to_string().into()))? {
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

    session
        .generate_agent_config(
            os,
            &agent_name,
            &agent_description,
            &mcp_servers_json,
            &schema_string,
            save_path,
        )
        .await
}
