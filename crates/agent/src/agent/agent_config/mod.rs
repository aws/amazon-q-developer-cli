pub mod definitions;
pub mod load;
pub mod parse;
pub mod types;

use std::collections::{
    HashMap,
    HashSet,
};
use std::path::{
    Path,
    PathBuf,
};

use definitions::{
    AgentConfig,
    HookConfig,
    HookTrigger,
    McpServerConfig,
    McpServers,
    ToolsSettings,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs;
use tracing::warn;

use crate::agent::util::error::{
    ErrorContext as _,
    UtilError,
};

/// Represents an agent config post-processing and ready for use in the agent loop.
///
/// TODO - add MCP servers as well
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadedAgentConfig {
    /// Where the config was sourced from
    source: ConfigSource,
    /// The actual config content
    config: AgentConfig,
    /// Resolved global prompt content
    #[serde(default)]
    resolved_global_prompt: ResolvedGlobalPrompt,
    /// Content to prepend to the global prompt (e.g. subagent preamble)
    #[serde(default)]
    global_prompt_prefix: Option<String>,
    /// Content to append to the global prompt (e.g. task context)
    #[serde(default)]
    global_prompt_suffix: Option<String>,
}

/// Result of resolving a global prompt from an agent config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ResolvedGlobalPrompt {
    /// No global prompt specified in config
    #[default]
    None,
    /// Successfully resolved (inline text or file:// URI content)
    Resolved(String),
    /// file:// URI failed to resolve (file not found, read error, etc.)
    ResolutionFailed,
}

impl LoadedAgentConfig {
    /// Creates a new LoadedAgentConfig with the given config and source.
    pub fn new(config: AgentConfig, source: ConfigSource, resolved_global_prompt: ResolvedGlobalPrompt) -> Self {
        Self {
            source,
            config,
            resolved_global_prompt,
            global_prompt_prefix: None,
            global_prompt_suffix: None,
        }
    }

