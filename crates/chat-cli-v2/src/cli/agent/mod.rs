pub mod hook;
mod legacy;
mod mcp_config;
mod root_command_args;
mod validator;
pub mod wrapper_types;

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

use crossterm::style::Stylize as _;
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

use super::chat::legacy::tools::{
    DEFAULT_APPROVE,
    ToolMetadata,
    ToolOrigin,
    ToolSpec,
};
use crate::cli::agent::hook::{
    Hook,
    HookTrigger,
};
use crate::constants::DEFAULT_AGENT_NAME;
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::consts::BUILTIN_TOOLS_PREFIX;
use crate::util::paths::PathResolver;
use crate::util::{
    self,
    MCP_SERVER_TOOL_DELIMITER,
    file_uri,
    paths,
};

/// Preferred aliases for all native tools - used for example agent config
const EXAMPLE_AGENT_NATIVE_TOOLS: &[&str] = &[
    ToolMetadata::FS_READ.preferred_alias,
    ToolMetadata::FS_WRITE.preferred_alias,
    ToolMetadata::EXECUTE_COMMAND.preferred_alias,
    ToolMetadata::USE_AWS.preferred_alias,
    ToolMetadata::GH_ISSUE.preferred_alias,
    ToolMetadata::INTROSPECT.preferred_alias,
    ToolMetadata::KNOWLEDGE.preferred_alias,
    ToolMetadata::THINKING.preferred_alias,
    ToolMetadata::TODO.preferred_alias,
    ToolMetadata::DELEGATE.preferred_alias,
    ToolMetadata::GREP.preferred_alias,
    ToolMetadata::GLOB.preferred_alias,
];

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
    Directories(#[from] util::paths::DirectoryError),
    #[error("Encountered io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse legacy mcp config: {0}")]
    BadLegacyMcpConfig(#[from] eyre::Report),
    #[error("File URI not found: {uri} (resolved to {path})")]
    FileUriNotFound { uri: String, path: PathBuf },
    #[error("Failed to read file URI: {uri} (resolved to {path}): {error}")]
    FileUriReadError {
        uri: String,
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("Invalid file URI format: {uri}")]
    InvalidFileUri { uri: String },
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
#[schemars(description = "An Agent is a declarative way of configuring a given instance of kiro-cli.")]
pub struct Agent {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none", default)]
    pub schema: Option<String>,
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
    /// Whether or not to include the legacy global MCP configuration in the agent
    /// You can reference tools brought in by these servers as just as you would with the servers
    /// you configure in the mcpServers field in this config
    #[serde(default, alias = "useLegacyMcpJson")]
    pub include_mcp_json: bool,
    /// The model ID to use for this agent. If not specified, uses the default model.
    #[serde(default)]
    pub model: Option<String>,
    /// Keyboard shortcut for swapping to this agent (e.g., "ctrl+shift+a", "shift+tab")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_shortcut: Option<String>,
    /// Welcome message displayed when switching to this agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub welcome_message: Option<String>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            schema: None,
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
            resources: Vec::new(),
            hooks: Default::default(),
            tools_settings: Default::default(),
            include_mcp_json: true,
            model: None,
            keyboard_shortcut: None,
            welcome_message: None,
            path: None,
        }
    }
}

impl Agent {
    /// Add all tools from schema to allowed_tools with proper formatting
    pub fn add_tools_to_allowed(&mut self, schema: &std::collections::HashMap<String, ToolSpec>) {
        for (tool_name, tool_spec) in schema.iter() {
            match &tool_spec.tool_origin {
                ToolOrigin::Native => {
                    self.allowed_tools.insert(tool_name.clone());
                },
                ToolOrigin::McpServer(server) => {
                    self.allowed_tools
                        .insert(format!("@{server}{MCP_SERVER_TOOL_DELIMITER}{tool_name}"));
                },
            }
        }
    }

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
        self.path = Some(path.to_path_buf());

        // Resolve file:// URIs in the prompt field
        if let Some(resolved_prompt) = self.resolve_prompt()? {
            self.prompt = Some(resolved_prompt);
        }

        let Self { mcp_servers, .. } = self;

