use std::collections::{
    BTreeMap,
    HashMap,
};
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
use crate::cli::chat::legacy::custom_tool::{
    CustomToolConfig,
    default_timeout,
};
use crate::cli::chat::legacy::{
    global_mcp_config_path,
    workspace_mcp_config_path,
};
use crate::constants::{
    CLI_NAME,
    DEFAULT_AGENT_NAME,
    MCP_SECURITY_DOC_URL,
};

/// Truncate server description to fit in terminal display
fn truncate_server_description(description: &str) -> String {
    if description.is_empty() {
        "(no description)".to_string()
    } else if description.len() <= 50 {
        description.to_string()
    } else {
        // Find a good break point (word boundary) within the limit
        let truncated = &description[..50];
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &description[..last_space])
        } else {
            format!("{truncated}...")
        }
    }
}
use crate::database::settings::Setting;
use crate::mcp_registry::{
    McpRegistryClient,
    McpRegistryResponse,
};
use crate::os::Os;
use crate::util::paths::PathResolver;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Hash)]
pub enum Scope {
    Default,
    Workspace,
    Global,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Default => write!(f, "default"),
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

// Internal struct used by AddArgs when delegating to registry functionality
#[derive(Debug, Clone, PartialEq, Eq, Args)]
struct RegistryAddArgs {
    /// Agent to add server to (defaults to user's configured default agent)
    #[arg(long)]
    pub agent: Option<String>,

    /// Scope for legacy config (workspace/global)
    #[arg(long)]
    pub scope: Option<Scope>,

    /// Specific server name (non-interactive)
    #[arg(long)]
    pub server: Option<String>,
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
    /// Name for the server (optional when registry is configured - shows interactive menu)
    #[arg(long)]
    pub name: Option<String>,
    /// Scope. This parameter is only meaningful in the absence of agent name.
    #[arg(long)]
    pub scope: Option<Scope>,
    /// The command used to launch the server (required for stdio servers, not needed for registry
    /// or HTTP servers)
    #[arg(long)]
    pub command: Option<String>,
    /// The URL for HTTP-based MCP server communication (required for HTTP servers)
    #[arg(long)]
    pub url: Option<String>,
    /// Arguments to pass to the command. Can be provided as:
    /// 1. Multiple --args flags: --args arg1 --args arg2 --args "arg,with,commas"
    /// 2. Comma-separated with escaping: --args "arg1,arg2,arg\,with\,commas"
    /// 3. JSON array format: --args '["arg1", "arg2", "arg,with,commas"]'
    #[arg(long, action = ArgAction::Append, allow_hyphen_values = true)]
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
        // For non-enterprise users, skip registry check and allow custom servers
        let is_enterprise = crate::auth::builder_id::is_idc_user(&os.database).await;

        if !is_enterprise {
            // Non-enterprise user - allow custom servers without registry check
            if self.name.is_none() {
                writeln!(output, "❌ --name is required when adding custom servers.")?;
                writeln!(
                    output,
                    "Example: {CLI_NAME} mcp add --name my-server --command 'python server.py'"
                )?;
                writeln!(
                    output,
                    "     or: {CLI_NAME} mcp add --name my-server --url 'http://localhost:3000'\n"
                )?;
                return Ok(());
            }
            if self.command.is_none() && self.url.is_none() {
                writeln!(
                    output,
                    "❌ Either --command or --url is required when adding custom servers."
                )?;
                writeln!(
                    output,
                    "Example: {CLI_NAME} mcp add --name my-server --command 'python server.py'"
                )?;
                writeln!(
                    output,
                    "     or: {CLI_NAME} mcp add --name my-server --url 'http://localhost:3000'\n"
                )?;
                return Ok(());
            }
            if self.command.is_some() && self.url.is_some() {
                writeln!(
                    output,
                    "❌ Cannot specify both --command and --url. Use one or the other.\n"
                )?;
                return Ok(());
            }
            // Continue with custom server addition below
        } else {
            // Enterprise user - check if MCP registry is configured
            match os.client.get_mcp_config().await {
                Ok((_, Some(registry_url))) => {
                    // Registry is configured - verify connectivity before blocking custom servers
                    let registry_client = McpRegistryClient::new();
                    match registry_client.fetch_registry(&registry_url).await {
                        Ok(registry) => {
                            // Registry is reachable - delegate to registry add functionality

                            // Check if user provided command in registry mode
                            if self.command.is_some() {
                                show_registry_only_error(output)?;
                                return Ok(());
                            }

                            // If user provided a server name, validate it exists in the registry
                            if let Some(ref name) = self.name
                                && registry.get_server(name).is_none()
                            {
                                show_registry_only_error(output)?;
                                return Ok(());
                            }

                            let registry_args = RegistryAddArgs {
                                agent: self.agent,
                                scope: self.scope,
                                server: self.name,
                            };
                            return registry_args.execute(os, output, registry_url).await;
                        },
                        Err(e) => {
                            // Registry is unavailable - block all server operations
                            writeln!(output, "❌ Failed to fetch registry data: {e}")?;
                            let error_type = crate::mcp_registry::RegistryErrorType::from_error(&e);
                            match error_type {
                                crate::mcp_registry::RegistryErrorType::NetworkConnectivity => {
                                    writeln!(output, "Check your network connection and try again.\n")?;
                                },
                                crate::mcp_registry::RegistryErrorType::RegistryData => {
                                    writeln!(output, "Registry contains invalid data. Contact your administrator.\n")?;
                                },
                            }
                            return Ok(());
                        },
                    }
                },
                Ok((_, None)) => {
                    // Registry is not configured - allow custom servers
                    // Validate that both name and (command or url) are provided for custom servers
                    if self.name.is_none() {
                        writeln!(output, "❌ --name is required when adding custom servers.")?;
                        writeln!(
                            output,
                            "Example: {CLI_NAME} mcp add --name my-server --command 'python server.py'"
                        )?;
                        writeln!(
                            output,
                            "     or: {CLI_NAME} mcp add --name my-server --url 'http://localhost:3000'\n"
                        )?;
                        return Ok(());
                    }
                    if self.command.is_none() && self.url.is_none() {
                        writeln!(
                            output,
                            "❌ Either --command or --url is required when adding custom servers."
                        )?;
                        writeln!(
                            output,
                            "Example: {CLI_NAME} mcp add --name my-server --command 'python server.py'"
                        )?;
                        writeln!(
                            output,
                            "     or: {CLI_NAME} mcp add --name my-server --url 'http://localhost:3000'\n"
                        )?;
                        return Ok(());
                    }
                    if self.command.is_some() && self.url.is_some() {
                        writeln!(
                            output,
                            "❌ Cannot specify both --command and --url. Use one or the other.\n"
                        )?;
                        return Ok(());
                    }
                },
                Err(e) => {
                    // Failed to check registry config - be conservative and block operations
                    writeln!(output, "❌ Failed to check MCP configuration: {e}")?;
                    writeln!(output, "🔒 MCP server operations are currently unavailable.")?;
                    writeln!(output, "   Please try again later.\n")?;
                    return Ok(());
                },
            }
        }

