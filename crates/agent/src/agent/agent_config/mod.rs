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

    pub fn config_mut(&mut self) -> &mut AgentConfig {
        &mut self.config
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

    pub fn allowed_tools_mut(&mut self) -> &mut HashSet<String> {
        self.config.allowed_tools_mut()
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        self.config.hooks()
    }

    pub fn resources(&self) -> Vec<&types::ResourcePath> {
        self.config.resources()
    }

    /// Returns typed resource paths (needed by `sync_agent_resources`).
    pub fn resource_paths(&self) -> &[types::ResourcePath] {
        self.config.resource_paths()
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

    pub fn add_mcp_servers(
        &mut self,
        servers: impl IntoIterator<Item = (String, definitions::McpServerConfig)>,
    ) -> Option<Vec<String>> {
        self.config.add_mcp_servers(servers)
    }

    pub fn add_resource(&mut self, resource: types::ResourcePath) -> bool {
        self.config.add_resource(resource)
    }

    pub fn remove_resource(&mut self, path: &str) -> bool {
        self.config.remove_resource(path)
    }

    pub fn clear_session_resources(&mut self, original_resources: &[types::ResourcePath]) {
        self.config.clear_session_resources(original_resources);
    }

    pub fn clear_all_resources(&mut self) {
        self.config.clear_all_resources();
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

pub use definitions::default_legacy_oauth_scopes;
pub use load::{
    build_default_agent,
    is_kas_only_agent_config,
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
            McpServerConfig::Registry(_) => false, // placeholder, not launchable
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
    let raw: serde_json::Value =
        serde_json::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::definitions::{
        AgentConfig,
        AgentConfigV2025_08_22,
        LocalMcpServerConfig,
        McpServerConfig,
        RemoteMcpServerConfig,
    };

    fn agent_config_with_mcp() -> AgentConfig {
        let mut v = AgentConfigV2025_08_22 {
            name: "test".to_string(),
            tools: vec!["*".to_string(), "@foo/bar".to_string()],
            use_legacy_mcp_json: true,
            ..Default::default()
        };
        v.mcp_servers.insert(
            "foo".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "/bin/echo".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 5_000,
                disabled: false,
                disabled_tools: vec![],
            }),
        );
        AgentConfig::V2025_08_22(v)
    }

    /// End-to-end: when `clear_mcp_configs` has been applied to the underlying agent config,
    /// `LoadedMcpServerConfigs::from_agent_config` yields an empty set, regardless of whether
    /// legacy-mcp-json loading would have pulled servers in. This is the contract that
    /// guarantees enterprise MCP governance is honored in the V2 TUI code path.
    #[tokio::test]
    async fn test_from_agent_config_empty_after_clear_mcp_configs() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy_path = tmp.path().join("mcp.json");
        std::fs::write(
            &legacy_path,
            r#"{
                "mcpServers": {
                    "legacy-server": {
                        "command": "/bin/echo",
                        "args": []
                    }
                }
            }"#,
        )
        .unwrap();

        // Baseline: without clearing, legacy mcp.json IS loaded and agent-level MCP server IS present.
        let mut cfg_before = agent_config_with_mcp();
        assert!(!cfg_before.mcp_servers().is_empty(), "sanity: pre-clear has servers");
        let loaded_cfg_before =
            LoadedAgentConfig::new(cfg_before.clone(), ConfigSource::Ephemeral, ResolvedGlobalPrompt::None);
        let before = LoadedMcpServerConfigs::from_agent_config(&loaded_cfg_before, None, Some(&legacy_path)).await;
        assert!(
            !before.configs.is_empty(),
            "sanity: pre-clear must load at least one MCP server"
        );

        // Apply clear_mcp_configs (governance disabled).
        cfg_before.clear_mcp_configs();

        // After clearing: no agent-level servers AND use_legacy_mcp_json=false, so legacy
        // mcp.json is NOT consulted — even though the file still exists on disk.
        let loaded_cfg_after = LoadedAgentConfig::new(cfg_before, ConfigSource::Ephemeral, ResolvedGlobalPrompt::None);
        let after = LoadedMcpServerConfigs::from_agent_config(&loaded_cfg_after, None, Some(&legacy_path)).await;
        assert!(
            after.configs.is_empty(),
            "after clear_mcp_configs, no MCP configs should be loaded — got {:?}",
            after.configs
        );
        assert!(
            after.overridden_configs.is_empty(),
            "no overridden configs expected either"
        );
    }

    /// Defense-in-depth: even if a caller sneaks a legacy mcp.json entry past by flipping
    /// `use_legacy_mcp_json` back on (which `clear_mcp_configs` explicitly resets to false),
    /// the agent-level MCP servers stay empty. This ensures the only path that *could* bypass
    /// governance is a caller mutating the cleared config after the fact — which `Agent::new`
    /// and `handle_swap_agent` re-clear when `settings.mcp_enabled == false`.
    #[tokio::test]
    async fn test_clear_mcp_configs_resets_use_legacy_mcp_json_flag() {
        let mut cfg = agent_config_with_mcp();
        assert!(cfg.use_legacy_mcp_json(), "sanity: starts with flag on");
        cfg.clear_mcp_configs();
        assert!(
            !cfg.use_legacy_mcp_json(),
            "clear_mcp_configs must reset use_legacy_mcp_json to false"
        );
        assert!(cfg.mcp_servers().is_empty(), "agent-level MCP servers must be empty");
    }

    #[test]
    fn test_global_prompt_none_when_not_set() {
        let cfg = LoadedAgentConfig::new(
            AgentConfig::default(),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );
        assert_eq!(cfg.global_prompt(), None);
    }

    #[test]
    fn test_global_prompt_none_when_resolution_failed() {
        let cfg = LoadedAgentConfig::new(
            AgentConfig::default(),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::ResolutionFailed,
        );
        assert_eq!(cfg.global_prompt(), None);
    }

    #[test]
    fn test_global_prompt_resolved_base_only() {
        let cfg = LoadedAgentConfig::new(
            AgentConfig::default(),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::Resolved("base prompt".into()),
        );
        assert_eq!(cfg.global_prompt(), Some("base prompt".into()));
    }

    #[test]
    fn test_global_prompt_with_prefix_and_suffix() {
        let mut cfg = LoadedAgentConfig::new(
            AgentConfig::default(),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::Resolved("base".into()),
        );
        cfg.set_global_prompt_prefix("PRE:");
        cfg.set_global_prompt_suffix(":SUF");
        assert_eq!(cfg.global_prompt(), Some("PRE:base:SUF".into()));
    }

    #[test]
    fn test_loaded_agent_config_accessors() {
        let inner = AgentConfigV2025_08_22 {
            name: "myagent".to_string(),
            ..Default::default()
        };
        let cfg = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(inner),
            ConfigSource::Workspace {
                path: PathBuf::from("/tmp"),
            },
            ResolvedGlobalPrompt::None,
        );
        assert_eq!(cfg.name(), "myagent");
        assert!(matches!(cfg.source(), ConfigSource::Workspace { .. }));
        assert!(cfg.tools().is_empty());
        assert!(cfg.tool_aliases().is_empty());
        assert!(cfg.tool_settings().is_none());
        assert!(cfg.allowed_tools().is_empty());
        assert!(cfg.hooks().is_empty());
        assert!(cfg.resources().is_empty());
        assert!(cfg.resource_paths().is_empty());
        assert!(cfg.model().is_none());
    }

    #[test]
    fn test_loaded_agent_config_config_mut() {
        let mut cfg = LoadedAgentConfig::new(
            AgentConfig::default(),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );
        let _m = cfg.config_mut();
        let _am = cfg.allowed_tools_mut();
    }

    #[test]
    fn test_loaded_mcp_server_config_is_enabled_local() {
        let config = LoadedMcpServerConfig::new(
            "test".into(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "/bin/echo".into(),
                args: vec![],
                env: None,
                timeout_ms: 5000,
                disabled: false,
                disabled_tools: vec![],
            }),
            McpServerConfigSource::AgentConfig,
        );
        assert!(config.is_enabled());
    }

    #[test]
    fn test_loaded_mcp_server_config_is_disabled_local() {
        let config = LoadedMcpServerConfig::new(
            "test".into(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "/bin/echo".into(),
                args: vec![],
                env: None,
                timeout_ms: 5000,
                disabled: true,
                disabled_tools: vec![],
            }),
            McpServerConfigSource::AgentConfig,
        );
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_loaded_mcp_server_config_is_enabled_remote() {
        let config = LoadedMcpServerConfig::new(
            "test".into(),
            McpServerConfig::Remote(RemoteMcpServerConfig {
                url: "http://localhost".into(),
                headers: HashMap::new(),
                timeout_ms: 5000,
                oauth_scopes: vec![],
                oauth: None,
                disabled: false,
                disabled_tools: vec![],
            }),
            McpServerConfigSource::GlobalMcpJson,
        );
        assert!(config.is_enabled());
    }

    #[test]
    fn test_loaded_mcp_server_config_is_disabled_remote() {
        let config = LoadedMcpServerConfig::new(
            "test".into(),
            McpServerConfig::Remote(RemoteMcpServerConfig {
                url: "http://localhost".into(),
                headers: HashMap::new(),
                timeout_ms: 5000,
                oauth_scopes: vec![],
                oauth: None,
                disabled: true,
                disabled_tools: vec![],
            }),
            McpServerConfigSource::WorkspaceMcpJson,
        );
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_loaded_mcp_server_configs_server_names() {
        let configs = LoadedMcpServerConfigs {
            configs: vec![
                LoadedMcpServerConfig::new(
                    "server1".into(),
                    McpServerConfig::Local(LocalMcpServerConfig {
                        command: "/bin/echo".into(),
                        args: vec![],
                        env: None,
                        timeout_ms: 5000,
                        disabled: false,
                        disabled_tools: vec![],
                    }),
                    McpServerConfigSource::AgentConfig,
                ),
                LoadedMcpServerConfig::new(
                    "server2".into(),
                    McpServerConfig::Local(LocalMcpServerConfig {
                        command: "/bin/echo".into(),
                        args: vec![],
                        env: None,
                        timeout_ms: 5000,
                        disabled: false,
                        disabled_tools: vec![],
                    }),
                    McpServerConfigSource::AgentConfig,
                ),
            ],
            overridden_configs: vec![],
        };
        let names = configs.server_names();
        assert_eq!(names, vec!["server1", "server2"]);
    }

    #[test]
    fn test_agent_config_error_display() {
        let err = AgentConfigError::AgentNotFound {
            name: "foo".to_string(),
        };
        assert!(err.to_string().contains("foo"));

        let err = AgentConfigError::InvalidAgentConfig {
            path: "/tmp/x".into(),
            message: "bad".into(),
        };
        assert!(err.to_string().contains("/tmp/x"));
        assert!(err.to_string().contains("bad"));

        let err = AgentConfigError::Channel;
        assert!(err.to_string().contains("channel"));

        let err = AgentConfigError::Custom("custom msg".into());
        assert_eq!(err.to_string(), "custom msg");
    }

    #[test]
    fn test_config_source_debug() {
        let sources = vec![
            ConfigSource::Workspace {
                path: PathBuf::from("/tmp"),
            },
            ConfigSource::Global {
                path: PathBuf::from("/home"),
            },
            ConfigSource::BuiltIn,
            ConfigSource::Ephemeral,
        ];
        for s in sources {
            let _ = format!("{:?}", s);
        }
    }

    #[tokio::test]
    async fn test_load_mcp_config_from_path_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(&path, r#"{"mcpServers":{"s1":{"command":"/bin/echo","args":[]}}}"#).unwrap();
        let result = load_mcp_config_from_path(&path).await.unwrap();
        assert!(result.mcp_servers.contains_key("s1"));
    }

    #[tokio::test]
    async fn test_load_mcp_config_from_path_missing_file() {
        let result = load_mcp_config_from_path("/nonexistent/path.json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_mcp_config_from_path_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(&path, "not json").unwrap();
        let result = load_mcp_config_from_path(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_mcp_config_from_path_no_mcp_servers_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(&path, r#"{"other": "value"}"#).unwrap();
        let result = load_mcp_config_from_path(&path).await.unwrap();
        assert!(result.mcp_servers.is_empty());
    }

    #[tokio::test]
    async fn test_load_mcp_config_from_path_skips_unrecognized_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"good":{"command":"/bin/echo","args":[]},"bad":{"unknownField":true}}}"#,
        )
        .unwrap();
        let result = load_mcp_config_from_path(&path).await.unwrap();
        assert!(result.mcp_servers.contains_key("good"));
        assert!(!result.mcp_servers.contains_key("bad"));
    }

    #[tokio::test]
    async fn test_from_agent_config_overrides_duplicate_names() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_path = tmp.path().join("workspace_mcp.json");
        std::fs::write(
            &workspace_path,
            r#"{"mcpServers":{"foo":{"command":"/bin/ls","args":[]}}}"#,
        )
        .unwrap();

        let cfg = agent_config_with_mcp();
        let loaded = LoadedAgentConfig::new(cfg, ConfigSource::Ephemeral, ResolvedGlobalPrompt::None);
        let result = LoadedMcpServerConfigs::from_agent_config(&loaded, Some(&workspace_path), None).await;

        assert!(!result.overridden_configs.is_empty());
    }
}
