pub mod hook;
mod legacy;
mod mcp_config;
mod root_command_args;
mod wrapper_types;

use std::borrow::Borrow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::ffi::OsStr;
use std::io::{
    self,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use crossterm::style::{
    Color,
    Stylize as _,
};
use crossterm::{
    execute,
    queue,
    style,
};
use eyre::bail;
pub use mcp_config::McpServerConfig;
pub use root_command_args::*;
use schemars::{
    JsonSchema,
    schema_for,
};
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::fs::ReadDir;
use tracing::{
    error,
    info,
    warn,
};
use wrapper_types::ResourcePath;
pub use wrapper_types::{
    OriginalToolName,
    ToolSettingTarget,
    alias_schema,
    tool_settings_schema,
};

use super::chat::tools::{
    DEFAULT_APPROVE,
    NATIVE_TOOLS,
    ToolOrigin,
};
use crate::cli::agent::hook::{
    Hook,
    HookTrigger,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::{
    self,
    MCP_SERVER_TOOL_DELIMITER,
    directories,
};

pub const DEFAULT_AGENT_NAME: &str = "q_cli_default";

#[derive(Debug, Error)]
pub enum AgentConfigError {
    #[error("Json supplied at {} is invalid: {}", path.display(), error)]
    InvalidJson { error: serde_json::Error, path: PathBuf },
    #[error(
        "Agent config is malformed at {}: {}", error.instance_path, error
    )]
    SchemaMismatch {
        #[from]
        error: Box<jsonschema::ValidationError<'static>>,
    },
    #[error("Encountered directory error: {0}")]
    Directories(#[from] util::directories::DirectoryError),
    #[error("Encountered io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse legacy mcp config: {0}")]
    BadLegacyMcpConfig(#[from] eyre::Report),
}

/// An [Agent] is a declarative way of configuring a given instance of q chat. Currently, it is
/// impacting q chat in via influenicng [ContextManager] and [ToolManager].
/// Changes made to [ContextManager] and [ToolManager] do not persist across sessions.
///
/// To increase the usability of the agent config, (both from the perspective of CLI and the users
/// who would need to write these config), the agent config has two states of existence: "cold" and
/// "warm".
///
/// A "cold" state describes the config as it is written. And a "warm" state is an alternate form
/// of the same config, modified for the convenience of the business logic that relies on it in the
/// application.
///
/// For example, the "cold" state does not require the field of "path" to be populated. This is
/// because it would be redundant and tedious for user to have to write the path of the file they
/// had created in said file. This field is thus populated during its parsing.
///
/// Another example is the mcp config. To support backwards compatibility of users existing global
/// mcp.json, we allow users to supply a flag to denote whether they would want to include servers
/// from the legacy global mcp.json. If this flag exists, we would need to read the legacy mcp
/// config and merge it with what is in the agent mcp servers field. Conversely, when we write this
/// config to file, we would want to filter out the servers that belong only in the mcp.json.
///
/// Where agents are instantiated from their config, we would need to convert them from "cold" to
/// "warm".
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(description = "An Agent is a declarative way of configuring a given instance of q chat.")]
pub struct Agent {
    #[serde(rename = "$schema", default = "default_schema")]
    pub schema: String,
    /// Name of the agent
    pub name: String,
    /// This field is not model facing and is mostly here for users to discern between agents
    #[serde(default)]
    pub description: Option<String>,
    /// The intention for this field is to provide high level context to the
    /// agent. This should be seen as the same category of context as a system prompt.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Configuration for Model Context Protocol (MCP) servers
    #[serde(default)]
    pub mcp_servers: McpServerConfig,
    /// List of tools the agent can see. Use \"@{MCP_SERVER_NAME}/tool_name\" to specify tools from
    /// mcp servers. To include all tools from a server, use \"@{MCP_SERVER_NAME}\"
    #[serde(default)]
    pub tools: Vec<String>,
    /// Tool aliases for remapping tool names
    #[serde(default)]
    #[schemars(schema_with = "alias_schema")]
    pub tool_aliases: HashMap<OriginalToolName, String>,
    /// List of tools the agent is explicitly allowed to use
    #[serde(default)]
    pub allowed_tools: HashSet<String>,
    /// Files to include in the agent's context
    #[serde(default)]
    pub resources: Vec<ResourcePath>,
    /// Commands to run when a chat session is created
    #[serde(default)]
    pub hooks: HashMap<HookTrigger, Vec<Hook>>,
    /// Settings for specific tools. These are mostly for native tools. The actual schema differs by
    /// tools and is documented in detail in our documentation
    #[serde(default)]
    #[schemars(schema_with = "tool_settings_schema")]
    pub tools_settings: HashMap<ToolSettingTarget, serde_json::Value>,
    /// Whether or not to include the legacy ~/.aws/amazonq/mcp.json in the agent
    /// You can reference tools brought in by these servers as just as you would with the servers
    /// you configure in the mcpServers field in this config
    #[serde(default)]
    pub use_legacy_mcp_json: bool,
    /// The model ID to use for this agent. If not specified, uses the default model.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            name: DEFAULT_AGENT_NAME.to_string(),
            description: Some("Default agent".to_string()),
            prompt: Default::default(),
            mcp_servers: Default::default(),
            tools: vec!["*".to_string()],
            tool_aliases: Default::default(),
            allowed_tools: {
                let mut set = HashSet::<String>::new();
                let default_approve = DEFAULT_APPROVE.iter().copied().map(str::to_string);
                set.extend(default_approve);
                set
            },
            resources: vec![
                "file://AmazonQ.md",
                "file://AGENTS.md",
                "file://README.md",
                "file://.amazonq/rules/**/*.md",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>(),
            hooks: Default::default(),
            tools_settings: Default::default(),
            use_legacy_mcp_json: true,
            model: None,
            path: None,
        }
    }
}