        if let (true, Some(legacy_mcp_config)) = (self.include_mcp_json, legacy_mcp_config) {
            for (name, legacy_server) in &legacy_mcp_config.mcp_servers {
                if mcp_servers.mcp_servers.contains_key(name) {
                    let _ = queue!(
                        output,
                        StyledText::warning_fg(),
                        style::Print("WARNING: "),
                        StyledText::reset(),
                        style::Print("MCP server '"),
                        StyledText::success_fg(),
                        style::Print(name),
                        StyledText::reset(),
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
        for allowed_tool in &self.allowed_tools {
            if let Some(settings) = self.tools_settings.get(allowed_tool.as_str()) {
                // currently we only have four native tools that offers tool settings
                let overridden_settings_key = match allowed_tool.as_str() {
                    name if ToolMetadata::FS_READ.aliases.contains(&name)
                        || ToolMetadata::FS_WRITE.aliases.contains(&name) =>
                    {
                        Some("allowedPaths")
                    },
                    name if ToolMetadata::USE_AWS.aliases.contains(&name) => Some("allowedServices"),
                    name if ToolMetadata::EXECUTE_COMMAND.aliases.contains(&name) => Some("allowedCommands"),
                    _ => None,
                };

                if let Some(key) = overridden_settings_key
                    && let Some(ref override_settings) = settings.get(key).map(|value| format!("{key}: {value}"))
                {
                    queue_permission_override_warning(allowed_tool.as_str(), override_settings, output)?;
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

    /// Resolves the prompt field, handling file:// URIs if present.
    /// Returns the prompt content as-is if it doesn't start with file://,
    /// or resolves the file URI and returns the file content.
    pub fn resolve_prompt(&self) -> Result<Option<String>, AgentConfigError> {
        match &self.prompt {
            None => Ok(None),
            Some(prompt_str) => {
                if prompt_str.starts_with("file://") {
                    // Get the base path from the agent config file path
                    let base_path = match &self.path {
                        Some(path) => path.parent().unwrap_or(Path::new(".")),
                        None => Path::new("."),
                    };

                    // Resolve the file URI
                    match file_uri::resolve_file_uri(prompt_str, base_path) {
                        Ok(content) => Ok(Some(content)),
                        Err(file_uri::FileUriError::InvalidUri { uri }) => {
                            Err(AgentConfigError::InvalidFileUri { uri })
                        },
                        Err(file_uri::FileUriError::FileNotFound { path }) => Err(AgentConfigError::FileUriNotFound {
                            uri: prompt_str.clone(),
                            path,
                        }),
                        Err(file_uri::FileUriError::ReadError { path, source }) => {
                            Err(AgentConfigError::FileUriReadError {
                                uri: prompt_str.clone(),
                                path,
                                error: source,
                            })
                        },
                    }
                } else {
                    // Return the prompt as-is for backward compatibility
                    Ok(Some(prompt_str.clone()))
                }
            },
        }
    }

    /// Retrieves an agent by name. It does so via first seeking the given agent under local dir,
    /// and falling back to global dir if it does not exist in local.
    pub async fn get_agent_by_name(os: &Os, agent_name: &str) -> eyre::Result<(Agent, PathBuf)> {
        let resolver = PathResolver::new(os);
        let config_path: Result<PathBuf, PathBuf> = 'config: {
            // local first, and then fall back to looking at global
            let local_config_dir = resolver.workspace().agents_dir()?.join(format!("{agent_name}.json"));
            if os.fs.exists(&local_config_dir) {
                break 'config Ok(local_config_dir);
            }

            let global_config_dir = resolver.global().agents_dir()?.join(format!("{agent_name}.json"));
            if os.fs.exists(&global_config_dir) {
                break 'config Ok(global_config_dir);
            }

            Err(global_config_dir)
        };

        match config_path {
            Ok(config_path) => {
                let content = os.fs.read(&config_path).await?;
                let mut agent = serde_json::from_slice::<Agent>(&content)?;
                let mut stderr = std::io::stderr();
                let legacy_mcp_config = if agent.include_mcp_json {
                    load_legacy_mcp_config(os, &mut stderr).await.unwrap_or(None)
                } else {
                    None
                };
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
            if agent.include_mcp_json && legacy_mcp_config.is_none() {
                let config = load_legacy_mcp_config(os, output).await.unwrap_or_default();
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
        self.include_mcp_json = false;

        // Transform tools: "*" → "@builtin", remove MCP refs
        self.tools = self
            .tools
            .iter()
            .filter_map(|tool| match tool.as_str() {
                "*" => Some(BUILTIN_TOOLS_PREFIX.to_string()),
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
    /// When true, bypasses `allowed_tools` check - all tools are trusted.
    /// When `/tools trust-all` is invoked, this is set AND `allowed_tools` is populated.
    /// When `/tools untrust <tool>` is called, this is set to false so that
    /// `allowed_tools` becomes the source of truth for permission checks.
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
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        if let Some(agent) = self.get_active_mut() {
            agent.allowed_tools.retain(|allowed_tool| {
                for tool_name in tool_names {
                    // Direct string match
                    if allowed_tool == tool_name {
                        return false;
                    }

                    // Check if tool_name and allowed_tool refer to the same native tool via aliases
                    if let Some(tool_info) = ToolMetadata::get_by_any_alias(tool_name) {
                        // Check if allowed_tool is also an alias of the same tool
                        if tool_info.aliases.contains(&allowed_tool.as_str()) {
                            return false;
                        }
                        // Check if allowed_tool is a wildcard pattern that matches any alias
                        let single_pattern: HashSet<String> = [allowed_tool.clone()].into_iter().collect();
                        if tool_info
                            .aliases
                            .iter()
                            .any(|alias| is_tool_in_allowlist(&single_pattern, alias, None))
                        {
                            return false;
                        }
                    }
                }
                true
            });
        }
    }

    pub fn get_active(&self) -> Option<&Agent> {
        self.agents.get(&self.active_idx)
    }

    pub fn get_active_mut(&mut self) -> Option<&mut Agent> {
        self.agents.get_mut(&self.active_idx)
    }

    /// Check if the active agent has a specific tool in its tools list (supports wildcards)
    ///
    /// # Arguments
    /// * `tool_aliases` - List of tool aliases to check
    /// * `is_native` - Whether the tool is a native/builtin tool (used for @builtin matching)
    pub fn has_tool_with_type(&self, tool_aliases: &[&str], is_native: bool) -> bool {
        use crate::util::consts::BUILTIN_TOOLS_PREFIX;
        use crate::util::pattern_matching::matches_any_pattern;

        self.get_active().is_some_and(|agent| {
            let patterns: std::collections::HashSet<&str> = agent.tools.iter().map(|s| s.as_str()).collect();

            // For native tools, check if @builtin matches
            if is_native && matches_any_pattern(&patterns, BUILTIN_TOOLS_PREFIX) {
                return true;
            }

            // Check if any tool alias matches any pattern in the agent's tools list
            tool_aliases.iter().any(|alias| matches_any_pattern(&patterns, alias))
        })
    }

    /// Check if the active agent has a specific tool in its tools list (supports wildcards)
    pub fn has_tool(&self, tool_aliases: &[&str]) -> bool {
        use crate::cli::chat::legacy::is_native_tool;

        // Check if any alias is a native tool
        let is_native = tool_aliases.iter().any(|alias| is_native_tool(alias));
        self.has_tool_with_type(tool_aliases, is_native)
    }

    pub async fn switch(&mut self, name: &str, os: &Os) -> eyre::Result<&Agent> {
        if !self.agents.contains_key(name) {
            eyre::bail!("No agent with name {name} found");
        }
        self.active_idx = name.to_string();

        // Sync resources for the newly active agent
        if let Some(agent) = self.agents.get(name) {
            use crate::util::knowledge_store::KnowledgeStore;
            let _ = KnowledgeStore::sync_agent_resources(agent, os).await;
        }

        self.agents
            .get(name)
            .ok_or(eyre::eyre!("No agent with name {name} found"))
    }

    /// Apply registry filtering to all agents in the collection
    /// This should be called after loading agents when in registry mode
    pub fn apply_registry_filtering(
        &mut self,
        registry: &crate::mcp_registry::McpRegistryResponse,
    ) -> eyre::Result<()> {
        for agent in self.agents.values_mut() {
            crate::mcp_registry::apply_registry_filtering_to_agent(agent, registry)?;
        }
        Ok(())
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
        mcp_api_failure: bool,
    ) -> (Self, AgentsLoadMetadata) {
        if !mcp_enabled {
            let message = if mcp_api_failure {
                "Failed to retrieve MCP settings; MCP functionality disabled\n\n"
            } else {
                "MCP functionality has been disabled by your administrator.\n\n"
            };

            let _ = execute!(
                output,
                StyledText::warning_fg(),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                StyledText::reset(),
                style::Print(message),
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

        let resolver = PathResolver::new(os);
        let mut global_mcp_config = None::<McpServerConfig>;

        let mut local_agents = 'local: {
            // We could be launching from the home dir, in which case the global and local agents
            // are the same set of agents. If that is the case, we simply skip this.
            match (std::env::current_dir(), paths::home_dir(os)) {
                (Ok(cwd), Ok(home_dir)) if cwd == home_dir => break 'local Vec::<Agent>::new(),
                _ => {
                    // noop, we keep going with the extraction of local agents (even if we have an
                    // error retrieving cwd or home_dir)
                },
            }

            let Ok(path) = resolver.workspace().agents_dir() else {
                break 'local Vec::<Agent>::new();
            };
            let Ok(files) = os.fs.read_dir(path).await else {
                break 'local Vec::<Agent>::new();
            };

            let mut agents = Vec::<Agent>::new();
            let results = load_agents_from_entries(files, os, &mut global_mcp_config, mcp_enabled, false, output).await;
            for result in results {
                match result {
                    Ok(agent) => agents.push(agent),
                    Err(e) => {
                        load_metadata.load_failed_count += 1;
                        let _ = queue!(
                            output,
                            StyledText::error_fg(),
                            style::Print("Error: "),
                            StyledText::reset(),
                            style::Print(e),
                            style::Print("\n"),
                        );
                    },
                }
            }

            agents
        };

        let mut global_agents = 'global: {
            let Ok(path) = resolver.global().agents_dir() else {
                break 'global Vec::<Agent>::new();
            };
            let files = match os.fs.read_dir(&path).await {
                Ok(files) => files,
                Err(e) => {
                    if matches!(e.kind(), io::ErrorKind::NotFound)
                        && let Err(e) = os.fs.create_dir_all(&path).await
                    {
                        error!("Error creating global agent dir: {:?}", e);
                    }
                    break 'global Vec::<Agent>::new();
                },
            };

            let mut agents = Vec::<Agent>::new();
            let results = load_agents_from_entries(files, os, &mut global_mcp_config, mcp_enabled, true, output).await;
            for result in results {
                match result {
                    Ok(agent) => agents.push(agent),
                    Err(e) => {
                        load_metadata.load_failed_count += 1;
                        let _ = queue!(
                            output,
                            StyledText::error_fg(),
                            style::Print("Error: "),
                            StyledText::reset(),
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
            let Ok(agents_dir) = resolver.global().agents_dir() else {
                error!("Error obtaining example agent path.");
                break 'example_config;
            };
            let path = agents_dir.join("agent_config.json.example");
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
                    EXAMPLE_AGENT_NATIVE_TOOLS
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
                    StyledText::warning_fg(),
                    style::Print("WARNING: "),
                    StyledText::reset(),
                    style::Print("Agent conflict for "),
                    StyledText::success_fg(),
                    style::Print(name),
                    StyledText::reset(),
                    style::Print(". Using workspace version.\n")
                );
                false
            } else {
                true
            }
        });

        local_agents.append(&mut global_agents);
        let mut all_agents = local_agents;

        // Add default agent
        all_agents.push({
            let mut agent = Agent {
                prompt: Some(include_str!("../../default_agent_prompt.md").to_string()),
                ..Default::default()
            };
            configure_builtin_agent_resources(&mut agent, &resolver).await;
            if mcp_enabled {
                let legacy_mcp_config = load_legacy_mcp_config(os, output).await.unwrap_or(None);
                set_agent_mcp_config(&mut agent, legacy_mcp_config);
            } else {
                agent.mcp_servers = McpServerConfig::default();
            }
            agent
        });

        // Add planner agent (loaded from embedded JSON config)
        all_agents.push({
            let mut agent: Agent =
                serde_json::from_str(include_str!("../../kiro_planner.json")).expect("Invalid kiro_planner.json");
            agent.prompt = Some(include_str!("../../planner_prompt.md").to_string());
            configure_builtin_agent_resources(&mut agent, &resolver).await;
            // Note: Planner agent intentionally does not get MCP tools to keep it read-only
            agent
        });

        let all_agents = validator::validate_agents(all_agents, output);

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
                    StyledText::error_fg(),
                    style::Print("Error"),
                    StyledText::warning_fg(),
                    style::Print(format!(
                        ": no agent with name {name} found. Falling back to user specified default"
                    )),
                    style::Print("\n"),
                    StyledText::reset(),
                );
            }

            if let Some(user_set_default) = os.database.settings.get_string(Setting::ChatDefaultAgent) {
                if all_agents.iter().any(|a| a.name == user_set_default) {
                    break 'active_idx user_set_default;
                }
                let _ = queue!(
                    output,
                    StyledText::error_fg(),
                    style::Print("Error"),
                    StyledText::warning_fg(),
                    style::Print(format!(
                        ": user defined default {user_set_default} not found. Falling back to in-memory default"
                    )),
                    style::Print("\n"),
                    StyledText::reset(),
                );
            }

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

        // Sync resources for the active agent
        if let Some(agent) = agents.get(&active_idx) {
            use crate::util::knowledge_store::KnowledgeStore;
            if let Err(e) = KnowledgeStore::sync_agent_resources(&agent.clone(), os).await {
                let _ = execute!(
                    output,
                    StyledText::warning_fg(),
                    style::Print("Failed to sync resources for active agent: "),
                    StyledText::reset(),
                    style::Print(format!("{e}\n")),
                );
            }
        }

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
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        let tool_trusted = self.get_active().is_some_and(|a| {
            let server_name = match origin {
                ToolOrigin::Native => None,
                ToolOrigin::McpServer(_) => Some(<ToolOrigin as Borrow<str>>::borrow(origin)),
            };

            // For native tools, check if any alias matches allowedTools
            if server_name.is_none()
                && let Some(info) = ToolMetadata::get_by_spec_name(tool_name)
            {
                return info
                    .aliases
                    .iter()
                    .any(|alias| is_tool_in_allowlist(&a.allowed_tools, alias, server_name));
            }

            is_tool_in_allowlist(&a.allowed_tools, tool_name, server_name)
        });

        if tool_trusted || self.trust_all_tools {
            format!("{}", "trusted".dark_green().bold())
        } else {
            self.default_permission_label(tool_name)
        }
    }

    /// Provide default permission labels for the built-in set of tools.
    // This "static" way avoids needing to construct a tool instance.
    fn default_permission_label(&self, tool_name: &str) -> String {
        let label = match tool_name {
            name if ToolMetadata::FS_READ.aliases.contains(&name) => "trust working directory".dark_grey(),
            name if ToolMetadata::FS_WRITE.aliases.contains(&name) => "not trusted".dark_grey(),
            name if ToolMetadata::EXECUTE_COMMAND.aliases.contains(&name) => "not trusted".dark_grey(),
            name if ToolMetadata::USE_AWS.aliases.contains(&name) => "trust read-only commands".dark_grey(),
            name if ToolMetadata::GH_ISSUE.aliases.contains(&name) => "trusted".dark_green().bold(),
            name if ToolMetadata::INTROSPECT.aliases.contains(&name) => "trusted".dark_green().bold(),
            name if ToolMetadata::THINKING.aliases.contains(&name) => "trusted (prerelease)".dark_green().bold(),
            name if ToolMetadata::TODO.aliases.contains(&name) => "trusted".dark_green().bold(),
            name if ToolMetadata::GLOB.aliases.contains(&name) => "trust working directory".dark_grey(),
            name if ToolMetadata::GREP.aliases.contains(&name) => "trust working directory".dark_grey(),
            name if ToolMetadata::USE_SUBAGENT.aliases.contains(&name) => "not trusted".dark_grey(),
            name if ToolMetadata::SWITCH_TO_EXECUTION.aliases.contains(&name) => "trusted".dark_green().bold(),
            name if ToolMetadata::CODE.aliases.contains(&name) => "trust read-only operations".dark_grey(),
            _ if self.trust_all_tools => "trusted".dark_grey().bold(),
            _ => "not trusted".dark_grey(),
        };

        format!("{label}")
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

/// Configure built-in agents with resources
async fn configure_builtin_agent_resources(agent: &mut Agent, resolver: &PathResolver<'_>) {
    agent
        .resources
        .extend(paths::workspace::DEFAULT_AGENT_RESOURCES.iter().map(|&s| s.into()));

    // Add global steering (KIRO-only)
    if let Ok(global_steering_dir) = resolver.global().steering_dir()
        && global_steering_dir.exists()
    {
        let global_steering_pattern = format!("file://{}/**/*.md", global_steering_dir.display());
        agent.resources.push(global_steering_pattern.into());
    }

    // Add workspace steering (KIRO-only)
    if let Ok(workspace_steering_dir) = resolver.workspace().steering_dir()
        && workspace_steering_dir.exists()
    {
        let workspace_steering_pattern = format!("file://{}/**/*.md", workspace_steering_dir.display());
        agent.resources.push(workspace_steering_pattern.into());
    }

    // Add rules pattern if available (only when .amazonq exists but .kiro doesn't)
    if let Some(rules_dir) = resolver.workspace().rules_dir() {
        let rules_pattern = paths::workspace::RULES_PATTERN.replace("{}", &rules_dir.display().to_string());
        agent.resources.push(rules_pattern.into());
    }

    agent.resources.insert(0, "file://AmazonQ.md".into());
}

async fn load_agents_from_entries(
    mut files: ReadDir,
    os: &Os,
    global_mcp_config: &mut Option<McpServerConfig>,
    mcp_enabled: bool,
    is_from_global_dir: bool,
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
            let agent_res = Agent::load(os, file_path, global_mcp_config, mcp_enabled, output).await;
            if let Ok(agent) = &agent_res
                && res.iter().any(|res| match res {
                    Ok(a) => a.name == agent.name,
                    Err(_) => false,
                })
            {
                let _ = queue!(
                    output,
                    StyledText::warning_fg(),
                    style::Print("WARNING: "),
                    StyledText::reset(),
                    style::Print("Duplicate agent with name "),
                    StyledText::success_fg(),
                    style::Print(&agent.name),
                    StyledText::reset(),
                    style::Print(" was found in the "),
                    style::Print(if is_from_global_dir { "global" } else { "workspace" }),
                    style::Print(" directory.\n"),
                    StyledText::reset(),
                );
                continue;
            }
            res.push(agent_res);
        }
    }

    res
}

fn set_agent_mcp_config(agent: &mut Agent, mcp_config: Option<McpServerConfig>) {
    if let Some(config) = mcp_config {
        agent.mcp_servers = config;
    }
}

/// Loads legacy mcp config by combining workspace and global config.
/// In case of a server naming conflict, the workspace config is prioritized.
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};

use mcp_config::McpConfigError;

static MCP_CONFIG_WARNING_SHOWN: AtomicBool = AtomicBool::new(false);

async fn load_legacy_mcp_config(os: &Os, output: &mut impl Write) -> eyre::Result<Option<McpServerConfig>> {
    let resolver = PathResolver::new(os);
    let show_warning = !MCP_CONFIG_WARNING_SHOWN.swap(true, Ordering::Relaxed);

    let global_mcp_path = resolver.global().mcp_config()?;
    let global_mcp_config = match McpServerConfig::load_from_file(os, &global_mcp_path).await {
        Ok(config) => Some(config),
        Err(McpConfigError::Io(e)) => {
            tracing::debug!("Global MCP config not found: {}", e);
            None
        },
        Err(e @ (McpConfigError::JsonParse(_) | McpConfigError::Other(_))) => {
            if show_warning {
                let _ = queue!(
                    output,
                    StyledText::warning_fg(),
                    style::Print("WARNING: "),
                    StyledText::reset(),
                    style::Print("Failed to parse MCP config "),
                    StyledText::success_fg(),
                    style::Print(global_mcp_path.display()),
                    StyledText::reset(),
                    style::Print(": "),
                    style::Print(e.to_string()),
                    style::Print("\n")
                );
            }
            tracing::error!(
                "Error loading global mcp config from {}: {}",
                global_mcp_path.display(),
                e
            );
            None
        },
    };

    let workspace_mcp_path = resolver.workspace().mcp_config()?;
    let workspace_mcp_config = match McpServerConfig::load_from_file(os, &workspace_mcp_path).await {
        Ok(config) => Some(config),
        Err(McpConfigError::Io(e)) => {
            tracing::debug!("Workspace MCP config not found: {}", e);
            None
        },
        Err(e @ (McpConfigError::JsonParse(_) | McpConfigError::Other(_))) => {
            if show_warning {
                let _ = queue!(
                    output,
                    StyledText::warning_fg(),
                    style::Print("WARNING: "),
                    StyledText::reset(),
                    style::Print("Failed to parse MCP config "),
                    StyledText::success_fg(),
                    style::Print(workspace_mcp_path.display()),
                    StyledText::reset(),
                    style::Print(": "),
                    style::Print(e.to_string()),
                    style::Print("\n")
                );
            }
            tracing::error!(
                "Error loading workspace mcp config from {}: {}",
                workspace_mcp_path.display(),
                e
            );
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
        StyledText::warning_fg(),
        style::Print("WARNING: "),
        StyledText::reset(),
        style::Print("You have trusted "),
        StyledText::success_fg(),
        style::Print(tool_name),
        StyledText::reset(),
        style::Print(" tool, which overrides the toolsSettings: "),
        StyledText::brand_fg(),
        style::Print(overridden_settings),
        StyledText::reset(),
        style::Print("\n"),
    )?)
}

// Check if a tool reference is MCP-specific (not @builtin and starts with @)
pub fn is_mcp_tool_ref(s: &str) -> bool {
    // @builtin is not MCP, it's a reference to all built-in tools
    // Any other @ prefix is MCP (e.g., "@git", "@git/git_status")
    !s.starts_with(BUILTIN_TOOLS_PREFIX) && s.starts_with('@')
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
    use std::fs;

    use bstr::ByteSlice;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::cli::agent::hook::Source;
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

    #[tokio::test]
    async fn test_switch() {
        let os = Os::new().await.unwrap();
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
        let result = collection.switch("dev", &os).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "dev");

        // Test switch to non-existent agent
        let result = collection.switch("nonexistent", &os).await;
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
                BUILTIN_TOOLS_PREFIX,
                format!("{}/fs_read", BUILTIN_TOOLS_PREFIX),
                format!("{}/execute_bash", BUILTIN_TOOLS_PREFIX),
                "@git",
                "@git/status",
                "fs_write"
            ],
            "allowedTools": [
                format!("{}/fs_read", BUILTIN_TOOLS_PREFIX),
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
        assert!(agent.tools.contains(&BUILTIN_TOOLS_PREFIX.to_string()));
        assert!(agent.tools.contains(&format!("{}/fs_read", BUILTIN_TOOLS_PREFIX)));
        assert!(agent.tools.contains(&format!("{}/execute_bash", BUILTIN_TOOLS_PREFIX)));
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
        // fs_read has a default of "trust working directory"
        assert!(
            label.contains("trust working directory"),
            "fs_read should show default trusted permission, instead found: {label}"
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
            "trust_all_tools should make everything trusted, instead found: {label}"
        );
    }

    #[test]
    fn test_display_label_default_permissions() {
        let agents = Agents::default();

        // Test default permissions for known tools
        let fs_read_label = agents.display_label("fs_read", &ToolOrigin::Native);
        assert!(
            fs_read_label.contains("trust working directory"),
            "fs_read should be trusted by default, instead found: {fs_read_label}"
        );

        let fs_write_label = agents.display_label("fs_write", &ToolOrigin::Native);
        assert!(
            fs_write_label.contains("not trusted"),
            "fs_write should not be trusted by default, instead found: {fs_write_label}"
        );

        let execute_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        let execute_bash_label = agents.display_label(execute_name, &ToolOrigin::Native);
        assert!(
            execute_bash_label.contains("not trusted"),
            "execute_bash should not be trusted by default, instead found: {execute_bash_label}"
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
            schema: None,
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
            include_mcp_json: false,
            model: None,
            keyboard_shortcut: None,
            welcome_message: None,
            path: None,
        };

        agents.agents.insert("test-agent".to_string(), agent);
        agents.active_idx = "test-agent".to_string();

        // Test 1: Native exact match
        let label = agents.display_label("fs_read", &ToolOrigin::Native);
        assert!(
            label.contains("trusted"),
            "fs_read should be trusted (exact match), instead found: {label}"
        );

        // Test 2: Native wildcard match
        let label = agents.display_label("execute_bash", &ToolOrigin::Native);
        assert!(
            label.contains("trusted"),
            "execute_bash should match execute_* pattern, instead found: {label}"
        );

        // Test 3: Native no match
        let label = agents.display_label("fs_write", &ToolOrigin::Native);
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "fs_write should not be trusted, instead found: {label}"
        );

        // Test 4: MCP server exact match (allows any tool from server1)
        let label = agents.display_label("any_tool", &ToolOrigin::McpServer("server1".to_string()));
        assert!(
            label.contains("trusted"),
            "Server-level permission should allow any tool, instead found: {label}"
        );

        // Test 5: MCP tool exact match
        let label = agents.display_label("specific_tool", &ToolOrigin::McpServer("server2".to_string()));
        assert!(
            label.contains("trusted"),
            "Exact MCP tool should be trusted, instead found: {label}"
        );

        // Test 6: MCP tool wildcard match
        let label = agents.display_label("tool_read", &ToolOrigin::McpServer("server3".to_string()));
        assert!(
            label.contains("trusted"),
            "tool_read should match @server3/tool_* pattern, instead found: {label}"
        );

        // Test 7: MCP tool no match
        let label = agents.display_label("other_tool", &ToolOrigin::McpServer("server2".to_string()));
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "Non-matching MCP tool should not be trusted, instead found: {label}"
        );

        // Test 8: MCP server no match
        let label = agents.display_label("some_tool", &ToolOrigin::McpServer("unknown_server".to_string()));
        assert!(
            !label.contains("trusted") || label.contains("not trusted"),
            "Unknown server should not be trusted, instead found: {label}"
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

    #[test]
    fn test_agent_with_hooks() {
        let agent_json = json!({
            "name": "test-agent",
            "hooks": {
                "agentSpawn": [
                    {
                        "command": "git status"
                    }
                ],
                "preToolUse": [
                    {
                        "matcher": "fs_write",
                        "command": "validate-tool.sh"
                    },
                    {
                        "matcher": "fs_read",
                        "command": "enforce-tdd.sh"
                    }
                ],
                "postToolUse": [
                    {
                        "matcher": "fs_write",
                        "command": "format-python.sh"
                    }
                ]
            }
        });

        let agent: Agent = serde_json::from_value(agent_json).expect("Failed to deserialize agent");

        // Verify agent name
        assert_eq!(agent.name, "test-agent");

        // Verify agentSpawn hook
        assert!(agent.hooks.contains_key(&HookTrigger::AgentSpawn));
        let agent_spawn_hooks = &agent.hooks[&HookTrigger::AgentSpawn];
        assert_eq!(agent_spawn_hooks.len(), 1);
        assert_eq!(agent_spawn_hooks[0].command, "git status");
        assert_eq!(agent_spawn_hooks[0].matcher, None);

        // Verify preToolUse hooks
        assert!(agent.hooks.contains_key(&HookTrigger::PreToolUse));
        let pre_tool_hooks = &agent.hooks[&HookTrigger::PreToolUse];
        assert_eq!(pre_tool_hooks.len(), 2);

        assert_eq!(pre_tool_hooks[0].command, "validate-tool.sh");
        assert_eq!(pre_tool_hooks[0].matcher, Some("fs_write".to_string()));

        assert_eq!(pre_tool_hooks[1].command, "enforce-tdd.sh");
        assert_eq!(pre_tool_hooks[1].matcher, Some("fs_read".to_string()));

        // Verify postToolUse hooks
        assert!(agent.hooks.contains_key(&HookTrigger::PostToolUse));

        // Verify default values are set correctly
        for hooks in agent.hooks.values() {
            for hook in hooks {
                assert_eq!(hook.timeout_ms, 30_000);
                assert_eq!(hook.max_output_size, 10_240);
                assert_eq!(hook.cache_ttl_seconds, 0);
                assert_eq!(hook.source, Source::Agent);
            }
        }
    }

    #[test]
    fn test_resolve_prompt_file_uri_relative() {
        let temp_dir = TempDir::new().unwrap();

        // Create a prompt file
        let prompt_content = "You are a test agent with specific instructions.";
        let prompt_file = temp_dir.path().join("test-prompt.md");
        fs::write(&prompt_file, prompt_content).unwrap();

        // Create agent config file path
        let config_file = temp_dir.path().join("test-agent.json");

        // Create agent with file:// URI prompt
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: Some("file://./test-prompt.md".to_string()),
            path: Some(config_file),
            ..Default::default()
        };

        // Test resolve_prompt
        let resolved = agent.resolve_prompt().unwrap();
        assert_eq!(resolved, Some(prompt_content.to_string()));
    }

    #[test]
    fn test_resolve_prompt_file_uri_absolute() {
        let temp_dir = TempDir::new().unwrap();

        // Create a prompt file
        let prompt_content = "Absolute path prompt content.";
        let prompt_file = temp_dir.path().join("absolute-prompt.md");
        fs::write(&prompt_file, prompt_content).unwrap();

        // Create agent with absolute file:// URI
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: Some(format!("file://{}", prompt_file.display())),
            path: Some(temp_dir.path().join("test-agent.json")),
            ..Default::default()
        };

        // Test resolve_prompt
        let resolved = agent.resolve_prompt().unwrap();
        assert_eq!(resolved, Some(prompt_content.to_string()));
    }

    #[test]
    fn test_resolve_prompt_inline_unchanged() {
        let temp_dir = TempDir::new().unwrap();

        // Create agent with inline prompt
        let inline_prompt = "This is an inline prompt.";
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: Some(inline_prompt.to_string()),
            path: Some(temp_dir.path().join("test-agent.json")),
            ..Default::default()
        };

        // Test resolve_prompt
        let resolved = agent.resolve_prompt().unwrap();
        assert_eq!(resolved, Some(inline_prompt.to_string()));
    }

    #[test]
    fn test_resolve_prompt_file_not_found_error() {
        let temp_dir = TempDir::new().unwrap();

        // Create agent with non-existent file URI
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: Some("file://./nonexistent.md".to_string()),
            path: Some(temp_dir.path().join("test-agent.json")),
            ..Default::default()
        };

        // Test resolve_prompt should fail
        let result = agent.resolve_prompt();
        assert!(result.is_err());

        if let Err(AgentConfigError::FileUriNotFound { uri, .. }) = result {
            assert_eq!(uri, "file://./nonexistent.md");
        } else {
            panic!("Expected FileUriNotFound error, got: {result:?}");
        }
    }

    #[test]
    fn test_resolve_prompt_no_prompt_field() {
        let temp_dir = TempDir::new().unwrap();

        // Create agent without prompt field
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: None,
            path: Some(temp_dir.path().join("test-agent.json")),
            ..Default::default()
        };

        // Test resolve_prompt
        let resolved = agent.resolve_prompt().unwrap();
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_resolve_prompt_no_path_set() {
        // Create agent without path set (should not happen in practice)
        let agent = Agent {
            name: "test-agent".to_string(),
            prompt: Some("file://./test.md".to_string()),
            path: None,
            ..Default::default()
        };

        // Test resolve_prompt should fail gracefully
        let result = agent.resolve_prompt();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_agents_from_entries_warns_duplicate() {
        // Given two agents with the same name
        let os = Os::new().await.unwrap();
        let agents = [
            Agent {
                name: "test-agent".to_string(),
                ..Default::default()
            },
            Agent {
                name: "test-agent".to_string(),
                ..Default::default()
            },
        ];
        for (i, agent) in agents.iter().enumerate() {
            os.fs
                .write(format!("{}_{}.json", agent.name, i), agent.to_str_pretty().unwrap())
                .await
                .unwrap();
        }

        // When we load them
        let mut output = Vec::new();
        let results = load_agents_from_entries(
            os.fs.read_dir(".").await.unwrap(),
            &os,
            &mut None,
            false,
            false,
            &mut output,
        )
        .await;

        // We should see a warning
        assert!(output.contains_str("WARNING"));
        assert!(output.contains_str("test-agent"));
        assert!(output.contains_str("workspace"));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_set_agent_mcp_config() {
        use std::collections::HashMap;

        use crate::cli::chat::legacy::custom_tool::CustomToolConfig;

        let mut agent = Agent::default();
        let mut config = McpServerConfig::default();

        // Add test servers
        config
            .mcp_servers
            .insert("workspace_server".to_string(), CustomToolConfig {
                transport_type: None,
                url: String::new(),
                headers: HashMap::new(),
                oauth_scopes: vec![],
                oauth: None,
                command: "echo".to_string(),
                args: vec!["workspace".to_string()],
                env: None,
                timeout: 120000,
                disabled: false,
                disabled_tools: vec![],
                is_from_legacy_mcp_json: false,
            });
        config
            .mcp_servers
            .insert("global_server".to_string(), CustomToolConfig {
                transport_type: None,
                url: String::new(),
                headers: HashMap::new(),
                oauth_scopes: vec![],
                oauth: None,
                command: "echo".to_string(),
                args: vec!["global".to_string()],
                env: None,
                timeout: 120000,
                disabled: false,
                disabled_tools: vec![],
                is_from_legacy_mcp_json: false,
            });

        set_agent_mcp_config(&mut agent, Some(config));

        // Verify both servers are set
        assert!(agent.mcp_servers.mcp_servers.contains_key("workspace_server"));
        assert!(agent.mcp_servers.mcp_servers.contains_key("global_server"));

        let workspace_server = &agent.mcp_servers.mcp_servers["workspace_server"];
        let global_server = &agent.mcp_servers.mcp_servers["global_server"];
        assert_eq!(workspace_server.args, vec!["workspace".to_string()]);
        assert_eq!(global_server.args, vec!["global".to_string()]);
    }

    #[test]
    fn test_set_agent_mcp_config_with_none() {
        let mut agent = Agent::default();
        let original_servers = agent.mcp_servers.mcp_servers.clone();

        set_agent_mcp_config(&mut agent, None);

        // Should remain unchanged when None is passed
        assert_eq!(agent.mcp_servers.mcp_servers, original_servers);
    }

    #[tokio::test]
    async fn test_default_agent_steering_directories() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let mut os = Os::new().await.unwrap();

        // Set working directory to temp path
        os.env.set_current_dir_for_test(temp_path.to_path_buf());

        // Create steering directories
        let global_steering = temp_path.join("home/.kiro/steering");
        let workspace_steering = temp_path.join(".kiro/steering");
        std::fs::create_dir_all(&global_steering).unwrap();
        std::fs::create_dir_all(&workspace_steering).unwrap();

        // Create test steering files
        std::fs::write(global_steering.join("global.md"), "Global steering").unwrap();
        std::fs::write(workspace_steering.join("workspace.md"), "Workspace steering").unwrap();

        let mut output = Vec::new();
        let (agents, _) = Agents::load(&mut os, None, false, &mut output, false, false).await;

        let default_agent = agents.agents.get("kiro_default").unwrap();

        // Should have steering patterns when directories exist
        let has_steering = default_agent
            .resources
            .iter()
            .any(|r| r.contains(".kiro/steering/**/*.md"));

        assert!(has_steering, "Should include steering directory patterns");
    }

    #[tokio::test]
    async fn test_default_agent_rules_conditional_loading() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let mut os = Os::new().await.unwrap();

        // Set working directory to temp path
        os.env.set_current_dir_for_test(temp_path.to_path_buf());

        // Test without .amazonq directory
        let mut output = Vec::new();
        let (agents, _) = Agents::load(&mut os, None, false, &mut output, false, false).await;
        let default_agent = agents.agents.get("kiro_default").unwrap();

        let has_rules = default_agent.resources.iter().any(|r| r.contains(".amazonq/rules"));
        assert!(
            !has_rules,
            "Should not include rules when .amazonq directory doesn't exist"
        );

        // Create .amazonq directory and test again
        os.fs.create_dir_all(temp_path.join(".amazonq")).await.unwrap();

        // Ensure .kiro directory doesn't exist (rules_dir only returns Some when .amazonq exists but .kiro
        // doesn't)
        let kiro_path = temp_path.join(".kiro");
        if os.fs.exists(&kiro_path) {
            os.fs.remove_dir_all(&kiro_path).await.unwrap();
        }

        let mut output = Vec::new();
        let (agents, _) = Agents::load(&mut os, None, false, &mut output, false, false).await;
        let default_agent = agents.agents.get("kiro_default").unwrap();

        let has_rules = default_agent.resources.iter().any(|r| r.contains(".amazonq/rules"));
        assert!(has_rules, "Should include rules when .amazonq directory exists");
    }

    #[tokio::test]
    async fn test_default_agent_steering_nonexistent_directories() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let mut os = Os::new().await.unwrap();

        // Set working directory to temp path
        os.env.set_current_dir_for_test(temp_path.to_path_buf());

        // Don't create steering directories
        let mut output = Vec::new();
        let (agents, _) = Agents::load(&mut os, None, false, &mut output, false, false).await;
        let default_agent = agents.agents.get("kiro_default").unwrap();

        // Should not have steering patterns when directories don't exist
        let has_steering = default_agent.resources.iter().any(|r| r.contains(".kiro/steering"));

        assert!(!has_steering, "Should not include non-existent steering directories");
    }

    #[test]
    fn test_resource_path_serialization() {
        use crate::cli::agent::wrapper_types::{
            ComplexResource,
            IndexType,
            ResourcePath,
        };

        // Test simple file path
        let simple = ResourcePath::FilePath("file://README.md".to_string());
        let json = serde_json::to_value(&simple).unwrap();
        assert_eq!(json, json!("file://README.md"));

        // Test complex knowledge base with type field
        let complex = ResourcePath::Complex(ComplexResource::KnowledgeBase {
            source: "file://./docs".to_string(),
            name: Some("Documentation".to_string()),
            description: Some("Project docs".to_string()),
            index_type: Some(IndexType::Best),
            include: Some(vec!["**/*.md".to_string()]),
            exclude: None,
            auto_update: Some(true),
        });
        let json = serde_json::to_value(&complex).unwrap();

        // Verify type field is present
        assert_eq!(json["type"], "knowledgeBase");
        assert_eq!(json["source"], "file://./docs");
        assert_eq!(json["name"], "Documentation");
        assert_eq!(json["indexType"], "best");
        assert_eq!(json["autoUpdate"], true);

        // Test deserialization
        let agent_json = json!({
            "name": "test",
            "resources": [
                "file://README.md",
                {
                    "type": "knowledgeBase",
                    "source": "file://./docs",
                    "name": "Docs",
                    "indexType": "fast"
                }
            ]
        });
        let agent: Agent = serde_json::from_value(agent_json).unwrap();
        assert_eq!(agent.resources.len(), 2);
        assert!(matches!(agent.resources[0], ResourcePath::FilePath(_)));
        assert!(matches!(agent.resources[1], ResourcePath::Complex(_)));
    }

    #[test]
    fn test_untrust_tools_with_aliases() {
        let mut agents = Agents::default();
        let mut agent = Agent::default();

        // Test case 1: allowed_tools has "fs_write", untrust "write"
        agent.allowed_tools.insert("fs_write".to_string());
        agent.allowed_tools.insert("fs_read".to_string());
        agents.agents.insert("test".to_string(), agent.clone());
        agents.active_idx = "test".to_string();

        agents.untrust_tools(&["write".to_string()]);
        let active = agents.get_active().unwrap();
        assert!(
            !active.allowed_tools.contains("fs_write"),
            "fs_write should be removed when untrusting 'write'"
        );
        assert!(active.allowed_tools.contains("fs_read"), "fs_read should remain");

        // Test case 2: allowed_tools has "write", untrust "fs_write"
        let mut agent = Agent::default();
        agent.allowed_tools.insert("write".to_string());
        agent.allowed_tools.insert("read".to_string());
        agents.agents.insert("test2".to_string(), agent.clone());
        agents.active_idx = "test2".to_string();

        agents.untrust_tools(&["fs_write".to_string()]);
        let active = agents.get_active().unwrap();
        assert!(
            !active.allowed_tools.contains("write"),
            "write should be removed when untrusting 'fs_write'"
        );
        assert!(active.allowed_tools.contains("read"), "read should remain");

        // Test case 3: wildcard pattern "@builtin/fs*", untrust "write"
        let mut agent = Agent::default();
        agent.allowed_tools.insert("@builtin/fs*".to_string());
        agent.allowed_tools.insert("shell".to_string());
        agents.agents.insert("test3".to_string(), agent.clone());
        agents.active_idx = "test3".to_string();

        agents.untrust_tools(&["write".to_string()]);
        let active = agents.get_active().unwrap();
        assert!(
            !active.allowed_tools.contains("@builtin/fs*"),
            "wildcard pattern should be removed when untrusting 'write'"
        );
        assert!(active.allowed_tools.contains("shell"), "shell should remain");

        // Test case 4: multiple aliases, untrust one
        let mut agent = Agent::default();
        agent.allowed_tools.insert("execute_bash".to_string());
        agent.allowed_tools.insert("fs_write".to_string());
        agents.agents.insert("test4".to_string(), agent.clone());
        agents.active_idx = "test4".to_string();

        agents.untrust_tools(&["shell".to_string()]);
        let active = agents.get_active().unwrap();
        assert!(
            !active.allowed_tools.contains("execute_bash"),
            "execute_bash should be removed when untrusting 'shell'"
        );
        assert!(active.allowed_tools.contains("fs_write"), "fs_write should remain");
    }
}
