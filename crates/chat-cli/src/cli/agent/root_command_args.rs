use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{
    Args,
    Subcommand,
};
use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    bail,
};
use schemars::schema_for;

use super::{
    Agent,
    AgentListDisplayInfo,
    Agents,
    McpServerConfig,
    legacy,
};
use crate::constants::CLI_NAME;
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::StyledText;

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub enum AgentSubcommands {
    /// List the available agents. Note that local agents are only discovered if the command is
    /// invoked at a directory that contains them
    List,
    /// Create an agent config. If path is not provided, Kiro CLI shall create this config in the
    /// global agent directory
    Create {
        /// Name of the agent to be created (can also be provided via --name for backwards
        /// compatibility)
        name: Option<String>,
        /// Name of the agent to be created (deprecated: use positional argument instead)
        #[arg(long = "name", short = 'n', hide = true)]
        name_flag: Option<String>,
        /// The directory where the agent will be saved. If not provided, the agent will be saved in
        /// the global agent directory
        #[arg(long, short)]
        directory: Option<String>,
        /// The name of an agent that shall be used as the starting point for the agent creation
        #[arg(long, short)]
        from: Option<String>,
    },
    /// Edit an existing agent config
    Edit {
        /// Name of the agent to edit (can also be provided via --name for backwards compatibility)
        name: Option<String>,
        /// Name of the agent to edit (deprecated: use positional argument instead)
        #[arg(long = "name", short = 'n', hide = true)]
        name_flag: Option<String>,
        /// Path to the agent config file to edit
        #[arg(long)]
        path: Option<String>,
    },
    /// Validate a config with the given path
    Validate {
        #[arg(long, short)]
        path: String,
    },
    /// Migrate profiles to agent
    /// Note that doing this is potentially destructive to agents that are already in the global
    /// agent directories
    Migrate {
        #[arg(long)]
        force: bool,
    },
    /// Define a default agent to use when q chat launches
    SetDefault {
        /// Name of the agent to set as default (can also be provided via --name for backwards
        /// compatibility)
        name: Option<String>,
        /// Name of the agent to set as default (deprecated: use positional argument instead)
        #[arg(long = "name", short = 'n', hide = true)]
        name_flag: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    cmd: Option<AgentSubcommands>,
}

impl AgentArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        let mut stderr = std::io::stderr();

        // For non-enterprise users, skip the API call and default to enabled
        let is_enterprise = crate::auth::builder_id::is_idc_user(&os.database).await;
        let (mcp_enabled, mcp_api_failure) = if !is_enterprise {
            (true, false)
        } else {
            match os.client.is_mcp_enabled(&os.database).await {
                Ok(enabled) => (enabled, false),
                Err(err) => {
                    // Check if this is a GetProfile API error
                    let is_api_failure = matches!(err, crate::api_client::ApiClientError::GetProfileError(_));
                    tracing::warn!(?err, "Failed to check MCP configuration, defaulting to disabled");
                    (false, is_api_failure)
                },
            }
        };
        match self.cmd {
            Some(AgentSubcommands::List) | None => {
                let agents = Agents::load(os, None, true, &mut stderr, mcp_enabled, mcp_api_failure)
                    .await
                    .0;

                // Print directory header
                AgentListDisplayInfo::render_directory_header(&mut stderr, os)?;

                let active_agent_name = agents.active_idx.clone();
                let mut agent_infos: Vec<AgentListDisplayInfo> = agents
                    .agents
                    .into_iter()
                    .map(|(name, agent)| {
                        let is_active = name == active_agent_name;
                        AgentListDisplayInfo::new(name, agent.source_location, agent.description.clone(), is_active)
                    })
                    .collect();

                AgentListDisplayInfo::sort_list(&mut agent_infos);
                AgentListDisplayInfo::render_list(&mut stderr, &agent_infos, false)?;
                writeln!(stderr)?;
            },
            Some(AgentSubcommands::Create {
                name,
                name_flag,
                directory,
                from,
            }) => {
                // Positional argument takes precedence over --name flag (which only exists for backwards
                // compatibility)
                let name = match (name, name_flag) {
                    (Some(n), _) => n,    // Positional argument has priority
                    (None, Some(n)) => n, // Fall back to --name flag
                    (None, None) => {
                        bail!("Agent name is required. Usage: {CLI_NAME} agent create <name>");
                    },
                };

                let mut agents = Agents::load(os, None, true, &mut stderr, mcp_enabled, mcp_api_failure)
                    .await
                    .0;
                let path_with_file_name = create_agent(os, &mut agents, name.clone(), directory, from).await?;

                crate::util::editor::launch_editor(&path_with_file_name)?;

                let Ok(content) = os.fs.read(&path_with_file_name).await else {
                    bail!(
                        "Post write validation failed. Error opening {}. Aborting",
                        path_with_file_name.display()
                    );
                };
                if let Err(e) = serde_json::from_slice::<Agent>(&content) {
                    bail!(
                        "Post write validation failed for agent '{name}' at path: {}. Malformed config detected: {e}",
                        path_with_file_name.display()
                    );
                }

                writeln!(
                    stderr,
                    "\n📁 Created agent {} '{}'\n",
                    name,
                    path_with_file_name.display()
                )?;
            },
            Some(AgentSubcommands::Edit { name, name_flag, path }) => {
                // Positional argument takes precedence over --name flag (which only exists for backwards
                // compatibility)
                let name = match (name, name_flag) {
                    (Some(n), _) => Some(n),    // Positional argument has priority
                    (None, Some(n)) => Some(n), // Fall back to --name flag
                    (None, None) => None,
                };

                let agents = Agents::load(os, None, true, &mut stderr, mcp_enabled, mcp_api_failure)
                    .await
                    .0;

                // Helper to check if agent is built-in and return error if so
                let check_not_builtin = |agent_name: &str| -> Result<()> {
                    if let Some(agent) = agents.agents.get(agent_name)
                        && agent.is_builtin()
                    {
                        bail!(
                            "Cannot edit built-in agent '{}'. Create a new agent with '{} agent create'",
                            agent_name,
                            CLI_NAME
                        );
                    }
                    Ok(())
                };

                let mut show_both_params_warning = false;
                let (agent_name, path_with_file_name) = match (name, path) {
                    (Some(name), None) => {
                        check_not_builtin(&name)?;
                        let (_agent, path) = Agent::get_agent_by_name(os, &name).await?;
                        (name, path)
                    },
                    (None, Some(path_arg)) => {
                        let path = PathBuf::from(&path_arg);
                        if !os.fs.exists(&path) {
                            bail!("Agent config file not found at path: {}", path.display());
                        }
                        let content = os.fs.read(&path).await?;
                        let agent = serde_json::from_slice::<Agent>(&content)?;
                        (agent.name.clone(), path)
                    },
                    (Some(name), Some(path_arg)) => {
                        check_not_builtin(&name)?;
                        // --name takes priority, but warn if --path points to a different agent
                        let (_agent, path) = Agent::get_agent_by_name(os, &name).await?;

                        let file_path = PathBuf::from(&path_arg);
                        if os.fs.exists(&file_path) && file_path != path {
                            show_both_params_warning = true;
                        }

                        (name, path)
                    },
                    (None, None) => {
                        // Default to editing the current (default) agent
                        let current_agent_name = agents.active_idx.clone();
                        check_not_builtin(&current_agent_name)?;
                        let (_agent, path) = Agent::get_agent_by_name(os, &current_agent_name).await?;
                        (current_agent_name, path)
                    },
                };

                crate::util::editor::launch_editor(&path_with_file_name)?;

                let Ok(content) = os.fs.read(&path_with_file_name).await else {
                    bail!(
                        "Post edit validation failed. Error opening {}. Aborting",
                        path_with_file_name.display()
                    );
                };
                if let Err(e) = serde_json::from_slice::<Agent>(&content) {
                    bail!(
                        "Post edit validation failed for agent '{}' at path: {}. Malformed config detected: {e}",
                        agent_name,
                        path_with_file_name.display()
                    );
                }

                writeln!(
                    stderr,
                    "\n✏️  Edited agent {} '{}'\n",
                    agent_name,
                    path_with_file_name.display()
                )?;

                if show_both_params_warning {
                    let _ = queue!(
                        stderr,
                        StyledText::warning_fg(),
                        style::Print("⚠ Warning: "),
                        StyledText::reset(),
                        style::Print(format!(
                            "Both --name and --path were provided. Used agent '{agent_name}' (ignored --path)\n\n"
                        )),
                    );
                    stderr.flush()?;
                }
            },
            Some(AgentSubcommands::Validate { path }) => {
                let mut global_mcp_config = None::<McpServerConfig>;
                let agent = Agent::load(os, path.as_str(), &mut global_mcp_config, mcp_enabled, &mut stderr).await;

                'validate: {
                    match agent {
                        Ok(agent) => {
                            let Ok(instance) = serde_json::to_value(&agent) else {
                                queue!(
                                    stderr,
                                    StyledText::error_fg(),
                                    style::Print("Error: "),
                                    StyledText::reset(),
                                    style::Print("failed to obtain value from agent provided. Aborting validation"),
                                )?;
                                break 'validate;
                            };

                            let schema = match serde_json::to_value(schema_for!(Agent)) {
                                Ok(schema) => schema,
                                Err(e) => {
                                    queue!(
                                        stderr,
                                        StyledText::error_fg(),
                                        style::Print("Error: "),
                                        StyledText::reset(),
                                        style::Print(format!("failed to obtain schema: {e}. Aborting validation"))
                                    )?;
                                    break 'validate;
                                },
                            };

                            if let Err(e) = jsonschema::validate(&schema, &instance).map_err(|e| e.to_owned()) {
                                let name = &agent.name;
                                queue!(
                                    stderr,
                                    StyledText::warning_fg(),
                                    style::Print("WARNING "),
                                    StyledText::reset(),
                                    style::Print("Agent config "),
                                    StyledText::success_fg(),
                                    style::Print(name),
                                    StyledText::reset(),
                                    style::Print(" is malformed at "),
                                    StyledText::warning_fg(),
                                    style::Print(&e.instance_path),
                                    StyledText::reset(),
                                    style::Print(format!(": {e}\n")),
                                )?;
                            }
                        },
                        Err(e) => {
                            let _ = queue!(
                                stderr,
                                StyledText::error_fg(),
                                style::Print("Error: "),
                                StyledText::reset(),
                                style::Print(e),
                                style::Print("\n"),
                            );
                        },
                    }
                }

                stderr.flush()?;
            },
            Some(AgentSubcommands::Migrate { force }) => {
                if !force {
                    let _ = queue!(
                        stderr,
                        StyledText::warning_fg(),
                        style::Print("WARNING: "),
                        StyledText::reset(),
                        style::Print(
                            "manual migrate is potentially destructive to existing agent configs with name collision. Use"
                        ),
                        StyledText::brand_fg(),
                        style::Print(" --force "),
                        StyledText::reset(),
                        style::Print("to run"),
                        style::Print("\n"),
                    );
                    return Ok(ExitCode::SUCCESS);
                }

                match legacy::migrate(os, force).await {
                    Ok(Some(new_agents)) => {
                        let migrated_count = new_agents.len();
                        let _ = queue!(
                            stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Success: "),
                            StyledText::reset(),
                            style::Print(format!(
                                "Profile migration successful. Migrated {migrated_count} agent(s)\n"
                            )),
                        );
                    },
                    Ok(None) => {
                        let _ = queue!(
                            stderr,
                            StyledText::info_fg(),
                            style::Print("Info: "),
                            StyledText::reset(),
                            style::Print("Migration was not performed. Nothing to migrate\n"),
                        );
                    },
                    Err(e) => {
                        let _ = queue!(
                            stderr,
                            StyledText::error_fg(),
                            style::Print("Error: "),
                            StyledText::reset(),
                            style::Print(format!("Migration did not happen for the following reason: {e}\n")),
                        );
                    },
                }
            },
            Some(AgentSubcommands::SetDefault { name, name_flag }) => {
                // Positional argument takes precedence over --name flag (which only exists for backwards
                // compatibility)
                let name = match (name, name_flag) {
                    (Some(n), _) => n,    // Positional argument has priority
                    (None, Some(n)) => n, // Fall back to --name flag
                    (None, None) => {
                        bail!("Agent name is required. Usage: {CLI_NAME} agent set-default <name>");
                    },
                };

                let mut agents = Agents::load(os, None, true, &mut stderr, mcp_enabled, mcp_api_failure)
                    .await
                    .0;
                match agents.switch(&name, os).await {
                    Ok(agent) => {
                        os.database
                            .settings
                            .set(Setting::ChatDefaultAgent, agent.name.clone(), None)
                            .await?;

                        let _ = queue!(
                            stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Default agent set to '"),
                            style::Print(&agent.name),
                            style::Print(format!(
                                "'. This will take effect the next time {CLI_NAME} chat is launched.\n"
                            )),
                            StyledText::reset(),
                        );
                    },
                    Err(e) => {
                        let _ = queue!(
                            stderr,
                            StyledText::error_fg(),
                            style::Print("Error: "),
                            StyledText::reset(),
                            style::Print(format!("Failed to set default agent: {e}\n")),
                        );
                    },
                }
            },
        }

        Ok(ExitCode::SUCCESS)
    }
}