impl Agent {
    /// This function mutates the agent to a state that is writable.
    /// Practically this means reverting some fields back to their original values as they were
    /// written in the config.
    fn freeze(&mut self) {
        let Self { mcp_servers, .. } = self;

        mcp_servers
            .mcp_servers
            .retain(|_name, config| !config.is_from_legacy_mcp_json);
    }

    /// This function mutates the agent to a state that is usable for runtime.
    /// Practically this means to convert some of the fields value to their usable counterpart.
    /// For example, converting the mcp array to actual mcp config and populate the agent file path.
    fn thaw(
        &mut self,
        path: &Path,
        legacy_mcp_config: Option<&McpServerConfig>,
        output: &mut impl Write,
    ) -> Result<(), AgentConfigError> {
        let Self { mcp_servers, .. } = self;

        self.path = Some(path.to_path_buf());

        if let (true, Some(legacy_mcp_config)) = (self.use_legacy_mcp_json, legacy_mcp_config) {
            for (name, legacy_server) in &legacy_mcp_config.mcp_servers {
                if mcp_servers.mcp_servers.contains_key(name) {
                    let _ = queue!(
                        output,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("WARNING: "),
                        style::ResetColor,
                        style::Print("MCP server '"),
                        style::SetForegroundColor(Color::Green),
                        style::Print(name),
                        style::ResetColor,
                        style::Print(
                            "' is already configured in agent config. Skipping duplicate from legacy mcp.json.\n"
                        )
                    );
                    continue;
                }
                let mut server_clone = legacy_server.clone();
                server_clone.is_from_legacy_mcp_json = true;
                mcp_servers.mcp_servers.insert(name.clone(), server_clone);
            }
        }

        output.flush()?;

        Ok(())
    }