        // Process args to handle comma-separated values, escaping, and JSON arrays
        let processed_args = self.process_args()?;

        // Extract name for custom server operations (guaranteed to be Some at this point)
        let server_name = self.name.as_ref().unwrap();

        match self.agent.as_deref() {
            Some(agent_name) => {
                let (mut agent, config_path) = Agent::get_agent_by_name(os, agent_name).await?;
                let mcp_servers = &mut agent.mcp_servers.mcp_servers;

                if mcp_servers.contains_key(server_name) && !self.force {
                    bail!(
                        "\nMCP server '{}' already exists in agent {} (path {}). Use --force to overwrite.",
                        server_name,
                        agent_name,
                        config_path.display(),
                    );
                }

                let merged_env = self.env.into_iter().flatten().collect::<HashMap<_, _>>();
                let tool: CustomToolConfig = if let Some(url) = self.url {
                    serde_json::from_value(serde_json::json!({
                        "url": url,
                        "timeout": self.timeout.unwrap_or(default_timeout()),
                        "disabled": self.disabled,
                    }))?
                } else {
                    serde_json::from_value(serde_json::json!({
                        "command": self.command.unwrap(),
                        "args": processed_args,
                        "env": merged_env,
                        "timeout": self.timeout.unwrap_or(default_timeout()),
                        "disabled": self.disabled,
                    }))?
                };

                mcp_servers.insert(server_name.clone(), tool);
                let json = agent.to_str_pretty()?;
                os.fs.write(config_path, json).await?;
                writeln!(output, "✓ Added MCP server '{server_name}' to agent {agent_name}\n")?;
            },
            None => {
                let resolver = PathResolver::new(os);
                let scope = self.scope.unwrap_or(Scope::Workspace);
                let legacy_mcp_config_path = match scope {
                    Scope::Workspace => resolver.workspace().mcp_config()?,
                    _ => resolver.global().mcp_config()?,
                };
                if !legacy_mcp_config_path.exists() {
                    // Ensure parent directory exists
                    if let Some(parent) = legacy_mcp_config_path.parent() {
                        os.fs.create_dir_all(parent).await?;
                    }
                    // Create an empty config file that won't fail to deserialize.
                    os.fs.write(&legacy_mcp_config_path, "{ \"mcpServers\": {} }").await?;
                }
                let mut mcp_servers = McpServerConfig::load_from_file(os, &legacy_mcp_config_path).await?;

                if mcp_servers.mcp_servers.contains_key(server_name) && !self.force {
                    bail!(
                        "\nMCP server '{}' already exists in {} config (path {}). Use --force to overwrite.",
                        server_name,
                        scope,
                        &legacy_mcp_config_path.display(),
                    );
                }

                let merged_env = self.env.into_iter().flatten().collect::<HashMap<_, _>>();
                let tool: CustomToolConfig = if let Some(url) = self.url {
                    serde_json::from_value(serde_json::json!({
                        "url": url,
                        "timeout": self.timeout.unwrap_or(default_timeout()),
                        "disabled": self.disabled,
                    }))?
                } else {
                    serde_json::from_value(serde_json::json!({
                        "command": self.command.unwrap(),
                        "args": processed_args,
                        "env": merged_env,
                        "timeout": self.timeout.unwrap_or(default_timeout()),
                        "disabled": self.disabled,
                    }))?
                };

                mcp_servers.mcp_servers.insert(server_name.clone(), tool);
                mcp_servers.save_to_file(os, &legacy_mcp_config_path).await?;
                writeln!(
                    output,
                    "✓ Added MCP server '{}' to {} config in {}\n",
                    server_name,
                    scope,
                    legacy_mcp_config_path.display()
                )?;
            },
        };

