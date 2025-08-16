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
            resources: vec!["file://AmazonQ.md", "file://README.md", "file://.amazonq/rules/**/*.md"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            hooks: Default::default(),
            tools_settings: Default::default(),
            use_legacy_mcp_json: true,
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
    fn thaw(&mut self, path: &Path, legacy_mcp_config: Option<&McpServerConfig>) -> Result<(), AgentConfigError> {
        let Self { mcp_servers, .. } = self;

        self.path = Some(path.to_path_buf());

        if let (true, Some(legacy_mcp_config)) = (self.use_legacy_mcp_json, legacy_mcp_config) {
            let mut stderr = std::io::stderr();
            for (name, legacy_server) in &legacy_mcp_config.mcp_servers {
                if mcp_servers.mcp_servers.contains_key(name) {
                    let _ = queue!(
                        stderr,
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

                agent.thaw(&config_path, legacy_mcp_config.as_ref())?;
                Ok((agent, config_path))
            },
            _ => bail!("Agent {agent_name} does not exist"),
        }
    }

    pub async fn load(
        os: &Os,
        agent_path: impl AsRef<Path>,
        legacy_mcp_config: &mut Option<McpServerConfig>,
    ) -> Result<Agent, AgentConfigError> {
        let content = os.fs.read(&agent_path).await?;
        let mut agent = serde_json::from_slice::<Agent>(&content).map_err(|e| AgentConfigError::InvalidJson {
            error: e,
            path: agent_path.as_ref().to_path_buf(),
        })?;

        if agent.use_legacy_mcp_json && legacy_mcp_config.is_none() {
            let config = load_legacy_mcp_config(os).await.unwrap_or_default();
            if let Some(config) = config {
                legacy_mcp_config.replace(config);
            }
        }

        agent.thaw(agent_path.as_ref(), legacy_mcp_config.as_ref())?;
        Ok(agent)
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
    ) -> (Self, AgentsLoadMetadata) {
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
            let results = load_agents_from_entries(files, os, &mut global_mcp_config).await;
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
            let results = load_agents_from_entries(files, os, &mut global_mcp_config).await;
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
        let tool_trusted = self.get_active().is_some_and(|a| {
            a.allowed_tools.iter().any(|name| {
                // Here the tool names can take the following forms:
                // - @{server_name}{delimiter}{tool_name}
                // - native_tool_name
                name == tool_name && matches!(origin, &ToolOrigin::Native)
                    || name.strip_prefix("@").is_some_and(|remainder| {
                        remainder
                            .split_once(MCP_SERVER_TOOL_DELIMITER)
                            .is_some_and(|(_left, right)| right == tool_name)
                            || remainder == <ToolOrigin as Borrow<str>>::borrow(origin)
                    })
            })
        });

        if tool_trusted || self.trust_all_tools {
            format!("* {}", "trusted".dark_green().bold())
        } else {
            // For custom agents, only show defaults if using the built-in default agent
            let is_default_agent = self
                .get_active()
                .is_some_and(|a| a.path.is_none());
            
            if is_default_agent {
                self.default_permission_label(tool_name)
            } else {
                format!("* {}", "not trusted".dark_grey())
            }
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
            "thinking" => "trusted (prerelease)".dark_green().bold(),
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
) -> Vec<Result<Agent, AgentConfigError>> {
    let mut res = Vec::<Result<Agent, AgentConfigError>>::new();

    while let Ok(Some(file)) = files.next_entry().await {
        let file_path = &file.path();
        if file_path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|s| s == "json")
        {
            res.push(Agent::load(os, file_path, global_mcp_config).await);
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

fn default_schema() -> String {
    "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json".into()
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
}