pub async fn create_agent(
    os: &mut Os,
    agents: &mut Agents,
    name: String,
    path: Option<String>,
    from: Option<String>,
) -> Result<PathBuf> {
    let path = if let Some(path) = path {
        let mut path = PathBuf::from(path);
        if path.is_relative() {
            path = os.env.current_dir()?.join(path);
        }

        if !path.is_dir() {
            bail!("Path must be a directory");
        }

        os.path_resolver().workspace().agents_dir_for_create()?
    } else {
        os.path_resolver().global().agents_dir_for_create()?
    };

    if let Some((name, _)) = agents.agents.iter().find(|(agent_name, agent)| {
        &name == *agent_name
            && agent
                .path
                .as_ref()
                .is_some_and(|agent_path| agent_path.parent().is_some_and(|parent| parent == path))
    }) {
        bail!("Agent with name {name} already exists. Aborting");
    }

    let prepopulated_content = if let Some(from) = from {
        let mut agent_to_copy = agents.switch(from.as_str(), os).await?.clone();
        agent_to_copy.name = name.clone();
        agent_to_copy
    } else {
        Agent {
            name: name.clone(),
            description: Some(Default::default()),
            ..Default::default()
        }
    }
    .to_str_pretty()?;
    let path_with_file_name = path.join(format!("{name}.json"));

    if !path.exists() {
        os.fs.create_dir_all(&path).await?;
    }
    os.fs.create_new(&path_with_file_name).await?;
    os.fs.write(&path_with_file_name, prepopulated_content).await?;

    Ok(path_with_file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RootSubcommand;
    use crate::util::test::assert_parse;

    #[test]
    fn test_agent_subcommand_list() {
        assert_parse!(
            ["agent", "list"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::List)
            })
        );
    }

    #[test]
    fn test_agent_subcommand_create() {
        // Test positional name argument
        assert_parse!(
            ["agent", "create", "some_agent", "--from", "some_old_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Create {
                    name: Some("some_agent".to_string()),
                    name_flag: None,
                    directory: None,
                    from: Some("some_old_agent".to_string())
                })
            })
        );
        // Test --name flag (for backwards compatibility)
        assert_parse!(
            ["agent", "create", "--name", "some_agent", "--from", "some_old_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Create {
                    name: None,
                    name_flag: Some("some_agent".to_string()),
                    directory: None,
                    from: Some("some_old_agent".to_string())
                })
            })
        );
        // Test -n flag (for backwards compatibility)
        assert_parse!(
            ["agent", "create", "-n", "some_agent", "--from", "some_old_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Create {
                    name: None,
                    name_flag: Some("some_agent".to_string()),
                    directory: None,
                    from: Some("some_old_agent".to_string())
                })
            })
        );
    }

    #[test]
    fn test_agent_subcommand_edit() {
        assert_parse!(
            ["agent", "edit", "existing_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Edit {
                    name: Some("existing_agent".to_string()),
                    name_flag: None,
                    path: None,
                })
            })
        );
        assert_parse!(
            ["agent", "edit", "--name", "existing_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Edit {
                    name: None,
                    name_flag: Some("existing_agent".to_string()),
                    path: None,
                })
            })
        );
        assert_parse!(
            ["agent", "edit", "-n", "existing_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Edit {
                    name: None,
                    name_flag: Some("existing_agent".to_string()),
                    path: None,
                })
            })
        );
        assert_parse!(
            ["agent", "edit", "--path", "/path/to/agent.json"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Edit {
                    name: None,
                    name_flag: None,
                    path: Some("/path/to/agent.json".to_string()),
                })
            })
        );
        // Test that both parameters can be provided
        assert_parse!(
            ["agent", "edit", "existing_agent", "--path", "/path/to/agent.json"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Edit {
                    name: Some("existing_agent".to_string()),
                    name_flag: None,
                    path: Some("/path/to/agent.json".to_string()),
                })
            })
        );
    }
}
