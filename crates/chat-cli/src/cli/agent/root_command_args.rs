use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{
    Args,
    Subcommand,
};
use crossterm::style::Color;
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
    Agents,
    McpServerConfig,
    legacy,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories;

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub enum AgentSubcommands {
    /// List the available agents. Note that local agents are only discovered if the command is
    /// invoked at a directory that contains them
    List,
    /// Create an agent config. If path is not provided, Q CLI shall create this config in the
    /// global agent directory
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
        #[arg(long, short)]
        name: String,
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
        match self.cmd {
            Some(AgentSubcommands::List) | None => {
                let agents = Agents::load(os, None, true, &mut stderr).await.0;
                let agent_with_path =
                    agents
                        .agents
                        .into_iter()
                        .fold(Vec::<(String, String)>::new(), |mut acc, (name, agent)| {
                            acc.push((
                                name,
                                agent
                                    .path
                                    .and_then(|p| p.parent().map(|p| p.to_string_lossy().to_string()))
                                    .unwrap_or("**No path found**".to_string()),
                            ));
                            acc
                        });
                let max_name_length = agent_with_path.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
                let output_str = agent_with_path
                    .into_iter()
                    .map(|(name, path)| format!("{name:<width$}    {path}", width = max_name_length))
                    .collect::<Vec<_>>()
                    .join("\n");

                writeln!(stderr, "{}", output_str)?;
            },
            Some(AgentSubcommands::Create { name, directory, from }) => {
                let mut agents = Agents::load(os, None, true, &mut stderr).await.0;
                let path_with_file_name = create_agent(os, &mut agents, name.clone(), directory, from).await?;
                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let mut cmd = std::process::Command::new(editor_cmd);

                let status = cmd.arg(&path_with_file_name).status()?;
                if !status.success() {
                    bail!("Editor process did not exit with success");
                }

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
            Some(AgentSubcommands::Validate { path }) => {
                let mut global_mcp_config = None::<McpServerConfig>;
                let agent = Agent::load(os, path.as_str(), &mut global_mcp_config).await;

                'validate: {
                    match agent {
                        Ok(agent) => {
                            let Ok(instance) = serde_json::to_value(&agent) else {
                                queue!(
                                    stderr,
                                    style::SetForegroundColor(style::Color::Red),
                                    style::Print("Error: "),
                                    style::ResetColor,
                                    style::Print("failed to obtain value from agent provided. Aborting validation"),
                                )?;
                                break 'validate;
                            };

                            let schema = match serde_json::to_value(schema_for!(Agent)) {
                                Ok(schema) => schema,
                                Err(e) => {
                                    queue!(
                                        stderr,
                                        style::SetForegroundColor(style::Color::Red),
                                        style::Print("Error: "),
                                        style::ResetColor,
                                        style::Print(format!("failed to obtain schema: {e}. Aborting validation"))
                                    )?;
                                    break 'validate;
                                },
                            };

                            if let Err(e) = jsonschema::validate(&schema, &instance).map_err(|e| e.to_owned()) {
                                let name = &agent.name;
                                queue!(
                                    stderr,
                                    style::SetForegroundColor(Color::Yellow),
                                    style::Print("WARNING "),
                                    style::ResetColor,
                                    style::Print("Agent config "),
                                    style::SetForegroundColor(Color::Green),
                                    style::Print(name),
                                    style::ResetColor,
                                    style::Print(" is malformed at "),
                                    style::SetForegroundColor(Color::Yellow),
                                    style::Print(&e.instance_path),
                                    style::ResetColor,
                                    style::Print(format!(": {e}\n")),
                                )?;
                            }
                        },
                        Err(e) => {
                            let _ = queue!(
                                stderr,
                                style::SetForegroundColor(Color::Red),
                                style::Print("Error: "),
                                style::ResetColor,
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
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("WARNING: "),
                        style::ResetColor,
                        style::Print(
                            "manual migrate is potentially destructive to existing agent configs with name collision. Use"
                        ),
                        style::SetForegroundColor(Color::Cyan),
                        style::Print(" --force "),
                        style::ResetColor,
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
                            style::SetForegroundColor(Color::Green),
                            style::Print("✓ Success: "),
                            style::ResetColor,
                            style::Print(format!(
                                "Profile migration successful. Migrated {} agent(s)\n",
                                migrated_count
                            )),
                        );
                    },
                    Ok(None) => {
                        let _ = queue!(
                            stderr,
                            style::SetForegroundColor(Color::Blue),
                            style::Print("Info: "),
                            style::ResetColor,
                            style::Print("Migration was not performed. Nothing to migrate\n"),
                        );
                    },
                    Err(e) => {
                        let _ = queue!(
                            stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(format!("Migration did not happen for the following reason: {e}\n")),
                        );
                    },
                }
            },
            Some(AgentSubcommands::SetDefault { name }) => {
                let mut agents = Agents::load(os, None, true, &mut stderr).await.0;
                match agents.switch(&name) {
                    Ok(agent) => {
                        os.database
                            .settings
                            .set(Setting::ChatDefaultAgent, agent.name.clone())
                            .await?;

                        let _ = queue!(
                            stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("✓ Default agent set to '"),
                            style::Print(&agent.name),
                            style::Print("'. This will take effect the next time q chat is launched.\n"),
                            style::ResetColor,
                        );
                    },
                    Err(e) => {
                        let _ = queue!(
                            stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
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

        directories::agent_config_dir(path)?
    } else {
        directories::chat_global_agent_path(os)?
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
        let mut agent_to_copy = agents.switch(from.as_str())?.clone();
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
        assert_parse!(
            ["agent", "create", "--name", "some_agent", "--from", "some_old_agent"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Create {
                    name: "some_agent".to_string(),
                    directory: None,
                    from: Some("some_old_agent".to_string())
                })
            })
        );
    }
}