    pub fn source(&self) -> &ConfigSource {
        &self.source
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn name(&self) -> &str {
        self.config.name()
    }

    pub fn tools(&self) -> Vec<String> {
        self.config.tools()
    }

    pub fn tool_aliases(&self) -> &HashMap<String, String> {
        self.config.tool_aliases()
    }

    pub fn tool_settings(&self) -> Option<&ToolsSettings> {
        self.config.tool_settings()
    }

    pub fn allowed_tools(&self) -> &HashSet<String> {
        self.config.allowed_tools()
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        self.config.hooks()
    }

    pub fn resources(&self) -> &[impl AsRef<str>] {
        self.config.resources()
    }

    pub fn model(&self) -> Option<&str> {
        self.config.model()
    }

    pub fn set_global_prompt_prefix(&mut self, prefix: impl Into<String>) {
        self.global_prompt_prefix = Some(prefix.into());
    }

    pub fn set_global_prompt_suffix(&mut self, suffix: impl Into<String>) {
        self.global_prompt_suffix = Some(suffix.into());
    }

    pub fn add_hook(&mut self, trigger: definitions::HookTrigger, config: definitions::HookConfig) {
        self.config.add_hook(trigger, config);
    }
}

/// Where an agent config originated from
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ConfigSource {
    /// Config was sourced from a workspace directory
    Workspace { path: PathBuf },
    /// Config was sourced from the global directory
    Global { path: PathBuf },
    /// Config is an in-memory built-in
    ///
    /// This would typically refer to the default agent for new sessions launched without any
    /// custom options, but could include others e.g. a planning/coding/researching agent, etc.
    BuiltIn,
    /// Config was created programmatically at runtime
    #[default]
    Ephemeral,
}

impl LoadedAgentConfig {
    /// Returns the combined global prompt: prefix + base + suffix.
    ///
    /// Returns None if no prompt was specified or if file:// resolution failed.
    pub fn global_prompt(&self) -> Option<String> {
        let base = match &self.resolved_global_prompt {
            ResolvedGlobalPrompt::Resolved(s) => s.as_str(),
            ResolvedGlobalPrompt::None | ResolvedGlobalPrompt::ResolutionFailed => return None,
        };
        let prefix = self.global_prompt_prefix.as_deref().unwrap_or_default();
        let suffix = self.global_prompt_suffix.as_deref().unwrap_or_default();
        Some(format!("{prefix}{base}{suffix}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentConfigError {
    #[error("Agent with the name '{}' was not found", .name)]
    AgentNotFound { name: String },
    #[error("Agent config at the path '{}' has an invalid config: {}", .path, .message)]
    InvalidAgentConfig { path: String, message: String },
    #[error("A failure occurred with the underlying channel")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}

impl From<UtilError> for AgentConfigError {
    fn from(value: UtilError) -> Self {
        Self::Custom(value.to_string())
    }
}

pub use load::{
    build_default_agent,
    load_agents,
};

#[derive(Debug, Clone)]
pub struct LoadedMcpServerConfig {
    /// The name (aka id) to associate with the config
    pub server_name: String,
    /// The mcp server config
    pub config: McpServerConfig,
    /// Where the config originated from
    pub source: McpServerConfigSource,
}

impl LoadedMcpServerConfig {
    fn new(server_name: String, config: McpServerConfig, source: McpServerConfigSource) -> Self {
        Self {
            server_name,
            config,
            source,
        }
    }

    pub fn is_enabled(&self) -> bool {
        match &self.config {
            McpServerConfig::Local(local_mcp_server_config) => !local_mcp_server_config.disabled,
            McpServerConfig::Remote(remote_mcp_server_config) => !remote_mcp_server_config.disabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedMcpServerConfigs {
    /// The configs to use for an agent.
    ///
    /// Each name is guaranteed to be unique - configs dropped due to name conflicts are given in
    /// [Self::overridden_configs].
    pub configs: Vec<LoadedMcpServerConfig>,
    /// Configs not included due to being overridden (e.g., a global config being overridden by a
    /// workspace config).
    pub overridden_configs: Vec<LoadedMcpServerConfig>,
}

impl LoadedMcpServerConfigs {
    /// Loads MCP configs from the given agent config, taking into consideration global and
    /// workspace MCP config files for when the use_legacy_mcp_json field is true.
    ///
    /// TODO: Move this logic into LoadedAgentConfig so that MCP configs are resolved during
    /// agent config loading, not at Agent::new time. This would allow LoadedAgentConfig to
    /// contain fully merged MCP configs from all sources (agent config, global/local mcp.json,
    /// ACP client) and eliminate the need for local_mcp_path/global_mcp_path params on Agent::new.
    pub async fn from_agent_config(
        config: &LoadedAgentConfig,
        local_mcp_path: Option<&PathBuf>,
        global_mcp_path: Option<&PathBuf>,
    ) -> LoadedMcpServerConfigs {
        let mut configs = vec![];
        let mut overwritten_configs = vec![];

        let inner = config.config();
        let mut agent_configs = inner
            .mcp_servers()
            .clone()
            .into_iter()
            .map(|(name, config)| LoadedMcpServerConfig::new(name, config, McpServerConfigSource::AgentConfig))
            .collect::<Vec<_>>();
        configs.append(&mut agent_configs);

        if inner.use_legacy_mcp_json() {
            let mut push_configs = |mcp_servers: McpServers, source: McpServerConfigSource| {
                for (name, config) in mcp_servers.mcp_servers {
                    let config = LoadedMcpServerConfig {
                        server_name: name,
                        config,
                        source,
                    };
                    if configs.iter().any(|c| c.server_name == config.server_name) {
                        overwritten_configs.push(config);
                    } else {
                        configs.push(config);
                    }
                }
            };

            // Load workspace configs
            if let Some(path) = local_mcp_path {
                let workspace_configs = load_mcp_config_from_path(path)
                    .await
                    .map_err(|err| warn!(?err, "failed to load workspace mcp configs"))
                    .unwrap_or_default();
                push_configs(workspace_configs, McpServerConfigSource::WorkspaceMcpJson);
            }

            // Load global configs
            if let Some(path) = global_mcp_path {
                let global_configs = load_mcp_config_from_path(path)
                    .await
                    .map_err(|err| warn!(?err, "failed to load global mcp configs"))
                    .unwrap_or_default();
                push_configs(global_configs, McpServerConfigSource::GlobalMcpJson);
            }
        }

        LoadedMcpServerConfigs {
            configs,
            overridden_configs: overwritten_configs,
        }
    }

    pub fn server_names(&self) -> Vec<String> {
        self.configs.iter().map(|c| c.server_name.clone()).collect()
    }
}

/// Where an [McpServerConfig] originated from
#[derive(Debug, Clone, Copy)]
pub enum McpServerConfigSource {
    /// Config is defined in the agent config
    AgentConfig,
    /// Config is defined in the global mcp.json file
    GlobalMcpJson,
    /// Config is defined in the workspace mcp.json file
    WorkspaceMcpJson,
}

async fn load_mcp_config_from_path(path: impl AsRef<Path>) -> Result<McpServers, UtilError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read MCP config from path {:?}", path.to_string_lossy()))?;

    // Parse the raw JSON first, then deserialize each server entry individually so that
    // unrecognized formats (e.g. `"type": "registry"`) are skipped with a warning instead of
    // causing the entire file to fail.
    let raw: serde_json::Value = serde_json::from_str(&contents)?;
    let servers_obj = raw
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut mcp_servers = HashMap::new();
    for (name, value) in servers_obj {
        match serde_json::from_value::<McpServerConfig>(value) {
            Ok(config) => {
                mcp_servers.insert(name, config);
            },
            Err(err) => {
                warn!(server_name = %name, ?err, "Skipping unrecognized MCP server config entry");
            },
        }
    }

    Ok(McpServers { mcp_servers })
}
