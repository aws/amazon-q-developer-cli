use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{
    Args,
    Subcommand,
};
use eyre::{
    Result,
    bail,
};

use super::{
    Agent,
    Agents,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories::{
    self,
    agent_config_dir,
};

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub enum AgentSubcommands {
    /// List the available agents. Note that local agents are only discovered if the command is
    /// invoked at a directory that contains them
    List,
    /// Renames a given agent to a new name
    Rename {
        /// Original name of the agent
        #[arg(long, short)]
        agent: String,
        /// New name the agent shall be changed to
        #[arg(long, short)]
        new_name: String,
    },
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
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    cmd: Option<AgentSubcommands>,
}

impl AgentArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        let mut stderr = std::io::stderr();
        let mut agents = Agents::load(os, None, true, &mut stderr).await;
        match self.cmd {
            Some(AgentSubcommands::List) | None => {
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
            Some(AgentSubcommands::Rename { agent, new_name }) => {
                rename_agent(os, &mut agents, agent.clone(), new_name.clone()).await?;
                writeln!(stderr, "\n✓ Renamed agent '{}' to '{}'\n", agent, new_name)?;
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
        let path = PathBuf::from(path);

        // If path points to a file, strip the filename to get the directory
        if !path.is_dir() {
            bail!("Path must be a directory");
        }

        let last_three_segments = agent_config_dir();
        if path.ends_with(&last_three_segments) {
            path
        } else {
            path.join(&last_three_segments)
        }
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
        let agent_to_copy = agents.switch(from.as_str())?;
        serde_json::to_string_pretty(agent_to_copy)?
    } else {
        Default::default()
    };
    let path_with_file_name = path.join(format!("{name}.json"));

    if !path.exists() {
        os.fs.create_dir_all(&path).await?;
    }
    os.fs.create_new(&path_with_file_name).await?;
    os.fs.write(&path_with_file_name, prepopulated_content).await?;

    Ok(path_with_file_name)
}

pub async fn rename_agent(os: &mut Os, agents: &mut Agents, agent: String, new_name: String) -> Result<()> {
    if agents.agents.iter().any(|(name, _)| name == &new_name) {
        bail!("New name {new_name} already exists in the current scope. Aborting");
    }

    match agents.switch(agent.as_str()) {
        Ok(target_agent) => {
            if let Some(path) = target_agent.path.as_ref() {
                let new_path = path
                    .parent()
                    .map(|p| p.join(format!("{new_name}.json")))
                    .ok_or(eyre::eyre!("Failed to retrieve parent directory of target config"))?;
                os.fs.rename(path, new_path).await?;

                if let Some(default_agent) = os.database.settings.get_string(Setting::ChatDefaultAgent) {
                    let global_agent_path = directories::chat_global_agent_path(os)?;
                    if default_agent == agent
                        && target_agent
                            .path
                            .as_ref()
                            .is_some_and(|p| p.parent().is_some_and(|p| p == global_agent_path))
                    {
                        os.database.settings.set(Setting::ChatDefaultAgent, new_name).await?;
                    }
                }
            } else {
                bail!("Target agent has no path associated. Aborting");
            }
        },
        Err(e) => {
            bail!(e);
        },
    }

    Ok(())
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

    #[test]
    fn test_agent_subcommand_rename() {
        assert_parse!(
            ["agent", "rename", "--agent", "old_name", "--new-name", "new_name"],
            RootSubcommand::Agent(AgentArgs {
                cmd: Some(AgentSubcommands::Rename {
                    agent: "old_name".to_string(),
                    new_name: "new_name".to_string(),
                })
            })
        );
    }
}