    pub fn print_overridden_permissions(&self, output: &mut impl Write) -> Result<(), AgentConfigError> {
        let execute_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        for allowed_tool in &self.allowed_tools {
            if let Some(settings) = self.tools_settings.get(allowed_tool.as_str()) {
                // currently we only have four native tools that offers tool settings
                let overridden_settings_key = match allowed_tool.as_str() {
                    "fs_read" | "fs_write" => Some("allowedPaths"),
                    "use_aws" => Some("allowedServices"),
                    name if name == execute_name => Some("allowedCommands"),
                    _ => None,
                };

                if let Some(key) = overridden_settings_key {
                    if let Some(ref override_settings) = settings.get(key).map(|value| format!("{key}: {value}")) {
                        queue_permission_override_warning(allowed_tool.as_str(), override_settings, output)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn to_str_pretty(&self) -> eyre::Result<String> {
        let mut agent_clone = self.clone();
        agent_clone.freeze();
        Ok(serde_json::to_string_pretty(&agent_clone)?)
    }

    /// Retrieves an agent by name. It does so via first seeking the given agent under local dir,
    /// and falling back to global dir if it does not exist in local.
    pub async fn get_agent_by_name(os: &Os, agent_name: &str) -> eyre::Result<(Agent, PathBuf)> {
        let config_path: Result<PathBuf, PathBuf> = 'config: {
            // local first, and then fall back to looking at global
            let local_config_dir = directories::chat_local_agent_dir(os)?.join(format!("{agent_name}.json"));
            if os.fs.exists(&local_config_dir) {
                break 'config Ok(local_config_dir);
            }

            let global_config_dir = directories::chat_global_agent_path(os)?.join(format!("{agent_name}.json"));
            if os.fs.exists(&global_config_dir) {
                break 'config Ok(global_config_dir);
            }

            Err(global_config_dir)
        };

        match config_path {
            Ok(config_path) => {
                let content = os.fs.read(&config_path).await?;
                let mut agent = serde_json::from_slice::<Agent>(&content)?;
                let legacy_mcp_config = if agent.use_legacy_mcp_json {
                    load_legacy_mcp_config(os).await.unwrap_or(None)
                } else {
                    None
                };
                let mut stderr = std::io::stderr();
                agent.thaw(&config_path, legacy_mcp_config.as_ref(), &mut stderr)?;
                Ok((agent, config_path))
            },
            _ => bail!("Agent {agent_name} does not exist"),
        }
    }

    pub async fn load(
        os: &Os,
        agent_path: impl AsRef<Path>,
        legacy_mcp_config: &mut Option<McpServerConfig>,
        mcp_enabled: bool,
        output: &mut impl Write,
    ) -> Result<Agent, AgentConfigError> {
        let content = os.fs.read(&agent_path).await?;
        let mut agent = serde_json::from_slice::<Agent>(&content).map_err(|e| AgentConfigError::InvalidJson {
            error: e,
            path: agent_path.as_ref().to_path_buf(),
        })?;

        if mcp_enabled {
            if agent.use_legacy_mcp_json && legacy_mcp_config.is_none() {
                let config = load_legacy_mcp_config(os).await.unwrap_or_default();
                if let Some(config) = config {
                    legacy_mcp_config.replace(config);
                }
            }
            agent.thaw(agent_path.as_ref(), legacy_mcp_config.as_ref(), output)?;
        } else {
            agent.clear_mcp_configs();
            // Thaw the agent with empty MCP config to finalize normalization.
            agent.thaw(agent_path.as_ref(), None, output)?;
        }
        Ok(agent)
    }

    /// Clear all MCP configurations while preserving built-in tools
    pub fn clear_mcp_configs(&mut self) {
        self.mcp_servers = McpServerConfig::default();
        self.use_legacy_mcp_json = false;

        // Transform tools: "*" → "@builtin", remove MCP refs
        self.tools = self
            .tools
            .iter()
            .filter_map(|tool| match tool.as_str() {
                "*" => Some("@builtin".to_string()),
                t if !is_mcp_tool_ref(t) => Some(t.to_string()),
                _ => None,
            })
            .collect();

        // Remove MCP references from other fields
        self.allowed_tools.retain(|tool| !is_mcp_tool_ref(tool));
        self.tool_aliases.retain(|orig, _| !is_mcp_tool_ref(&orig.to_string()));
        self.tools_settings
            .retain(|target, _| !is_mcp_tool_ref(&target.to_string()));
    }
}

/// Result of evaluating tool permissions, indicating whether a tool should be allowed,
/// require user confirmation, or be denied with specific reasons.
#[derive(Debug, PartialEq)]
pub enum PermissionEvalResult {
    /// Tool is allowed to execute without user confirmation
    Allow,
    /// Tool requires user confirmation before execution
    Ask,
    /// Denial with specific reasons explaining why the tool was denied
    /// Tools are free to overload what these reasons are
    Deny(Vec<String>),
}

#[derive(Clone, Default, Debug)]
pub struct Agents {
    /// Mapping from agent name to an [Agent].
    pub agents: HashMap<String, Agent>,
    /// Agent name.
    pub active_idx: String,
    pub trust_all_tools: bool,
}

impl Agents {
    /// This function assumes the relevant transformation to the tool names have been done:
    /// - model tool name -> host tool name
    /// - custom tool namespacing
    pub fn trust_tools(&mut self, tool_names: Vec<String>) {
        if let Some(agent) = self.get_active_mut() {
            agent.allowed_tools.extend(tool_names);
        }
    }

    /// This function assumes the relevant transformation to the tool names have been done:
    /// - model tool name -> host tool name
    /// - custom tool namespacing
    pub fn untrust_tools(&mut self, tool_names: &[String]) {
        if let Some(agent) = self.get_active_mut() {
            agent.allowed_tools.retain(|t| !tool_names.contains(t));
        }
    }

    pub fn get_active(&self) -> Option<&Agent> {
        self.agents.get(&self.active_idx)
    }

    pub fn get_active_mut(&mut self) -> Option<&mut Agent> {
        self.agents.get_mut(&self.active_idx)
    }

    pub fn switch(&mut self, name: &str) -> eyre::Result<&Agent> {
        if !self.agents.contains_key(name) {
            eyre::bail!("No agent with name {name} found");
        }
        self.active_idx = name.to_string();
        self.agents
            .get(name)
            .ok_or(eyre::eyre!("No agent with name {name} found"))
    }

    /// This function does a number of things in the following order:
    /// 1. Migrates old profiles if applicable
    /// 2. Loads local agents
    /// 3. Loads global agents
    /// 4. Resolve agent conflicts and merge the two sets of agents
    /// 5. Validates the active agent config and surfaces error to output accordingly
    ///
    /// # Arguments
    /// * `os` - Operating system interface for file system operations and database access
    /// * `agent_name` - Optional specific agent name to activate; if None, falls back to default
    ///   agent selection
    /// * `skip_migration` - If true, skips migration of old profiles to new format
    /// * `output` - Writer for outputting warnings, errors, and status messages during loading
    pub async fn load(
        os: &mut Os,
        agent_name: Option<&str>,
        skip_migration: bool,
        output: &mut impl Write,
        mcp_enabled: bool,
    ) -> (Self, AgentsLoadMetadata) {
        if !mcp_enabled {
            let _ = execute!(
                output,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                style::SetForegroundColor(Color::Reset),
                style::Print("MCP functionality has been disabled by your administrator.\n\n"),
            );
        }

        // Tracking metadata about the performed load operation.
        let mut load_metadata = AgentsLoadMetadata::default();

        let new_agents = if !skip_migration {
            match legacy::migrate(os, false).await {
                Ok(Some(new_agents)) => {
                    let migrated_count = new_agents.len();
                    info!(migrated_count, "Profile migration successful");
                    load_metadata.migration_performed = true;
                    load_metadata.migrated_count = migrated_count as u32;
                    new_agents
                },
                Ok(None) => {
                    info!("Migration was not performed");
                    vec![]
                },
                Err(e) => {
                    error!("Migration did not happen for the following reason: {e}");
                    vec![]
                },
            }
        } else {
            vec![]
        };

        let mut global_mcp_config = None::<McpServerConfig>;

        let mut local_agents = 'local: {
            // We could be launching from the home dir, in which case the global and local agents
            // are the same set of agents. If that is the case, we simply skip this.
            match (std::env::current_dir(), directories::home_dir(os)) {
                (Ok(cwd), Ok(home_dir)) if cwd == home_dir => break 'local Vec::<Agent>::new(),
                _ => {
                    // noop, we keep going with the extraction of local agents (even if we have an
                    // error retrieving cwd or home_dir)
                },
            }

            let Ok(path) = directories::chat_local_agent_dir(os) else {
                break 'local Vec::<Agent>::new();
            };
            let Ok(files) = os.fs.read_dir(path).await else {
                break 'local Vec::<Agent>::new();
            };

            let mut agents = Vec::<Agent>::new();
            let results = load_agents_from_entries(files, os, &mut global_mcp_config, mcp_enabled, output).await;
            for result in results {
                match result {
                    Ok(agent) => agents.push(agent),
                    Err(e) => {
                        load_metadata.load_failed_count += 1;
                        let _ = queue!(
                            output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(e),
                            style::Print("\n"),
                        );
                    },
                }
            }

            agents
        };

        let mut global_agents = 'global: {
            let Ok(path) = directories::chat_global_agent_path(os) else {
                break 'global Vec::<Agent>::new();
            };
            let files = match os.fs.read_dir(&path).await {
                Ok(files) => files,
                Err(e) => {
                    if matches!(e.kind(), io::ErrorKind::NotFound) {
                        if let Err(e) = os.fs.create_dir_all(&path).await {
                            error!("Error creating global agent dir: {:?}", e);
                        }
                    }
                    break 'global Vec::<Agent>::new();
                },
            };

            let mut agents = Vec::<Agent>::new();
            let results = load_agents_from_entries(files, os, &mut global_mcp_config, mcp_enabled, output).await;
            for result in results {
                match result {
                    Ok(agent) => agents.push(agent),
                    Err(e) => {
                        load_metadata.load_failed_count += 1;
                        let _ = queue!(
                            output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(e),
                            style::Print("\n"),
                        );
                    },
                }
            }

            agents
        }
        .into_iter()
        .chain(new_agents)
        .collect::<Vec<_>>();

        // Here we also want to make sure the example config is written to disk if it's not already
        // there.
        // Note that this config is not what q chat uses. It merely serves as an example.
        'example_config: {
            let Ok(path) = directories::example_agent_config(os) else {
                error!("Error obtaining example agent path.");
                break 'example_config;
            };
            if os.fs.exists(&path) {
                break 'example_config;
            }

            // At this point the agents dir would have been created. All we have to worry about is
            // the creation of the example config
            if let Err(e) = os.fs.create_new(&path).await {
                error!("Error creating example agent config: {e}.");
                break 'example_config;
            }

            let example_agent = Agent {
                // This is less important than other fields since names are derived from the name
                // of the config file and thus will not be persisted
                name: "example".to_string(),
                description: Some("This is an example agent config (and will not be loaded unless you change it to have .json extension)".to_string()),
                tools: {
                    NATIVE_TOOLS
                        .iter()
                        .copied()
                        .map(str::to_string)
                        .chain(vec![
                            format!("@mcp_server_name{MCP_SERVER_TOOL_DELIMITER}mcp_tool_name"),
                            "@mcp_server_name_without_tool_specification_to_include_all_tools".to_string(),
                        ])
                        .collect::<Vec<_>>()
                },
                ..Default::default()
            };
            let Ok(content) = example_agent.to_str_pretty() else {
                error!("Error serializing example agent config");
                break 'example_config;
            };
            if let Err(e) = os.fs.write(&path, &content).await {
                error!("Error writing example agent config to file: {e}");
                break 'example_config;
            };
        }

        let local_names = local_agents.iter().map(|a| a.name.as_str()).collect::<HashSet<&str>>();
        global_agents.retain(|a| {
            // If there is a naming conflict for agents, we would retain the local instance
            let name = a.name.as_str();
            if local_names.contains(name) {
                let _ = queue!(
                    output,
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("WARNING: "),
                    style::ResetColor,
                    style::Print("Agent conflict for "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print(name),
                    style::ResetColor,
                    style::Print(". Using workspace version.\n")
                );
                false
            } else {
                true
            }
        });

        local_agents.append(&mut global_agents);
        let mut all_agents = local_agents;

        // Assume agent in the following order of priority:
        // 1. The agent name specified by the start command via --agent (this is the agent_name that's
        //    passed in)
        // 2. If the above is missing or invalid, assume one that is specified by chat.defaultAgent
        // 3. If the above is missing or invalid, assume the in-memory default
        let active_idx = 'active_idx: {
            if let Some(name) = agent_name {
                if all_agents.iter().any(|a| a.name.as_str() == name) {
                    break 'active_idx name.to_string();
                }
                let _ = queue!(
                    output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error"),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!(
                        ": no agent with name {} found. Falling back to user specified default",
                        name
                    )),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Reset)
                );
            }

