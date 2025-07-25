use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{
    ArgAction,
    Args,
    ValueEnum,
};
use crossterm::{
    execute,
    style,
};
use eyre::{
    Result,
    bail,
};

use super::agent::{
    Agent,
    Agents,
    McpServerConfig,
};
use crate::cli::chat::tool_manager::{
    global_mcp_config_path,
    workspace_mcp_config_path,
};
use crate::cli::chat::tools::custom_tool::{
    CustomToolConfig,
    default_timeout,
};
use crate::os::Os;
use crate::util::directories;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    Workspace,
    Global,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Workspace => write!(f, "workspace"),
            Scope::Global => write!(f, "global"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, clap::Subcommand)]
pub enum McpSubcommand {
    /// Add or replace a configured server
    Add(AddArgs),
    /// Remove a server from the MCP configuration
    #[command(alias = "rm")]
    Remove(RemoveArgs),
    /// List configured servers
    List(ListArgs),
    /// Import a server configuration from another file
    Import(ImportArgs),
    /// Get the status of a configured server
    Status(StatusArgs),
}

impl McpSubcommand {
    pub async fn execute(self, os: &mut Os, output: &mut impl Write) -> Result<ExitCode> {
        match self {
            Self::Add(args) => args.execute(os, output).await?,
            Self::Remove(args) => args.execute(os, output).await?,
            Self::List(args) => args.execute(os, output).await?,
            Self::Import(args) => args.execute(os, output).await?,
            Self::Status(args) => args.execute(os, output).await?,
        }

        output.flush()?;
        Ok(ExitCode::SUCCESS)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct AddArgs {
    /// Name for the server
    #[arg(long)]
    pub name: String,
    /// The command used to launch the server
    #[arg(long)]
    pub command: String,
    /// Arguments to pass to the command
    #[arg(long, action = ArgAction::Append, allow_hyphen_values = true, value_delimiter = ',')]
    pub args: Vec<String>,
    /// Where to add the server to. If an agent name is not supplied, the changes shall be made to
    /// the global mcp.json
    #[arg(long)]
    pub agent: Option<String>,
    /// Environment variables to use when launching the server
    #[arg(long, value_parser = parse_env_vars)]
    pub env: Vec<HashMap<String, String>>,
    /// Server launch timeout, in milliseconds
    #[arg(long)]
    pub timeout: Option<u64>,
    /// Whether the server should be disabled (not loaded)
    #[arg(long, default_value_t = false)]
    pub disabled: bool,
    /// Overwrite an existing server with the same name
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

impl AddArgs {
    pub async fn execute(self, os: &Os, output: &mut impl Write) -> Result<()> {
        match self.agent.as_deref() {
            Some(agent_name) => {
                let (mut agent, config_path) = Agent::get_agent_by_name(os, agent_name).await?;
                let mcp_servers = &mut agent.mcp_servers.mcp_servers;

                if mcp_servers.contains_key(&self.name) && !self.force {
                    bail!(
                        "\nMCP server '{}' already exists in agent {} (path {}). Use --force to overwrite.",
                        self.name,
                        agent_name,
                        config_path.display(),
                    );
                }

                let merged_env = self.env.into_iter().flatten().collect::<HashMap<_, _>>();
                let tool: CustomToolConfig = serde_json::from_value(serde_json::json!({
                    "command": self.command,
                    "args": self.args,
                    "env": merged_env,
                    "timeout": self.timeout.unwrap_or(default_timeout()),
                    "disabled": self.disabled,
                }))?;

                mcp_servers.insert(self.name.clone(), tool);
                let json = agent.to_str_pretty()?;
                os.fs.write(config_path, json).await?;
                writeln!(output, "✓ Added MCP server '{}' to agent {}\n", self.name, agent_name)?;
            },
            None => {
                let global_config_path = directories::chat_legacy_mcp_config(os)?;
                let mut mcp_servers = McpServerConfig::load_from_file(os, &global_config_path).await?;

                if mcp_servers.mcp_servers.contains_key(&self.name) && !self.force {
                    bail!(
                        "\nMCP server '{}' already exists in global config (path {}). Use --force to overwrite.",
                        self.name,
                        &global_config_path.display(),
                    );
                }

                let merged_env = self.env.into_iter().flatten().collect::<HashMap<_, _>>();
                let tool: CustomToolConfig = serde_json::from_value(serde_json::json!({
                    "command": self.command,
                    "args": self.args,
                    "env": merged_env,
                    "timeout": self.timeout.unwrap_or(default_timeout()),
                    "disabled": self.disabled,
                }))?;

                mcp_servers.mcp_servers.insert(self.name.clone(), tool);
                mcp_servers.save_to_file(os, &global_config_path).await?;
                writeln!(
                    output,
                    "✓ Added MCP server '{}' to global config in {}\n",
                    self.name,
                    global_config_path.display()
                )?;
            },
        };

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct RemoveArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, value_enum)]
    pub agent: Option<String>,
}

impl RemoveArgs {
    pub async fn execute(self, os: &Os, output: &mut impl Write) -> Result<()> {
        match self.agent.as_deref() {
            Some(agent_name) => {
                let (mut agent, config_path) = Agent::get_agent_by_name(os, agent_name).await?;

                if !os.fs.exists(&config_path) {
                    writeln!(output, "\nNo MCP server configurations found.\n")?;
                    return Ok(());
                }

                let config = &mut agent.mcp_servers.mcp_servers;

                match config.remove(&self.name) {
                    Some(_) => {
                        let json = agent.to_str_pretty()?;
                        os.fs.write(config_path, json).await?;
                        writeln!(
                            output,
                            "\n✓ Removed MCP server '{}' from agent {}\n",
                            self.name, agent_name,
                        )?;
                    },
                    None => {
                        writeln!(
                            output,
                            "\nNo MCP server named '{}' found in agent {}\n",
                            self.name, agent_name,
                        )?;
                    },
                }
            },
            None => {
                let global_config_path = directories::chat_legacy_mcp_config(os)?;
                let mut config = McpServerConfig::load_from_file(os, &global_config_path).await?;

                match config.mcp_servers.remove(&self.name) {
                    Some(_) => {
                        config.save_to_file(os, &global_config_path).await?;
                        writeln!(
                            output,
                            "\n✓ Removed MCP server '{}' from global config (path {})\n",
                            self.name,
                            &global_config_path.display(),
                        )?;
                    },
                    None => {
                        writeln!(
                            output,
                            "\nNo MCP server named '{}' found in global config (path {})\n",
                            self.name,
                            &global_config_path.display(),
                        )?;
                    },
                }
            },
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ListArgs {
    #[arg(value_enum)]
    pub scope: Option<Scope>,
    #[arg(long, hide = true)]
    pub profile: Option<String>,
}

impl ListArgs {
    pub async fn execute(self, os: &mut Os, output: &mut impl Write) -> Result<()> {
        let configs = get_mcp_server_configs(os, self.scope).await?;
        if configs.is_empty() {
            writeln!(output, "No MCP server configurations found.\n")?;
            return Ok(());
        }

        for (scope, path, cfg_opt) in configs {
            writeln!(output)?;
            writeln!(output, "{}:\n  {}", scope_display(&scope), path.display())?;
            match cfg_opt {
                Some(cfg) if !cfg.mcp_servers.is_empty() => {
                    for (name, tool_cfg) in &cfg.mcp_servers {
                        let status = if tool_cfg.disabled { " (disabled)" } else { "" };
                        writeln!(output, "    • {name:<12} {}{}", tool_cfg.command, status)?;
                    }
                },
                _ => {
                    writeln!(output, "    (empty)")?;
                },
            }
        }
        writeln!(output, "\n")?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ImportArgs {
    #[arg(long)]
    pub file: String,
    #[arg(value_enum)]
    pub scope: Option<Scope>,
    /// Overwrite an existing server with the same name
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

impl ImportArgs {
    pub async fn execute(self, os: &Os, output: &mut impl Write) -> Result<()> {
        let scope: Scope = self.scope.unwrap_or(Scope::Workspace);
        let config_path = resolve_scope_profile(os, self.scope)?;
        let mut dst_cfg = ensure_config_file(os, &config_path, output).await?;

        let src_path = expand_path(os, &self.file)?;
        let src_cfg: McpServerConfig = McpServerConfig::load_from_file(os, &src_path).await?;

        let mut added = 0;
        for (name, cfg) in src_cfg.mcp_servers {
            if dst_cfg.mcp_servers.contains_key(&name) && !self.force {
                bail!(
                    "\nMCP server '{}' already exists in {} (scope {}). Use --force to overwrite.\n",
                    name,
                    config_path.display(),
                    scope
                );
            }
            dst_cfg.mcp_servers.insert(name.clone(), cfg);
            added += 1;
        }

        writeln!(
            output,
            "\nTo learn more about MCP safety, see https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp-security.html\n\n"
        )?;

        dst_cfg.save_to_file(os, &config_path).await?;
        writeln!(
            output,
            "✓ Imported {added} MCP server(s) into {}\n",
            scope_display(&scope)
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub name: String,
}

impl StatusArgs {
    pub async fn execute(self, os: &mut Os, output: &mut impl Write) -> Result<()> {
        let configs = get_mcp_server_configs(os, None).await?;
        let mut found = false;

        for (sc, path, cfg_opt) in configs {
            if let Some(cfg) = cfg_opt.and_then(|c| c.mcp_servers.get(&self.name).cloned()) {
                found = true;
                execute!(
                    output,
                    style::Print("\n─────────────\n"),
                    style::Print(format!("Scope   : {}\n", scope_display(&sc))),
                    style::Print(format!("File    : {}\n", path.display())),
                    style::Print(format!("Command : {}\n", cfg.command)),
                    style::Print(format!("Timeout : {} ms\n", cfg.timeout)),
                    style::Print(format!("Disabled: {}\n", cfg.disabled)),
                    style::Print(format!(
                        "Env Vars: {}\n",
                        cfg.env
                            .as_ref()
                            .map_or_else(|| "(none)".into(), |e| e.keys().cloned().collect::<Vec<_>>().join(", "))
                    )),
                )?;
            }
        }
        writeln!(output, "\n")?;

        if !found {
            bail!("No MCP server named '{}' found in any scope/profile\n", self.name);
        }

        Ok(())
    }
}

async fn get_mcp_server_configs(
    os: &mut Os,
    scope: Option<Scope>,
) -> Result<Vec<(Scope, PathBuf, Option<McpServerConfig>)>> {
    let mut targets = Vec::new();
    match scope {
        Some(scope) => targets.push(scope),
        None => targets.extend([Scope::Workspace, Scope::Global]),
    }

    let mut results = Vec::new();
    let mut stderr = std::io::stderr();
    let agents = Agents::load(os, None, true, &mut stderr).await.0;
    let global_path = directories::chat_global_agent_path(os)?;
    for (_, agent) in agents.agents {
        let scope = if agent
            .path
            .as_ref()
            .is_some_and(|p| p.parent().is_some_and(|p| p == global_path))
        {
            Scope::Global
        } else {
            Scope::Workspace
        };

        results.push((
            scope,
            agent.path.ok_or(eyre::eyre!("Agent missing path info"))?,
            Some(agent.mcp_servers),
        ));
    }
    Ok(results)
}

fn scope_display(scope: &Scope) -> String {
    match scope {
        Scope::Workspace => "📄 workspace".into(),
        Scope::Global => "🌍 global".into(),
    }
}

fn resolve_scope_profile(os: &Os, scope: Option<Scope>) -> Result<PathBuf> {
    Ok(match scope {
        Some(Scope::Global) => global_mcp_config_path(os)?,
        _ => workspace_mcp_config_path(os)?,
    })
}

fn expand_path(os: &Os, p: &str) -> Result<PathBuf> {
    let p = shellexpand::tilde(p);
    let mut path = PathBuf::from(p.as_ref() as &str);
    if path.is_relative() {
        path = os.env.current_dir()?.join(path);
    }
    Ok(path)
}

async fn ensure_config_file(os: &Os, path: &PathBuf, output: &mut impl Write) -> Result<McpServerConfig> {
    if !os.fs.exists(path) {
        if let Some(parent) = path.parent() {
            os.fs.create_dir_all(parent).await?;
        }
        McpServerConfig::default().save_to_file(os, path).await?;
        writeln!(output, "\n📁 Created MCP config in '{}'", path.display())?;
    }

    load_cfg(os, path).await
}

fn parse_env_vars(arg: &str) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    for pair in arg.split(",") {
        match pair.split_once('=') {
            Some((key, value)) => {
                vars.insert(key.trim().to_string(), value.trim().to_string());
            },
            None => {
                bail!(
                    "Failed to parse environment variables, invalid environment variable '{}'. Expected 'name=value'",
                    pair
                )
            },
        }
    }

    Ok(vars)
}

async fn load_cfg(os: &Os, p: &PathBuf) -> Result<McpServerConfig> {
    Ok(if os.fs.exists(p) {
        McpServerConfig::load_from_file(os, p).await?
    } else {
        McpServerConfig::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RootSubcommand;
    use crate::util::test::assert_parse;

    #[tokio::test]
    async fn test_scope_and_profile_defaults_to_workspace() {
        let os = Os::new().await.unwrap();
        let path = resolve_scope_profile(&os, None).unwrap();
        assert_eq!(
            path.to_str(),
            workspace_mcp_config_path(&os).unwrap().to_str(),
            "No scope or profile should default to the workspace path"
        );
    }

    #[tokio::test]
    async fn test_resolve_paths() {
        let os = Os::new().await.unwrap();
        // workspace
        let p = resolve_scope_profile(&os, Some(Scope::Workspace)).unwrap();
        assert_eq!(p, workspace_mcp_config_path(&os).unwrap());

        // global
        let p = resolve_scope_profile(&os, Some(Scope::Global)).unwrap();
        assert_eq!(p, global_mcp_config_path(&os).unwrap());
    }

    #[ignore = "TODO: fix in CI"]
    #[tokio::test]
    async fn ensure_file_created_and_loaded() {
        let os = Os::new().await.unwrap();
        let path = workspace_mcp_config_path(&os).unwrap();

        let cfg = super::ensure_config_file(&os, &path, &mut vec![]).await.unwrap();
        assert!(path.exists(), "config file should be created");
        assert!(cfg.mcp_servers.is_empty());
    }

    #[ignore = "TODO: fix in CI"]
    #[tokio::test]
    async fn add_then_remove_cycle() {
        let os = Os::new().await.unwrap();

        // 1. add
        AddArgs {
            name: "local".into(),
            command: "echo hi".into(),
            args: vec![
                "awslabs.eks-mcp-server".to_string(),
                "--allow-write".to_string(),
                "--allow-sensitive-data-access".to_string(),
            ],
            env: vec![],
            timeout: None,
            agent: None,
            disabled: false,
            force: false,
        }
        .execute(&os, &mut vec![])
        .await
        .unwrap();

        let cfg_path = workspace_mcp_config_path(&os).unwrap();
        let cfg: McpServerConfig =
            serde_json::from_str(&os.fs.read_to_string(cfg_path.clone()).await.unwrap()).unwrap();
        assert!(cfg.mcp_servers.len() == 1);

        // 2. remove
        RemoveArgs {
            name: "local".into(),
            agent: None,
        }
        .execute(&os, &mut vec![])
        .await
        .unwrap();

        let cfg: McpServerConfig = serde_json::from_str(&os.fs.read_to_string(cfg_path).await.unwrap()).unwrap();
        assert!(cfg.mcp_servers.is_empty());
    }

    #[test]
    fn test_mcp_subcomman_add() {
        assert_parse!(
            [
                "mcp",
                "add",
                "--name",
                "test_server",
                "--command",
                "test_command",
                "--args",
                "awslabs.eks-mcp-server,--allow-write,--allow-sensitive-data-access",
                "--env",
                "key1=value1,key2=value2"
            ],
            RootSubcommand::Mcp(McpSubcommand::Add(AddArgs {
                name: "test_server".to_string(),
                command: "test_command".to_string(),
                args: vec![
                    "awslabs.eks-mcp-server".to_string(),
                    "--allow-write".to_string(),
                    "--allow-sensitive-data-access".to_string(),
                ],
                agent: None,
                env: vec![
                    [
                        ("key1".to_string(), "value1".to_string()),
                        ("key2".to_string(), "value2".to_string())
                    ]
                    .into_iter()
                    .collect()
                ],
                timeout: None,
                disabled: false,
                force: false,
            }))
        );
    }

    #[test]
    fn test_mcp_subcomman_remove_workspace() {
        assert_parse!(
            ["mcp", "remove", "--name", "old"],
            RootSubcommand::Mcp(McpSubcommand::Remove(RemoveArgs {
                name: "old".into(),
                agent: None,
            }))
        );
    }

    #[test]
    fn test_mcp_subcomman_import_profile_force() {
        assert_parse!(
            ["mcp", "import", "--file", "servers.json", "--force"],
            RootSubcommand::Mcp(McpSubcommand::Import(ImportArgs {
                file: "servers.json".into(),
                scope: None,
                force: true,
            }))
        );
    }

    #[test]
    fn test_mcp_subcommand_status_simple() {
        assert_parse!(
            ["mcp", "status", "--name", "aws"],
            RootSubcommand::Mcp(McpSubcommand::Status(StatusArgs { name: "aws".into() }))
        );
    }

    #[test]
    fn test_mcp_subcommand_list() {
        assert_parse!(
            ["mcp", "list", "global"],
            RootSubcommand::Mcp(McpSubcommand::List(ListArgs {
                scope: Some(Scope::Global),
                profile: None
            }))
        );
    }
}
