pub mod definitions;
pub mod parse;

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

use super::util::directories::legacy_global_mcp_config_path;
use crate::agent::util::directories::{
    legacy_workspace_mcp_config_path,
    local_agents_path,
};
use crate::agent::util::error::{
    ErrorContext as _,
    UtilError,
};
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    new_request_channel,
    respond,
};

#[derive(Debug, Clone)]
pub struct ConfigHandle {
    /// Sender for sending requests to the tool manager task
    sender: RequestSender<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
}

impl ConfigHandle {
    pub async fn get_config(&self, agent_name: &str) -> Result<AgentConfig, AgentConfigError> {
        match self
            .sender
            .send_recv(AgentConfigRequest::GetConfig {
                agent_name: agent_name.to_string(),
            })
            .await
            .unwrap_or(Err(AgentConfigError::Channel))?
        {
            AgentConfigResponse::Config(agent_config) => Ok(agent_config),
            other => {
                error!(?other, "received unexpected response");
                Err(AgentConfigError::Custom("received unexpected response".to_string()))
            },
        }
    }
}

/// Represents an agent config
///
/// Wraps [Config] along with some metadata
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Where the config was sourced from
    source: ConfigSource,
    /// The actual config content
    config: Config,
}

impl AgentConfig {
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

    pub fn tool_settings(&self) -> &ToolSettings {
        self.config.tool_settings()
    }

    pub fn allowed_tools(&self) -> &HashSet<String> {
        self.config.allowed_tools()
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        self.config.hooks()
    }

    pub fn resources(&self) -> &Vec<String> {
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

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            source: ConfigSource::BuiltIn,
            config: Default::default(),
        }
    }
}

impl AgentConfig {
    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt()
    }
}

#[derive(Debug)]
pub struct AgentConfigManager {
    configs: Vec<AgentConfig>,

    request_tx: RequestSender<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
    request_rx: RequestReceiver<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
}

impl AgentConfigManager {
    pub fn new() -> Self {
        let (request_tx, request_rx) = new_request_channel();
        Self {
            configs: Vec::new(),
            request_tx,
            request_rx,
        }
    }

    pub async fn spawn(mut self) -> Result<(ConfigHandle, Vec<AgentConfigError>)> {
        let request_tx_clone = self.request_tx.clone();

        // TODO - return errors back.
        let (configs, errors) = load_agents().await?;
        self.configs = configs;

        tokio::spawn(async move {
            self.run().await;
        });

        Ok((
            ConfigHandle {
                sender: request_tx_clone,
            },
            errors,
        ))
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                req = self.request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("Agent config request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_agent_config_request(req.payload).await;
                    respond!(req, res);
                }
            }
        }
    }

    async fn handle_agent_config_request(
        &mut self,
        req: AgentConfigRequest,
    ) -> Result<AgentConfigResponse, AgentConfigError> {
        match req {
            AgentConfigRequest::GetConfig { agent_name } => {
                let agent_config = self
                    .configs
                    .iter()
                    .find_map(|a| {
                        if a.config.name() == agent_name {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or(AgentConfigError::AgentNotFound { name: agent_name })?;
                Ok(AgentConfigResponse::Config(agent_config))
            },
            AgentConfigRequest::GetAllConfigs => {
                todo!()
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentConfigRequest {
    GetConfig { agent_name: String },
    GetAllConfigs,
}

#[derive(Debug, Clone)]
pub enum AgentConfigResponse {
    Config(AgentConfig),
    AllConfigs {
        configs: Vec<AgentConfig>,
        invalid_configs: Vec<()>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentConfigError {
    #[error("Agent with the name '{}' was not found", .name)]
    AgentNotFound { name: String },
    #[error("Agent config at the path '{}' has an invalid config", .path)]
    InvalidAgentConfig { path: String },
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

pub async fn load_agents() -> Result<(Vec<AgentConfig>, Vec<AgentConfigError>)> {
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
                    .map(|(path, config)| AgentConfig {
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

    // Always include the default agent as a fallback.
    agent_configs.push(AgentConfig::default());

    info!(?agent_configs, "loaded agent config");

    Ok((agent_configs, invalid_agents))
}

pub async fn load_workspace_agents() -> Result<(Vec<(PathBuf, Config)>, Vec<AgentConfigError>)> {
    load_agents_from_dir(local_agents_path()?, true).await
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

#[derive(Debug)]
pub struct LoadedMcpServerConfig {
    /// The name (aka id) to associate with the config
    pub name: String,
    /// The mcp server config
    pub config: McpServerConfig,
    /// Where the config originated from
    pub source: McpServerConfigSource,
}

impl LoadedMcpServerConfig {
    fn new(name: String, config: McpServerConfig, source: McpServerConfigSource) -> Self {
        Self { name, config, source }
    }
}

#[derive(Debug)]
pub struct LoadedMcpServerConfigs {
    /// The configs to use for an agent
    ///
    /// Each name is guaranteed to be unique - configs dropped due to name conflicts are given in
    /// [Self::overwritten_legacy_configs]
    pub configs: Vec<LoadedMcpServerConfig>,
    /// Configs not included due to being overwritten
    pub overwritten_configs: Vec<LoadedMcpServerConfig>,
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

pub async fn load_mcp_configs(config: &Config) -> Result<LoadedMcpServerConfigs, UtilError> {
    let mut configs = vec![];
    let mut overwritten_configs = vec![];

    let mut agent_configs = config
        .mcp_servers()
        .cloned()
        .unwrap_or_default()
        .mcp_servers
        .into_iter()
        .map(|(name, config)| LoadedMcpServerConfig::new(name, config, McpServerConfigSource::AgentConfig))
        .collect::<Vec<_>>();
    configs.append(&mut agent_configs);

    if config.use_legacy_mcp_json() {
        let mut push_configs = |mcp_servers: McpServers, source: McpServerConfigSource| {
            for (name, config) in mcp_servers.mcp_servers {
                let config = LoadedMcpServerConfig { name, config, source };
                if configs.iter().any(|c| c.name == config.name) {
                    overwritten_configs.push(config);
                } else {
                    configs.push(config);
                }
            }
        };

        // Load workspace configs
        let workspace_configs = load_mcp_config_from_path(legacy_workspace_mcp_config_path()?)
            .await
            .map_err(|err| warn!(?err, "failed to load workspace mcp configs"))
            .unwrap_or_default();
        push_configs(workspace_configs, McpServerConfigSource::WorkspaceMcpJson);

        // Load global configs
        let global_configs = load_mcp_config_from_path(legacy_global_mcp_config_path()?)
            .await
            .map_err(|err| warn!(?err, "failed to load global mcp configs"))
            .unwrap_or_default();
        push_configs(global_configs, McpServerConfigSource::GlobalMcpJson);
    }

    Ok(LoadedMcpServerConfigs {
        configs,
        overwritten_configs,
    })
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
    async fn test_load_workspace_agents() {
        let result = load_workspace_agents().await;
        println!("{:?}", result);
    }
}