            if let Some(user_set_default) = os.database.settings.get_string(Setting::ChatDefaultAgent) {
                if all_agents.iter().any(|a| a.name == user_set_default) {
                    break 'active_idx user_set_default;
                }
                let _ = queue!(
                    output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error"),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!(
                        ": user defined default {} not found. Falling back to in-memory default",
                        user_set_default
                    )),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Reset)
                );
            }

            all_agents.push({
                let mut agent = Agent::default();
                if mcp_enabled {
                    'load_legacy_mcp_json: {
                        if global_mcp_config.is_none() {
                            let Ok(global_mcp_path) = directories::chat_legacy_global_mcp_config(os) else {
                                tracing::error!("Error obtaining legacy mcp json path. Skipping");
                                break 'load_legacy_mcp_json;
                            };
                            let legacy_mcp_config = match McpServerConfig::load_from_file(os, global_mcp_path).await {
                                Ok(config) => config,
                                Err(e) => {
                                    tracing::error!("Error loading global mcp json path: {e}. Skipping");
                                    break 'load_legacy_mcp_json;
                                },
                            };
                            global_mcp_config.replace(legacy_mcp_config);
                        }
                    }

                    if let Some(config) = &global_mcp_config {
                        agent.mcp_servers = config.clone();
                    }
                } else {
                    agent.mcp_servers = McpServerConfig::default();
                }
                agent
            });

            DEFAULT_AGENT_NAME.to_string()
        };

        let _ = output.flush();

        // Post parsing validation here
        let schema = schema_for!(Agent);
        let agents = all_agents
            .into_iter()
            .map(|a| (a.name.clone(), a))
            .collect::<HashMap<_, _>>();
        let active_agent = agents.get(&active_idx);

        'validate: {
            match (serde_json::to_value(schema), active_agent) {
                (Ok(schema), Some(agent)) => {
                    let Ok(instance) = serde_json::to_value(agent) else {
                        let name = &agent.name;
                        error!("Error converting active agent {name} to value for validation. Skipping");
                        break 'validate;
                    };
                    if let Err(e) = jsonschema::validate(&schema, &instance).map_err(|e| e.to_owned()) {
                        let name = &agent.name;
                        let _ = execute!(
                            output,
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
                        );
                    }
                },
                (Err(e), _) => {
                    error!("Failed to convert agent definition to schema: {e}. Skipping validation");
                },
                (_, None) => {
                    warn!("Skipping config validation because there is no active agent");
                },
            }
        }

        load_metadata.launched_agent = active_idx.clone();
        (
            Self {
                agents,
                active_idx,
                ..Default::default()
            },
            load_metadata,
        )
    }

    /// Returns a label to describe the permission status for a given tool.
    pub fn display_label(&self, tool_name: &str, origin: &ToolOrigin) -> String {
        use crate::util::pattern_matching::matches_any_pattern;

        let tool_trusted = self.get_active().is_some_and(|a| {
            if matches!(origin, &ToolOrigin::Native) {
                return matches_any_pattern(&a.allowed_tools, tool_name);
            }

            a.allowed_tools.iter().any(|name| {
                name.strip_prefix("@").is_some_and(|remainder| {
                    remainder
                        .split_once(MCP_SERVER_TOOL_DELIMITER)
                        .is_some_and(|(_left, right)| right == tool_name)
                        || remainder == <ToolOrigin as Borrow<str>>::borrow(origin)
                }) || {
                    if let Some(server_name) = name.strip_prefix("@").and_then(|s| s.split('/').next()) {
                        if server_name == <ToolOrigin as Borrow<str>>::borrow(origin) {
                            let tool_pattern = format!("@{}/{}", server_name, tool_name);
                            matches_any_pattern(&a.allowed_tools, &tool_pattern)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            })
        });

        if tool_trusted || self.trust_all_tools {
            format!("* {}", "trusted".dark_green().bold())
        } else {
            self.default_permission_label(tool_name)
        }
    }

    /// Provide default permission labels for the built-in set of tools.
    // This "static" way avoids needing to construct a tool instance.
    fn default_permission_label(&self, tool_name: &str) -> String {
        let label = match tool_name {
            "fs_read" => "trusted".dark_green().bold(),
            "fs_write" => "not trusted".dark_grey(),
            #[cfg(not(windows))]
            "execute_bash" => "trust read-only commands".dark_grey(),
            #[cfg(windows)]
            "execute_cmd" => "trust read-only commands".dark_grey(),
            "use_aws" => "trust read-only commands".dark_grey(),
            "report_issue" => "trusted".dark_green().bold(),
            "introspect" => "trusted".dark_green().bold(),
            "thinking" => "trusted (prerelease)".dark_green().bold(),
            "todo_list" => "trusted".dark_green().bold(),
            _ if self.trust_all_tools => "trusted".dark_grey().bold(),
            _ => "not trusted".dark_grey(),
        };

        format!("{} {label}", "*".reset())
    }
}

/// Metadata from the executed [Agents::load] operation.
#[derive(Debug, Clone, Default)]
pub struct AgentsLoadMetadata {
    pub migration_performed: bool,
    pub migrated_count: u32,
    pub load_count: u32,
    pub load_failed_count: u32,
    pub launched_agent: String,
}

async fn load_agents_from_entries(
    mut files: ReadDir,
    os: &Os,
    global_mcp_config: &mut Option<McpServerConfig>,
    mcp_enabled: bool,
    output: &mut impl Write,
) -> Vec<Result<Agent, AgentConfigError>> {
    let mut res = Vec::<Result<Agent, AgentConfigError>>::new();

    while let Ok(Some(file)) = files.next_entry().await {
        let file_path = &file.path();
        if file_path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|s| s == "json")
        {
            res.push(Agent::load(os, file_path, global_mcp_config, mcp_enabled, output).await);
        }
    }

    res
}

/// Loads legacy mcp config by combining workspace and global config.
/// In case of a server naming conflict, the workspace config is prioritized.
async fn load_legacy_mcp_config(os: &Os) -> eyre::Result<Option<McpServerConfig>> {
    let global_mcp_path = directories::chat_legacy_global_mcp_config(os)?;
    let global_mcp_config = match McpServerConfig::load_from_file(os, global_mcp_path).await {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::error!("Error loading global mcp json path: {e}.");
            None
        },
    };

    let workspace_mcp_path = directories::chat_legacy_workspace_mcp_config(os)?;
    let workspace_mcp_config = match McpServerConfig::load_from_file(os, workspace_mcp_path).await {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::error!("Error loading global mcp json path: {e}.");
            None
        },
    };

    Ok(match (workspace_mcp_config, global_mcp_config) {
        (Some(mut wc), Some(gc)) => {
            for (server_name, config) in gc.mcp_servers {
                // We prioritize what is in the workspace
                wc.mcp_servers.entry(server_name).or_insert(config);
            }

            Some(wc)
        },
        (None, Some(gc)) => Some(gc),
        (Some(wc), None) => Some(wc),
        _ => None,
    })
}