        Ok(())
    }

    fn process_args(&self) -> Result<Vec<String>> {
        let mut processed_args = Vec::new();

        for arg in &self.args {
            let parsed = parse_args(arg)?;
            processed_args.extend(parsed);
        }

        Ok(processed_args)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct RemoveArgs {
    #[arg(long)]
    pub name: String,
    /// Scope. This parameter is only meaningful in the absence of agent name.
    #[arg(long)]
    pub scope: Option<Scope>,
    #[arg(long, value_enum)]
    pub agent: Option<String>,
}

impl RemoveArgs {
    pub async fn execute(self, os: &Os, output: &mut impl Write) -> Result<()> {
        // Remove works the same regardless of registry configuration

        match self.agent.as_deref() {
            Some(agent_name) => {
                let (mut agent, config_path) = Agent::get_agent_by_name(os, agent_name).await?;

                if !os.fs.exists(&config_path) {
                    writeln!(output, "\nNo MCP server configurations found.\n")?;
                    return Ok(());
                }

                let config = &mut agent.mcp_servers.mcp_servers;

                // Check if server exists and if it's from legacy config
                if let Some(server_config) = config.get(&self.name)
                    && server_config.is_from_legacy_mcp_json
                {
                    writeln!(
                        output,
                        "⚠ Server '{}' is from legacy mcp.json and cannot be removed from agent config.",
                        self.name
                    )?;
                    writeln!(
                        output,
                        "   To remove it, use: {CLI_NAME} mcp remove --name {} --scope workspace",
                        self.name
                    )?;
                    return Ok(());
                }

                match config.remove(&self.name) {
                    Some(_) => {
                        let json = agent.to_str_pretty()?;
                        os.fs.write(config_path, json).await?;
                        writeln!(output, "✓ Removed MCP server '{}' from agent {agent_name}", self.name)?;
                    },
                    None => {
                        writeln!(output, "⚠ Server '{}' not found in agent '{agent_name}'", self.name)?;
                    },
                }
            },
            None => {
                let resolver = PathResolver::new(os);
                let scope = self.scope.unwrap_or(Scope::Workspace);
                let legacy_mcp_config_path = match scope {
                    Scope::Workspace => resolver.workspace().mcp_config()?,
                    _ => resolver.global().mcp_config()?,
                };
                let mut config = McpServerConfig::load_from_file(os, &legacy_mcp_config_path).await?;

                match config.mcp_servers.remove(&self.name) {
                    Some(_) => {
                        config.save_to_file(os, &legacy_mcp_config_path).await?;
                        writeln!(output, "✓ Removed MCP server '{}' from {scope} config", self.name)?;
                    },
                    None => {
                        writeln!(output, "⚠ Server '{}' not found in {scope} config", self.name)?;
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
}

impl ListArgs {
    pub async fn execute(self, os: &mut Os, output: &mut impl Write) -> Result<()> {
        // Check if registry mode is enabled
        // For non-enterprise users, skip the API call and default to enabled with no registry
        let is_enterprise = crate::auth::builder_id::is_idc_user(&os.database).await;
        let (mcp_enabled, registry_url) = if !is_enterprise {
            (true, None)
        } else {
            match os.client.get_mcp_config().await {
                Ok((enabled, url)) => (enabled, url),
                Err(_) => (false, None),
            }
        };

        let is_registry_mode = mcp_enabled && registry_url.is_some();

        let mut configs = get_mcp_server_configs(os).await?;
        configs.retain(|k, _| self.scope.is_none_or(|s| s == *k));
        if configs.is_empty() {
            writeln!(output, "No MCP server configurations found.\n")?;
            return Ok(());
        }

        // Fetch registry data if in registry mode
        let registry_data = if is_registry_mode {
            match registry_url {
                Some(url) => {
                    let registry_client = crate::mcp_registry::McpRegistryClient::new();
                    match registry_client.fetch_registry(&url).await {
                        Ok(registry) => Some(registry),
                        Err(e) => {
                            writeln!(output, "❌ Failed to fetch registry data: {e}")?;
                            let error_type = crate::mcp_registry::RegistryErrorType::from_error(&e);
                            match error_type {
                                crate::mcp_registry::RegistryErrorType::NetworkConnectivity => {
                                    writeln!(output, "Check your network connection and try again.\n")?;
                                },
                                crate::mcp_registry::RegistryErrorType::RegistryData => {
                                    writeln!(output, "Registry contains invalid data. Contact your administrator.\n")?;
                                },
                            }
                            return Ok(());
                        },
                    }
                },
                None => None,
            }
        } else {
            None
        };

        for (scope, agents) in configs {
            if let Some(s) = self.scope
                && scope != s
            {
                continue;
            }
            writeln!(output)?;
            writeln!(output, "{}:\n", scope_display(&scope))?;
            for (agent_name, cfg_opt, _) in agents {
                writeln!(output, "  {}", &agent_name)?;
                match cfg_opt {
                    Some(cfg) if !cfg.mcp_servers.is_empty() => {
                        self.display_agent_servers(&cfg, &registry_data, output).await?;
                    },
                    _ => {
                        writeln!(output, "    (empty)")?;
                    },
                }
            }
        }

        // Add legend if registry mode is active
        if registry_data.is_some() {
            writeln!(output, "Legend:")?;
            writeln!(output, "  ✓ Active server (loaded from registry)")?;
            writeln!(output, "  ⚠ Ignored server (not in registry)")?;
            writeln!(output, "  [legacy] Server from legacy mcp.json")?;
        }
        writeln!(output, "\n")?;

        Ok(())
    }

    async fn display_agent_servers(
        &self,
        cfg: &McpServerConfig,
        registry_data: &Option<crate::mcp_registry::McpRegistryResponse>,
        output: &mut impl Write,
    ) -> Result<()> {
        // Sort servers by name for consistent display
        let mut servers = cfg.mcp_servers.iter().collect::<Vec<_>>();
        servers.sort_by(|a, b| a.0.cmp(b.0));

        if let Some(registry) = registry_data {
            // Registry mode: show active and ignored servers separately
            let result = crate::mcp_registry::process_mcp_servers(&cfg.mcp_servers, Some(registry))?;

            // Show active servers
            for (name, _tool_cfg) in &servers {
                if result.servers.contains_key(*name) {
                    let config = &result.servers[*name];
                    let status = if config.disabled { " (disabled)" } else { "" };
                    let source = if config.is_from_legacy_mcp_json {
                        " [legacy]"
                    } else {
                        ""
                    };
                    writeln!(output, "    ✓ {name:<12} {}{}{}", config.command, status, source)?;
                }
            }

            // Show ignored servers
            if !result.ignored_servers.is_empty() {
                writeln!(output, "    ")?;
                writeln!(output, "    Ignored (not in registry):")?;
                for ignored_name in &result.ignored_servers {
                    if let Some((_name, tool_cfg)) = servers.iter().find(|(n, _)| *n == ignored_name) {
                        let status = if tool_cfg.disabled { " (disabled)" } else { "" };
                        let source = if tool_cfg.is_from_legacy_mcp_json {
                            " [legacy]"
                        } else {
                            ""
                        };
                        writeln!(
                            output,
                            "    ⚠ {ignored_name:<12} {}{}{}",
                            tool_cfg.command, status, source
                        )?;
                    }
                }
            }
        } else {
            // Non-registry mode: show all servers as before
            for (name, tool_cfg) in &servers {
                let status = if tool_cfg.disabled { " (disabled)" } else { "" };
                let source = if tool_cfg.is_from_legacy_mcp_json {
                    " [legacy]"
                } else {
                    ""
                };
                writeln!(output, "    • {name:<12} {}{}{}", tool_cfg.command, status, source)?;
            }
        }

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
            "\nTo learn more about MCP safety, see {MCP_SECURITY_DOC_URL}\n\n"
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
        let configs = get_mcp_server_configs(os).await?;
        let mut found = false;

        for (sc, agents) in configs {
            for (name, cfg_opt, _) in agents {
                if let Some(cfg) = cfg_opt.and_then(|c| c.mcp_servers.get(&self.name).cloned()) {
                    found = true;
                    execute!(
                        output,
                        style::Print("\n─────────────\n"),
                        style::Print(format!("Scope   : {}\n", scope_display(&sc))),
                        style::Print(format!("Agent   : {name}\n")),
                        style::Print(format!("Command : {}\n", cfg.command)),
                        style::Print(format!("Timeout : {} ms\n", cfg.timeout)),
                        style::Print(format!("Disabled: {}\n", cfg.disabled)),
                        style::Print(format!(
                            "Env Vars: {}\n",
                            cfg.env.map_or_else(
                                || "(none)".into(),
                                |e| e.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
                            )
                        )),
                    )?;
                }
            }
            writeln!(output, "\n")?;
        }

        if !found {
            bail!("No MCP server named '{}' found in any agent\n", self.name);
        }

        Ok(())
    }
}

/// Returns a [BTreeMap] for consistent key iteration.
async fn get_mcp_server_configs(os: &mut Os) -> Result<BTreeMap<Scope, Vec<(String, Option<McpServerConfig>, bool)>>> {
    let mut results = BTreeMap::new();
    let mut stderr = std::io::stderr();

    // For non-enterprise users, skip the API call and default to enabled
    let is_enterprise = crate::auth::builder_id::is_idc_user(&os.database).await;
    let (mcp_enabled, mcp_api_failure) = if !is_enterprise {
        (true, false)
    } else {
        match os.client.is_mcp_enabled().await {
            Ok(enabled) => (enabled, false),
            Err(err) => {
                // Check if this is a GetProfile API error
                let is_api_failure = matches!(err, crate::api_client::ApiClientError::GetProfileError(_));
                tracing::warn!(?err, "Failed to check MCP configuration, defaulting to disabled");
                (false, is_api_failure)
            },
        }
    };
    let agents = Agents::load(os, None, true, &mut stderr, mcp_enabled, mcp_api_failure)
        .await
        .0;
    let global_path = PathResolver::new(os).global().agents_dir()?;
    for (_, agent) in agents.agents {
        let scope = if agent
            .path
            .as_ref()
            .is_some_and(|p| p.parent().is_some_and(|p| p == global_path))
        {
            Scope::Global
        } else if agent.name == DEFAULT_AGENT_NAME {
            Scope::Default
        } else {
            Scope::Workspace
        };
        results
            .entry(scope)
            .or_insert(Vec::new())
            .push((agent.name, Some(agent.mcp_servers), agent.include_mcp_json));
    }

    for agents in results.values_mut() {
        agents.sort_by(|a, b| a.0.cmp(&b.0));
    }

    Ok(results)
}

fn scope_display(scope: &Scope) -> String {
    match scope {
        Scope::Default => "🤖 default".into(),
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

fn parse_args(arg: &str) -> Result<Vec<String>> {
    // Try to parse as JSON array first
    if arg.trim_start().starts_with('[') {
        match serde_json::from_str::<Vec<String>>(arg) {
            Ok(args) => return Ok(args),
            Err(_) => {
                bail!(
                    "Failed to parse arguments as JSON array. Expected format: '[\"arg1\", \"arg2\", \"arg,with,commas\"]'"
                );
            },
        }
    }

    // Check if the string contains escaped commas
    let has_escaped_commas = arg.contains("\\,");

    if has_escaped_commas {
        // Parse with escape support
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut chars = arg.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    // Handle escape sequences
                    if let Some(&next_ch) = chars.peek() {
                        if next_ch == ',' || next_ch == '\\' {
                            current_arg.push(chars.next().unwrap());
                        } else {
                            current_arg.push(ch);
                        }
                    } else {
                        current_arg.push(ch);
                    }
                },
                ',' => {
                    // Split on unescaped comma
                    args.push(current_arg.trim().to_string());
                    current_arg.clear();
                },
                _ => {
                    current_arg.push(ch);
                },
            }
        }

        // Add the last argument
        if !current_arg.is_empty() || !args.is_empty() {
            args.push(current_arg.trim().to_string());
        }

        Ok(args)
    } else {
        // Default behavior: split on commas (backward compatibility)
        Ok(arg.split(',').map(|s| s.trim().to_string()).collect())
    }
}

async fn load_cfg(os: &Os, p: &PathBuf) -> Result<McpServerConfig> {
    Ok(if os.fs.exists(p) {
        McpServerConfig::load_from_file(os, p).await?
    } else {
        McpServerConfig::default()
    })
}

fn show_registry_only_error(output: &mut impl Write) -> Result<()> {
    writeln!(
        output,
        "❌ Your administrator has configured an MCP registry, and you can only install servers from that registry."
    )?;
    writeln!(output, "To install a registry server:")?;
    writeln!(output, "{CLI_NAME} mcp add                    # Add to default agent")?;
    writeln!(output, "{CLI_NAME} mcp add --agent <name>     # For specific agent")?;
    writeln!(output, "{CLI_NAME} mcp add --scope <scope>    # For workspace/global\n")?;
    Ok(())
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
            "No scope should default to the workspace path"
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
            name: Some("local".to_string()),
            scope: None,
            command: Some("echo hi".to_string()),
            args: vec![
                "awslabs.eks-mcp-server".to_string(),
                "--allow-write".to_string(),
                "--allow-sensitive-data-access".to_string(),
            ],
            env: vec![],
            url: None,
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
            scope: None,
            agent: None,
        }
        .execute(&os, &mut vec![])
        .await
        .unwrap();

        let cfg: McpServerConfig = serde_json::from_str(&os.fs.read_to_string(cfg_path).await.unwrap()).unwrap();
        assert!(cfg.mcp_servers.is_empty());
    }

    #[test]
    fn test_mcp_subcommand_add() {
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
                name: Some("test_server".to_string()),
                scope: None,
                command: Some("test_command".to_string()),
                url: None,
                args: vec!["awslabs.eks-mcp-server,--allow-write,--allow-sensitive-data-access".to_string(),],
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
    fn test_mcp_subcommand_add_url() {
        assert_parse!(
            [
                "mcp",
                "add",
                "--name",
                "remote_server",
                "--url",
                "http://localhost:3000"
            ],
            RootSubcommand::Mcp(McpSubcommand::Add(AddArgs {
                name: Some("remote_server".to_string()),
                scope: None,
                command: None,
                url: Some("http://localhost:3000".to_string()),
                args: vec![],
                agent: None,
                env: vec![],
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
                scope: None,
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
            }))
        );
    }

    #[test]
    fn test_parse_args_comma_separated() {
        let result = parse_args("arg1,arg2,arg3").unwrap();
        assert_eq!(result, vec!["arg1", "arg2", "arg3"]);
    }

    #[test]
    fn test_parse_args_with_escaped_commas() {
        let result = parse_args("arg1,arg2\\,with\\,commas,arg3").unwrap();
        assert_eq!(result, vec!["arg1", "arg2,with,commas", "arg3"]);
    }

    #[test]
    fn test_parse_args_json_array() {
        let result = parse_args(r#"["arg1", "arg2", "arg,with,commas"]"#).unwrap();
        assert_eq!(result, vec!["arg1", "arg2", "arg,with,commas"]);
    }

    #[test]
    fn test_parse_args_single_arg_with_commas() {
        let result = parse_args("--config=key1=val1\\,key2=val2").unwrap();
        assert_eq!(result, vec!["--config=key1=val1,key2=val2"]);
    }

    #[test]
    fn test_parse_args_backward_compatibility() {
        let result = parse_args("--config=key1=val1,key2=val2").unwrap();
        assert_eq!(result, vec!["--config=key1=val1", "key2=val2"]);
    }

    #[test]
    fn test_parse_args_mixed_escaping() {
        let result = parse_args("normal,escaped\\,comma,--flag=val1\\,val2").unwrap();
        assert_eq!(result, vec!["normal", "escaped,comma", "--flag=val1,val2"]);
    }

    #[test]
    fn test_parse_args_json_array_invalid() {
        let result = parse_args(r#"["invalid json"#);
        assert!(result.is_err());
    }
}

impl RegistryAddArgs {
    pub async fn execute(self, os: &Os, output: &mut impl Write, registry_url: String) -> Result<()> {
        let target_agent = match (self.agent.as_deref(), self.scope) {
            // Explicit agent specified
            (Some(agent_name), None) => agent_name.to_string(),

            // Explicit scope specified (legacy mode)
            (None, Some(scope)) => {
                return self.execute_legacy_scope(scope, os, output, registry_url).await;
            },

            // No target specified - use the same logic as chat sessions
            (None, None) => match determine_default_agent_or_scope(os).await? {
                Either::Left(agent_name) => {
                    writeln!(output, "No target specified, using default agent '{agent_name}'")?;
                    agent_name
                },
                Either::Right(scope) => {
                    writeln!(output, "No agent files found, using {scope} scope instead")?;
                    return self.execute_legacy_scope(scope, os, output, registry_url).await;
                },
            },

            // Both specified - error
            (Some(_), Some(_)) => {
                bail!("Cannot specify both --agent and --scope");
            },
        };

        self.execute_agent_mode(&target_agent, os, output, registry_url).await
    }

    async fn execute_agent_mode(
        &self,
        agent_name: &str,
        os: &Os,
        output: &mut impl Write,
        registry_url: String,
    ) -> Result<()> {
        // Load the target agent
        let (mut agent, config_path) = Agent::get_agent_by_name(os, agent_name).await?;

        // Fetch registry data with fallback for registry issues
        let registry_client = McpRegistryClient::new();
        let registry = match registry_client.fetch_registry(&registry_url).await {
            Ok(registry) => registry,
            Err(e) => {
                let error_type = crate::mcp_registry::RegistryErrorType::from_error(&e);
                match error_type {
                    crate::mcp_registry::RegistryErrorType::NetworkConnectivity => {
                        writeln!(output, "Check your network connection and try again.\n")?;
                    },
                    crate::mcp_registry::RegistryErrorType::RegistryData => {
                        writeln!(output, "Registry contains invalid data. Contact your administrator.\n")?;
                    },
                }
                return Ok(());
            },
        };

        // Get currently enabled servers for this agent
        let enabled_servers: std::collections::HashSet<String> =
            agent.mcp_servers.mcp_servers.keys().cloned().collect();

        // If specific server requested, add it directly
        if let Some(server_name) = &self.server {
            return self
                .add_specific_server(server_name, &mut agent, &config_path, &registry, os, output)
                .await;
        }

        // Interactive mode - show available servers
        self.interactive_add(&mut agent, &config_path, &registry, &enabled_servers, os, output)
            .await
    }

    async fn execute_legacy_scope(
        &self,
        scope: Scope,
        os: &Os,
        output: &mut impl Write,
        registry_url: String,
    ) -> Result<()> {
        // Fetch registry data with fallback for connectivity issues
        let registry_client = McpRegistryClient::new();
        let registry = match registry_client.fetch_registry(&registry_url).await {
            Ok(registry) => registry,
            Err(e) => {
                writeln!(output, "❌ Failed to fetch registry data: {e}")?;
                let error_type = crate::mcp_registry::RegistryErrorType::from_error(&e);
                match error_type {
                    crate::mcp_registry::RegistryErrorType::NetworkConnectivity => {
                        writeln!(output, "Check your network connection and try again.\n")?;
                    },
                    crate::mcp_registry::RegistryErrorType::RegistryData => {
                        writeln!(output, "Registry contains invalid data. Contact your administrator.\n")?;
                    },
                }
                return Ok(());
            },
        };

        // Load legacy config
        let resolver = PathResolver::new(os);
        let legacy_mcp_config_path = match scope {
            Scope::Workspace => resolver.workspace().mcp_config()?,
            _ => resolver.global().mcp_config()?,
        };

        // Ensure config file exists
        if !legacy_mcp_config_path.exists() {
            if let Some(parent) = legacy_mcp_config_path.parent() {
                os.fs.create_dir_all(parent).await?;
            }
            os.fs.write(&legacy_mcp_config_path, "{ \"mcpServers\": {} }").await?;
        }

        let mut mcp_servers = McpServerConfig::load_from_file(os, &legacy_mcp_config_path).await?;
        let enabled_servers: std::collections::HashSet<String> = mcp_servers.mcp_servers.keys().cloned().collect();

        // If specific server requested, add it directly
        if let Some(server_name) = &self.server {
            return self
                .add_specific_server_to_legacy(
                    server_name,
                    &mut mcp_servers,
                    &legacy_mcp_config_path,
                    &registry,
                    scope,
                    os,
                    output,
                )
                .await;
        }

        // Interactive mode for legacy config
        self.interactive_add_legacy(
            &mut mcp_servers,
            &legacy_mcp_config_path,
            &registry,
            &enabled_servers,
            scope,
            os,
            output,
        )
        .await
    }

    /// Helper function for interactive server selection menu
    /// Returns the selected server name, or None if user cancelled (ESC)
    async fn interactive_select_server(
        enabled_servers: &std::collections::HashSet<String>,
        registry: &McpRegistryResponse,
        all_enabled_message: &str,
        output: &mut impl Write,
    ) -> Result<Option<String>> {
        use dialoguer::Select;

        // Build list of available servers (not already enabled)
        let mut server_names = Vec::new();
        let mut server_labels = Vec::new();

        for server_entry in &registry.servers {
            let server = &server_entry.server;
            if !enabled_servers.contains(&server.name) {
                server_names.push(server.name.clone());

                let truncated_description = truncate_server_description(&server.description);
                let label = format!(
                    "{:<25} {}",
                    server.name.as_str(),
                    crossterm::style::Stylize::dark_grey(truncated_description)
                );
                server_labels.push(label);
            }
        }

        if server_names.is_empty() {
            writeln!(output, "✓ {all_enabled_message}")?;
            return Ok(None);
        }

        // Show interactive menu
        let interact_result = Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt("Press (↑↓) to navigate · Enter(⏎) to add a server".to_string())
            .items(&server_labels)
            .default(0)
            .report(false)
            .interact_on_opt(&dialoguer::console::Term::stdout());

        let selection = match interact_result {
            Ok(sel) => sel,
            // EINTR (Interrupted system call) - retry
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
                return Ok(Some(String::new())); // Signal to retry
            },
            Err(e) => {
                return Err(eyre::eyre!("Failed to choose server: {}", e));
            },
        };

        if let Some(index) = selection
            && index < server_names.len()
        {
            return Ok(Some(server_names[index].clone()));
        }

        // ESC was pressed
        Ok(None)
    }

    async fn interactive_add(
        &self,
        agent: &mut Agent,
        config_path: &PathBuf,
        registry: &McpRegistryResponse,
        _enabled_servers: &std::collections::HashSet<String>,
        os: &Os,
        output: &mut impl Write,
    ) -> Result<()> {
        writeln!(
            output,
            "📋 Interactive mode: Select servers to add (you can add multiple servers, press ESC when done)\n"
        )?;
        let mut changes_made = false;

        // Loop to allow adding multiple servers
        loop {
            // Get currently enabled servers (refresh each iteration)
            let enabled_servers: std::collections::HashSet<String> =
                agent.mcp_servers.mcp_servers.keys().cloned().collect();

            let all_enabled_msg = format!("All registry servers are already enabled for agent '{}'", agent.name);

            match Self::interactive_select_server(&enabled_servers, registry, &all_enabled_msg, output).await? {
                Some(server_name) if server_name.is_empty() => {
                    // Retry signal (EINTR) - loop continues
                },
                Some(server_name) => {
                    self.add_specific_server(&server_name, agent, config_path, registry, os, output)
                        .await?;
                    changes_made = true;
                },
                None => {
                    // User cancelled or all servers enabled
                    break;
                },
            }
        }

        if changes_made {
            writeln!(output, "\n✓ Agent configuration updated successfully")?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn interactive_add_legacy(
        &self,
        mcp_servers: &mut McpServerConfig,
        config_path: &PathBuf,
        registry: &McpRegistryResponse,
        _enabled_servers: &std::collections::HashSet<String>,
        scope: Scope,
        os: &Os,
        output: &mut impl Write,
    ) -> Result<()> {
        writeln!(
            output,
            "📋 Interactive mode: Select servers to add (you can add multiple servers, press ESC when done)\n"
        )?;
        let mut changes_made = false;

        // Loop to allow adding multiple servers
        loop {
            // Get currently enabled servers (refresh each iteration)
            let enabled_servers: std::collections::HashSet<String> = mcp_servers.mcp_servers.keys().cloned().collect();

            let all_enabled_msg = format!("All registry servers are already enabled in {scope} config");

            match Self::interactive_select_server(&enabled_servers, registry, &all_enabled_msg, output).await? {
                Some(server_name) if server_name.is_empty() => {
                    // Retry signal (EINTR) - loop continues
                },
                Some(server_name) => {
                    self.add_specific_server_to_legacy(
                        &server_name,
                        mcp_servers,
                        config_path,
                        registry,
                        scope,
                        os,
                        output,
                    )
                    .await?;
                    changes_made = true;
                },
                None => {
                    // User cancelled or all servers enabled
                    break;
                },
            }
        }

        if changes_made {
            writeln!(output, "\n✓ {scope} configuration updated successfully")?;
        }

        Ok(())
    }

    async fn add_specific_server(
        &self,
        server_name: &str,
        agent: &mut Agent,
        config_path: &PathBuf,
        registry: &McpRegistryResponse,
        os: &Os,
        output: &mut impl Write,
    ) -> Result<()> {
        // Verify server exists in registry
        if registry.get_server(server_name).is_none() {
            return Err(eyre::eyre!("Server '{}' not found in registry", server_name));
        }

        // Create a minimal registry reference config - just type: "registry"
        let server_config = CustomToolConfig::minimal_registry();

        // Check if server already exists
        if agent.mcp_servers.mcp_servers.contains_key(server_name) {
            writeln!(
                output,
                "⚠ Server '{server_name}' is already configured in agent '{}'",
                agent.name
            )?;
            return Ok(());
        }

        // Add to agent
        agent
            .mcp_servers
            .mcp_servers
            .insert(server_name.to_string(), server_config);

        // Add to tools whitelist if not using "*"
        if !agent.tools.contains(&"*".to_string()) {
            let tool_name = format!("@{server_name}");
            if !agent.tools.contains(&tool_name) {
                agent.tools.push(tool_name);
            }
        }

        // Save agent config
        let json = agent.to_str_pretty()?;
        os.fs.write(config_path, json).await?;

        writeln!(output, "✓ {server_name} added to agent '{}'", agent.name)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_specific_server_to_legacy(
        &self,
        server_name: &str,
        mcp_servers: &mut McpServerConfig,
        config_path: &PathBuf,
        registry: &McpRegistryResponse,
        scope: Scope,
        os: &Os,
        output: &mut impl Write,
    ) -> Result<()> {
        // Verify server exists in registry
        if registry.get_server(server_name).is_none() {
            return Err(eyre::eyre!("Server '{}' not found in registry", server_name));
        }

        // Create a minimal registry reference config - just type: "registry"
        let server_config = CustomToolConfig::minimal_registry();

        // Check if server already exists
        if mcp_servers.mcp_servers.contains_key(server_name) {
            writeln!(
                output,
                "⚠ Server '{server_name}' is already configured in {scope} config"
            )?;
            return Ok(());
        }

        // Add to legacy config
        mcp_servers.mcp_servers.insert(server_name.to_string(), server_config);

        // Save legacy config
        mcp_servers.save_to_file(os, config_path).await?;

        writeln!(output, "✓ {server_name} added to {scope} config")?;
        Ok(())
    }
}

async fn determine_default_agent_or_scope(os: &Os) -> Result<Either<String, Scope>> {
    // Use the same logic as Agents::load() for determining active agent
    if let Some(user_default) = os.database.settings.get_string(Setting::ChatDefaultAgent) {
        // Verify the user's default agent exists as a file
        if Agent::get_agent_by_name(os, &user_default).await.is_ok() {
            return Ok(Either::Left(user_default));
        }
    }

    // Check if kiro_default agent exists
    if Agent::get_agent_by_name(os, DEFAULT_AGENT_NAME).await.is_ok() {
        return Ok(Either::Left(DEFAULT_AGENT_NAME.to_string()));
    }

    // If no agent files exist, fall back to workspace scope
    // This happens when kiro_default is in-memory only
    Ok(Either::Right(Scope::Workspace))
}

#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}
