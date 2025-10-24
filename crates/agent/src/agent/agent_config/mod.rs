pub mod definitions;
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
    Config,
    HookConfig,
    HookTrigger,
    McpServerConfig,
    McpServers,
    ToolSettings,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs;
use tracing::{
    error,
    info,
    warn,
};

use super::util::directories::{
    global_agents_path,
    legacy_global_mcp_config_path,
};
use crate::agent::util::directories::{
    legacy_workspace_mcp_config_path,
    local_agents_path,
};
use crate::agent::util::error::{
    ErrorContext as _,
    UtilError,
};

/// Represents an agent config.
///
/// Basically just wraps [Config] along with some metadata.
#[derive(Debug, Clone)]
pub struct LoadedAgentConfig {
    /// Where the config was sourced from
    #[allow(dead_code)]
    source: ConfigSource,
    /// The actual config content
    config: Config,
}

impl LoadedAgentConfig {
    pub fn config(&self) -> &Config {
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

    pub fn tool_settings(&self) -> Option<&ToolSettings> {
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
}

/// Where an agent config originated from
#[derive(Debug, Clone)]
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
}

impl Default for LoadedAgentConfig {
    fn default() -> Self {
        Self {
            source: ConfigSource::BuiltIn,
            config: Default::default(),
        }
    }
}

impl LoadedAgentConfig {
    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt()
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

pub async fn load_agents() -> Result<(Vec<LoadedAgentConfig>, Vec<AgentConfigError>)> {
    let mut agent_configs = Vec::new();
    let mut invalid_agents = Vec::new();
    match load_workspace_agents().await {
        Ok((valid, mut invalid)) => {
            if !invalid.is_empty() {
                error!(?invalid, "found invalid workspace agents");
                invalid_agents.append(&mut invalid);
            }
            agent_configs.append(
                &mut valid
                    .into_iter()
                    .map(|(path, config)| LoadedAgentConfig {
                        source: ConfigSource::Workspace { path },
                        config,
                    })
                    .collect(),
            );
        },
        Err(e) => {
            error!(?e, "failed to read local agents");
        },
    };

    match load_global_agents().await {
        Ok((valid, mut invalid)) => {
            if !invalid.is_empty() {
                error!(?invalid, "found invalid global agents");
                invalid_agents.append(&mut invalid);
            }
            agent_configs.append(
                &mut valid
                    .into_iter()
                    .map(|(path, config)| LoadedAgentConfig {
                        source: ConfigSource::Global { path },
                        config,
                    })
                    .collect(),
            );
        },
        Err(e) => {
            error!(?e, "failed to read global agents");
        },
    };

    // Always include the default agent as a fallback.
    agent_configs.push(LoadedAgentConfig::default());

    info!(?agent_configs, "loaded agent config");

    Ok((agent_configs, invalid_agents))
}

pub async fn load_workspace_agents() -> Result<(Vec<(PathBuf, Config)>, Vec<AgentConfigError>)> {
    load_agents_from_dir(local_agents_path()?, true).await
}

pub async fn load_global_agents() -> Result<(Vec<(PathBuf, Config)>, Vec<AgentConfigError>)> {
    load_agents_from_dir(global_agents_path()?, true).await
}

async fn load_agents_from_dir(
    dir: impl AsRef<Path>,
    create_if_missing: bool,
) -> Result<(Vec<(PathBuf, Config)>, Vec<AgentConfigError>)> {
    let dir = dir.as_ref();

    if !dir.exists() && create_if_missing {
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("failed to create agents directory {:?}", &dir))?;
    }

    let mut read_dir = tokio::fs::read_dir(&dir)
        .await
        .with_context(|| format!("failed to read local agents directory {:?}", &dir))?;

    let mut agents: Vec<(PathBuf, Config)> = vec![];
    let mut invalid_agents: Vec<AgentConfigError> = vec![];

    loop {
        match read_dir.next_entry().await {
            Ok(Some(entry)) => {
                let entry_path = entry.path();
                let Ok(md) = entry
                    .metadata()
                    .await
                    .map_err(|e| error!(?e, "failed to read metadata for {:?}", entry_path))
                else {
                    continue;
                };

                if !md.is_file() {
                    warn!("skipping agent for path {:?}: not a file", entry_path);
                }

                let Ok(entry_contents) = tokio::fs::read_to_string(&entry_path)
                    .await
                    .map_err(|e| error!(?e, "failed to read agent config at {:?}", entry_path))
                else {
                    continue;
                };

                match serde_json::from_str(&entry_contents) {
                    Ok(agent) => agents.push((entry_path, agent)),
                    Err(e) => invalid_agents.push(AgentConfigError::InvalidAgentConfig {
                        path: entry_path.to_string_lossy().to_string(),
                        message: e.to_string(),
                    }),
                }
            },
            Ok(None) => break,
            Err(e) => {
                error!(?e, "failed to ready directory entry in {:?}", dir);
                break;
            },
        }
    }

    Ok((agents, invalid_agents))
}

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
    pub async fn from_agent_config(config: &Config) -> LoadedMcpServerConfigs {
        let mut configs = vec![];
        let mut overwritten_configs = vec![];

        let mut agent_configs = config
            .mcp_servers()
            .clone()
            .into_iter()
            .map(|(name, config)| LoadedMcpServerConfig::new(name, config, McpServerConfigSource::AgentConfig))
            .collect::<Vec<_>>();
        configs.append(&mut agent_configs);

        if config.use_legacy_mcp_json() {
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
            if let Ok(path) = legacy_workspace_mcp_config_path() {
                let workspace_configs = load_mcp_config_from_path(path)
                    .await
                    .map_err(|err| warn!(?err, "failed to load workspace mcp configs"))
                    .unwrap_or_default();
                push_configs(workspace_configs, McpServerConfigSource::WorkspaceMcpJson);
            }

            // Load global configs
            if let Ok(path) = legacy_global_mcp_config_path() {
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
    Ok(serde_json::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_agents() {
        let result = load_agents().await;
        println!("{:?}", result);
    }
}