pub fn queue_permission_override_warning(
    tool_name: &str,
    overridden_settings: &str,
    output: &mut impl Write,
) -> Result<(), std::io::Error> {
    Ok(queue!(
        output,
        style::SetForegroundColor(Color::Yellow),
        style::Print("WARNING: "),
        style::ResetColor,
        style::Print("You have trusted "),
        style::SetForegroundColor(Color::Green),
        style::Print(tool_name),
        style::ResetColor,
        style::Print(" tool, which overrides the toolsSettings: "),
        style::SetForegroundColor(Color::Cyan),
        style::Print(overridden_settings),
        style::ResetColor,
        style::Print("\n"),
    )?)
}

fn default_schema() -> String {
    "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json".into()
}

// Check if a tool reference is MCP-specific (not @builtin and starts with @)
pub fn is_mcp_tool_ref(s: &str) -> bool {
    // @builtin is not MCP, it's a reference to all built-in tools
    // Any other @ prefix is MCP (e.g., "@git", "@git/git_status")
    !s.starts_with("@builtin") && s.starts_with('@')
}

#[cfg(test)]
fn validate_agent_name(name: &str) -> eyre::Result<()> {
    // Check if name is empty
    if name.is_empty() {
        eyre::bail!("Agent name cannot be empty");
    }

    // Check if name contains only allowed characters and starts with an alphanumeric character
    let re = regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")?;
    if !re.is_match(name) {
        eyre::bail!(
            "Agent name must start with an alphanumeric character and can only contain alphanumeric characters, hyphens, and underscores"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    const INPUT: &str = r#"
            {
              "name": "some_agent",
              "description": "My developer agent is used for small development tasks like solving open issues.",
              "prompt": "You are a principal developer who uses multiple agents to accomplish difficult engineering tasks",
              "mcpServers": {
                "fetch": { "command": "fetch3.1", "args": [] },
                "git": { "command": "git-mcp", "args": [] }
              },
              "tools": [                                    
                "@git"
              ],
              "toolAliases": {
                  "@gits/some_tool": "some_tool2"
              },
              "allowedTools": [                           
                "fs_read",                               
                "@fetch",
                "@gits/git_status"
              ],
              "resources": [                        
                "file://~/my-genai-prompts/unittest.md"
              ],
              "toolsSettings": {                     
                "fs_write": { "allowedPaths": ["~/**"] },
                "@git/git_status": { "git_user": "$GIT_USER" }
              }
            }
        "#;

    #[test]
    fn test_deser() {
        let agent = serde_json::from_str::<Agent>(INPUT).expect("Deserializtion failed");
        assert!(agent.mcp_servers.mcp_servers.contains_key("fetch"));
        assert!(agent.mcp_servers.mcp_servers.contains_key("git"));
        assert!(agent.tool_aliases.contains_key("@gits/some_tool"));
    }

    #[test]
    fn test_get_active() {
        let mut collection = Agents::default();
        assert!(collection.get_active().is_none());

        let agent = Agent::default();
        let agent_name = agent.name.clone();
        collection.agents.insert(agent_name.clone(), agent);
        collection.active_idx = agent_name.clone();

        assert!(collection.get_active().is_some());
        assert_eq!(collection.get_active().unwrap().name, agent_name);
    }

    #[test]
    fn test_get_active_mut() {
        let mut collection = Agents::default();
        assert!(collection.get_active_mut().is_none());

        let agent = Agent::default();
        collection.agents.insert("default".to_string(), agent);
        collection.active_idx = "default".to_string();

        assert!(collection.get_active_mut().is_some());
        let active = collection.get_active_mut().unwrap();
        active.description = Some("Modified description".to_string());

        assert_eq!(
            collection.agents.get("default").unwrap().description,
            Some("Modified description".to_string())
        );
    }

    #[test]
    fn test_switch() {
        let mut collection = Agents::default();

        let default_agent = Agent::default();
        let dev_agent = Agent {
            name: "dev".to_string(),
            description: Some("Developer agent".to_string()),
            ..Default::default()
        };

        collection.agents.insert("default".to_string(), default_agent);
        collection.agents.insert("dev".to_string(), dev_agent);
        collection.active_idx = "default".to_string();

        // Test successful switch
        let result = collection.switch("dev");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "dev");

        // Test switch to non-existent agent
        let result = collection.switch("nonexistent");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "No agent with name nonexistent found");
    }

    #[test]
    fn test_validate_agent_name() {
        // Valid names
        assert!(validate_agent_name("valid").is_ok());
        assert!(validate_agent_name("valid123").is_ok());
        assert!(validate_agent_name("valid-name").is_ok());
        assert!(validate_agent_name("valid_name").is_ok());
        assert!(validate_agent_name("123valid").is_ok());

        // Invalid names
        assert!(validate_agent_name("").is_err());
        assert!(validate_agent_name("-invalid").is_err());
        assert!(validate_agent_name("_invalid").is_err());
        assert!(validate_agent_name("invalid!").is_err());
        assert!(validate_agent_name("invalid space").is_err());
    }

    #[test]
    fn test_clear_mcp_configs_with_builtin_variants() {
        let mut agent: Agent = serde_json::from_value(json!({
            "name": "test",
            "tools": [
                "@builtin",
                "@builtin/fs_read",
                "@builtin/execute_bash",
                "@git",
                "@git/status",
                "fs_write"
            ],
            "allowedTools": [
                "@builtin/fs_read",
                "@git/status",
                "fs_write"
            ],
            "toolAliases": {
                "@builtin/fs_read": "read",
                "@git/status": "git_st"
            },
            "toolsSettings": {
                "@builtin/fs_write": { "allowedPaths": ["~/**"] },
                "@git/commit": { "sign": true }
            }
        }))
        .unwrap();

        agent.clear_mcp_configs();

        // All @builtin variants should be preserved while MCP tools should be removed
        assert!(agent.tools.contains(&"@builtin".to_string()));
        assert!(agent.tools.contains(&"@builtin/fs_read".to_string()));
        assert!(agent.tools.contains(&"@builtin/execute_bash".to_string()));
        assert!(agent.tools.contains(&"fs_write".to_string()));
        assert!(!agent.tools.contains(&"@git".to_string()));
        assert!(!agent.tools.contains(&"@git/status".to_string()));

        assert!(agent.allowed_tools.contains("@builtin/fs_read"));
        assert!(agent.allowed_tools.contains("fs_write"));
        assert!(!agent.allowed_tools.contains("@git/status"));

        // Check tool aliases - need to iterate since we can't construct OriginalToolName directly
        let has_builtin_alias = agent
            .tool_aliases
            .iter()
            .any(|(k, v)| k.to_string() == "@builtin/fs_read" && v == "read");
        assert!(has_builtin_alias, "@builtin/fs_read alias should be preserved");

        let has_git_alias = agent.tool_aliases.iter().any(|(k, _)| k.to_string() == "@git/status");
        assert!(!has_git_alias, "@git/status alias should be removed");

        // Check tool settings - need to iterate since we can't construct ToolSettingTarget directly
        let has_builtin_setting = agent
            .tools_settings
            .iter()
            .any(|(k, _)| k.to_string() == "@builtin/fs_write");
        assert!(has_builtin_setting, "@builtin/fs_write settings should be preserved");

        let has_git_setting = agent.tools_settings.iter().any(|(k, _)| k.to_string() == "@git/commit");
        assert!(!has_git_setting, "@git/commit settings should be removed");
    }

    #[test]
    fn test_display_label_no_active_agent() {
        let agents = Agents::default();

        let label = agents.display_label("fs_read", &ToolOrigin::Native);
        // With no active agent, it should fall back to default permissions
        // fs_read has a default of "trusted"
        assert!(
            label.contains("trusted"),
            "fs_read should show default trusted permission, instead found: {}",
            label
        );
    }

    #[test]
    fn test_display_label_trust_all_tools() {
        let agents = Agents {
            trust_all_tools: true,
            ..Default::default()
        };

        // Should be trusted even if not in allowed_tools
        let label = agents.display_label("random_tool", &ToolOrigin::Native);
        assert!(
            label.contains("trusted"),
            "trust_all_tools should make everything trusted, instead found: {}",
            label
        );
    }

    #[test]
    fn test_display_label_default_permissions() {
        let agents = Agents::default();

        // Test default permissions for known tools
        let fs_read_label = agents.display_label("fs_read", &ToolOrigin::Native);
        assert!(
            fs_read_label.contains("trusted"),
            "fs_read should be trusted by default, instead found: {}",
            fs_read_label
        );

        let fs_write_label = agents.display_label("fs_write", &ToolOrigin::Native);
        assert!(
            fs_write_label.contains("not trusted"),
            "fs_write should not be trusted by default, instead found: {}",
            fs_write_label
        );

        let execute_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        let execute_bash_label = agents.display_label(execute_name, &ToolOrigin::Native);
        assert!(
            execute_bash_label.contains("read-only"),
            "execute_bash should show read-only by default, instead found: {}",
            execute_bash_label
        );
    }

    #[test]
    fn test_display_label_comprehensive_patterns() {
        let mut agents = Agents::default();

        // Create agent with all types of patterns
        let mut allowed_tools = HashSet::new();
        // Native exact match
        allowed_tools.insert("fs_read".to_string());
        // Native wildcard
        allowed_tools.insert("execute_*".to_string());
        // MCP server exact (allows all tools from that server)
        allowed_tools.insert("@server1".to_string());
        // MCP tool exact
        allowed_tools.insert("@server2/specific_tool".to_string());
        // MCP tool wildcard
        allowed_tools.insert("@server3/tool_*".to_string());

        let agent = Agent {
            schema: "test".to_string(),
            name: "test-agent".to_string(),
            description: None,
            prompt: None,
            mcp_servers: Default::default(),
            tools: Vec::new(),
            tool_aliases: Default::default(),
            allowed_tools,
            tools_settings: Default::default(),
            resources: Vec::new(),
            hooks: Default::default(),
            use_legacy_mcp_json: false,
            model: None,
            path: None,
        };

        agents.agents.insert("test-agent".to_string(), agent);
        agents.active_idx = "test-agent".to_string();

        // Test 1: Native exact match
        let label = agents.display_label("fs_read", &ToolOrigin::Native);
        assert!(
            label.contains("trusted"),
            "fs_read should be trusted (exact match), instead found: {}",
            label
        );

        // Test 2: Native wildcard match
        let label = agents.display_label("execute_bash", &ToolOrigin::Native);
        assert!(
            label.contains("trusted"),
            "execute_bash should match execute_* pattern, instead found: {}",
            label
        );

        // Test 3: Native no match
        let label = agents.display_label("fs_write", &ToolOrigin::Native);
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "fs_write should not be trusted, instead found: {}",
            label
        );

        // Test 4: MCP server exact match (allows any tool from server1)
        let label = agents.display_label("any_tool", &ToolOrigin::McpServer("server1".to_string()));
        assert!(
            label.contains("trusted"),
            "Server-level permission should allow any tool, instead found: {}",
            label
        );

        // Test 5: MCP tool exact match
        let label = agents.display_label("specific_tool", &ToolOrigin::McpServer("server2".to_string()));
        assert!(
            label.contains("trusted"),
            "Exact MCP tool should be trusted, instead found: {}",
            label
        );

        // Test 6: MCP tool wildcard match
        let label = agents.display_label("tool_read", &ToolOrigin::McpServer("server3".to_string()));
        assert!(
            label.contains("trusted"),
            "tool_read should match @server3/tool_* pattern, instead found: {}",
            label
        );

        // Test 7: MCP tool no match
        let label = agents.display_label("other_tool", &ToolOrigin::McpServer("server2".to_string()));
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "Non-matching MCP tool should not be trusted, instead found: {}",
            label
        );

        // Test 8: MCP server no match
        let label = agents.display_label("some_tool", &ToolOrigin::McpServer("unknown_server".to_string()));
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "Unknown server should not be trusted, instead found: {}",
            label
        );
    }

    #[test]
    fn test_agent_model_field() {
        // Test deserialization with model field
        let agent_json = r#"{
            "name": "test-agent",
            "model": "claude-sonnet-4"
        }"#;

        let agent: Agent = serde_json::from_str(agent_json).expect("Failed to deserialize agent with model");
        assert_eq!(agent.model, Some("claude-sonnet-4".to_string()));

        // Test default agent has no model
        let default_agent = Agent::default();
        assert_eq!(default_agent.model, None);

        // Test serialization includes model field
        let agent_with_model = Agent {
            model: Some("test-model".to_string()),
            ..Default::default()
        };
        let serialized = serde_json::to_string(&agent_with_model).expect("Failed to serialize");
        assert!(serialized.contains("\"model\":\"test-model\""));
    }

    #[test]
    fn test_agent_model_fallback_priority() {
        // Test that agent model is checked and falls back correctly
        let mut agents = Agents::default();

        // Create agent with unavailable model
        let agent_with_invalid_model = Agent {
            name: "test-agent".to_string(),
            model: Some("unavailable-model".to_string()),
            ..Default::default()
        };

        agents.agents.insert("test-agent".to_string(), agent_with_invalid_model);
        agents.active_idx = "test-agent".to_string();

        // Verify the agent has the model set
        assert_eq!(
            agents.get_active().and_then(|a| a.model.as_ref()),
            Some(&"unavailable-model".to_string())
        );

        // Test agent without model
        let agent_without_model = Agent {
            name: "no-model-agent".to_string(),
            model: None,
            ..Default::default()
        };

        agents.agents.insert("no-model-agent".to_string(), agent_without_model);
        agents.active_idx = "no-model-agent".to_string();

        assert_eq!(agents.get_active().and_then(|a| a.model.as_ref()), None);
    }
}
