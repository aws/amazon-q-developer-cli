use std::collections::{
    HashMap,
    HashSet,
};

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

use super::types::ResourcePath;
use crate::mcp::oauth_util::OAuthConfig;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AgentConfig {
    #[serde(rename = "2025_08_22")]
    V2025_08_22(AgentConfigV2025_08_22),
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::V2025_08_22(AgentConfigV2025_08_22::default())
    }
}

impl AgentConfig {
    /// Creates an empty config with no allowed tools or resources.
    pub fn new_empty() -> Self {
        Self::V2025_08_22(AgentConfigV2025_08_22 {
            allowed_tools: HashSet::new(),
            resources: Vec::new(),
            ..Default::default()
        })
    }

    pub fn name(&self) -> &str {
        match self {
            AgentConfig::V2025_08_22(a) => a.name.as_str(),
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            AgentConfig::V2025_08_22(a) => a.description.as_deref(),
        }
    }

    pub fn system_prompt(&self) -> Option<&str> {
        match self {
            AgentConfig::V2025_08_22(a) => a.system_prompt.as_deref(),
        }
    }

    pub fn tools(&self) -> Vec<String> {
        match self {
            AgentConfig::V2025_08_22(a) => a.tools.clone(),
        }
    }

    pub fn tool_aliases(&self) -> &HashMap<String, String> {
        match self {
            AgentConfig::V2025_08_22(a) => &a.tool_aliases,
        }
    }

    pub fn tool_settings(&self) -> Option<&ToolsSettings> {
        match self {
            AgentConfig::V2025_08_22(a) => a.tools_settings.as_ref(),
        }
    }

    pub fn allowed_tools(&self) -> &HashSet<String> {
        match self {
            AgentConfig::V2025_08_22(a) => &a.allowed_tools,
        }
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        match self {
            AgentConfig::V2025_08_22(a) => &a.hooks,
        }
    }

    pub fn resources(&self) -> &[impl AsRef<str>] {
        match self {
            AgentConfig::V2025_08_22(a) => a.resources.as_slice(),
        }
    }

    pub fn mcp_servers(&self) -> &HashMap<String, McpServerConfig> {
        match self {
            AgentConfig::V2025_08_22(a) => &a.mcp_servers,
        }
    }

    pub fn use_legacy_mcp_json(&self) -> bool {
        match self {
            AgentConfig::V2025_08_22(a) => a.use_legacy_mcp_json,
        }
    }

    pub fn model(&self) -> Option<&str> {
        match self {
            AgentConfig::V2025_08_22(a) => a.model.as_deref(),
        }
    }

    pub fn append_to_system_prompt(&mut self, incoming: &str) {
        match self {
            AgentConfig::V2025_08_22(a) => {
                if let Some(prompt) = a.system_prompt.as_mut() {
                    prompt.push_str("\n\n");
                    prompt.push_str(incoming);
                }
            },
        }
    }

    pub fn prepend_to_system_prompt(&mut self, incoming: &str) {
        match self {
            AgentConfig::V2025_08_22(a) => {
                if let Some(prompt) = a.system_prompt.as_mut() {
                    let mut new_prompt = format!("{incoming}\n\n{prompt}");
                    std::mem::swap(prompt, &mut new_prompt);
                }
            },
        }
    }

    /// Adds MCP servers to the agent config.
    ///
    /// - If a server name conflicts with an existing one, it is overridden
    /// - Adds `@server_name/*` to the tools list to include all tools from the server
    ///
    /// Returns `Some` with the list of overridden server names, or `None` if no conflicts.
    pub fn add_mcp_servers(
        &mut self,
        servers: impl IntoIterator<Item = (String, McpServerConfig)>,
    ) -> Option<Vec<String>> {
        let mut overridden = Vec::new();

        match self {
            AgentConfig::V2025_08_22(c) => {
                for (name, config) in servers {
                    if c.mcp_servers.contains_key(&name) {
                        overridden.push(name.clone());
                    }
                    c.mcp_servers.insert(name.clone(), config);

                    let tool_pattern = format!("@{}/*", name);
                    if !c.tools.contains(&tool_pattern) {
                        c.tools.push(tool_pattern);
                    }
                }
            },
        }

        if overridden.is_empty() { None } else { Some(overridden) }
    }

    /// Adds a hook to the agent config
    pub fn add_hook(&mut self, trigger: HookTrigger, config: HookConfig) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.hooks.entry(trigger).or_default().push(config);
            },
        }
    }

    /// Sets the tools available to the agent
    pub fn set_tools(&mut self, tools: Vec<String>) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.tools = tools;
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(description = "An Agent is a declarative way of configuring a given instance of kiro-cli.")]
pub struct AgentConfigV2025_08_22 {
    #[serde(rename = "$schema", default = "default_schema")]
    #[schemars(skip)]
    pub schema: String,
    /// Name of the agent.
    #[serde(default)]
    pub name: String,
    /// Human-readable description of what the agent does.
    ///
    /// This field is not passed to the model as context.
    #[serde(default)]
    pub description: Option<String>,
    /// A system prompt for guiding the agent's behavior.
    #[serde(alias = "prompt", default)]
    pub system_prompt: Option<String>,

    // tools
    /// The list of tools available to the agent.
    ///
    /// fs_read
    /// fs_write
    /// @mcp_server_name/tool_name
    /// #agent_name
    #[serde(default)]
    pub tools: Vec<String>,
    /// Tool aliases for remapping tool names
    #[serde(default)]
    pub tool_aliases: HashMap<String, String>,
    /// Settings for specific tools
    #[serde(default)]
    pub tools_settings: Option<ToolsSettings>,
    /// A JSON schema specification describing the arguments for when this agent is invoked as a
    /// tool.
    #[serde(default)]
    pub tool_schema: Option<InputSchema>,

    /// Hooks to add additional context
    #[serde(default)]
    pub hooks: HashMap<HookTrigger, Vec<HookConfig>>,
    /// Preferences for selecting a model the agent uses to generate responses.
    ///
    /// TODO: unimplemented
    #[serde(skip)]
    #[allow(dead_code)]
    pub model_preferences: Option<ModelPreferences>,

    // mcp
    /// Configuration for Model Context Protocol (MCP) servers
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    /// Whether or not to include the legacy ~/.aws/amazonq/mcp.json in the agent
    ///
    /// You can reference tools brought in by these servers as just as you would with the servers
    /// you configure in the mcpServers field in this config
    #[serde(default, alias = "includeMcpJson")]
    pub use_legacy_mcp_json: bool,

    // context files
    /// Files to include in the agent's context
    #[serde(default)]
    pub resources: Vec<ResourcePath>,

    // permissioning stuff
    /// List of tools the agent is explicitly allowed to use
    #[serde(default)]
    pub allowed_tools: HashSet<String>,

    /// The model ID to use for this agent. If not specified, uses the default model.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolsSettings {
    #[serde(default, alias = "read", alias = "fs_read")]
    pub fs_read: FsReadSettings,
    #[serde(default, alias = "write", alias = "fs_write")]
    pub fs_write: FsWriteSettings,
    #[serde(default, alias = "execute_bash", alias = "executeCmd", alias = "execute_cmd")]
    pub shell: ExecuteCmdSettings,
    #[serde(default)]
    pub grep: GrepSettings,
    #[serde(default)]
    pub glob: GlobSettings,
    #[serde(default, alias = "aws")]
    pub use_aws: UseAwsSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteCmdSettings {
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    #[serde(default)]
    pub denied_commands: Vec<String>,
    #[serde(default)]
    pub deny_by_default: bool,
    #[serde(default)]
    pub auto_allow_readonly: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrepSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GlobSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UseAwsSettings {
    #[serde(default)]
    pub allowed_services: Vec<String>,
    #[serde(default)]
    pub denied_services: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_allow_readonly: bool,
}

impl Default for UseAwsSettings {
    fn default() -> Self {
        Self {
            allowed_services: Vec::new(),
            denied_services: Vec::new(),
            auto_allow_readonly: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// This mirrors claude's config set up.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpServers {
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum McpServerConfig {
    Local(LocalMcpServerConfig),
    Remote(RemoteMcpServerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalMcpServerConfig {
    /// The command string used to initialize the mcp server
    pub command: String,
    /// A list of arguments to be used to run the command with
    #[serde(default)]
    pub args: Vec<String>,
    /// A list of environment variables to run the command with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Timeout for each mcp request in ms
    #[serde(alias = "timeout")]
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// A boolean flag to denote whether or not to load this mcp server
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMcpServerConfig {
    /// The URL endpoint for HTTP-based MCP servers
    pub url: String,
    /// HTTP headers to include when communicating with HTTP-based MCP servers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Timeout for each mcp request in ms
    #[serde(alias = "timeout")]
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// OAuth scopes required for authentication with the remote MCP server
    #[serde(default)]
    pub oauth_scopes: Vec<String>,
    /// OAuth configuration for this server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
    /// A boolean flag to denote whether or not to load this mcp server
    #[serde(default)]
    pub disabled: bool,
}

pub fn default_timeout() -> u64 {
    120 * 1000
}

/// The schema specification describing a tool's fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InputSchema(pub serde_json::Value);

// #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
// #[serde(rename_all = "camelCase")]
// pub struct HooksConfig {
//     /// Triggered during agent spawn
//     pub agent_spawn: Vec<HookConfig>,
//
//     /// Triggered per user message submission
//     #[serde(alias = "user_prompt_submit")]
//     pub per_prompt: Vec<HookConfig>,
//
//     /// Triggered before tool execution
//     pub pre_tool_use: Vec<HookConfig>,
//
//     /// Triggered after tool execution
//     pub post_tool_use: Vec<HookConfig>,
// }

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumString, strum::Display, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum HookTrigger {
    /// Triggered during agent spawn
    AgentSpawn,
    /// Triggered per user message submission
    UserPromptSubmit,
    /// Triggered before tool execution
    PreToolUse,
    /// Triggered after tool execution
    PostToolUse,
    /// Triggered when the assistant finishes responding
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum HookConfig {
    /// An external command executed by the system's shell.
    ShellCommand(CommandHook),
    /// A tool hook (unimplemented)
    Tool(ToolHook),
}

impl HookConfig {
    pub fn opts(&self) -> &BaseHookConfig {
        match self {
            HookConfig::ShellCommand(h) => &h.opts,
            HookConfig::Tool(h) => &h.opts,
        }
    }

    pub fn matcher(&self) -> Option<&str> {
        self.opts().matcher.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct CommandHook {
    /// The command to run
    pub command: String,
    #[serde(flatten)]
    pub opts: BaseHookConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ToolHook {
    pub tool_name: String,
    pub args: serde_json::Value,
    #[serde(flatten)]
    pub opts: BaseHookConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BaseHookConfig {
    /// Max time the hook can run before it throws a timeout error
    #[serde(default = "hook_default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max output size of the hook before it is truncated
    #[serde(default = "hook_default_max_output_size")]
    pub max_output_size: usize,

    /// How long the hook output is cached before it will be executed again
    #[serde(default = "hook_default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,

    /// Optional glob matcher for hook
    ///
    /// Currently used for matching tool names for PreToolUse and PostToolUse hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
}

impl Default for BaseHookConfig {
    fn default() -> Self {
        Self {
            timeout_ms: hook_default_timeout_ms(),
            max_output_size: hook_default_max_output_size(),
            cache_ttl_seconds: hook_default_cache_ttl_seconds(),
            matcher: None,
        }
    }
}

fn hook_default_timeout_ms() -> u64 {
    10_000
}

fn hook_default_max_output_size() -> usize {
    1024 * 10
}

fn hook_default_cache_ttl_seconds() -> u64 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelPreferences {
    // hints: Vec<String>,
    cost_priority: Option<f32>,
    speed_priority: Option<f32>,
    intelligence_priority: Option<f32>,
}

fn default_schema() -> String {
    // TODO
    "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_deser() {
        let agent = serde_json::json!({
            "spec_version": "2025_08_22",
            "name": "orchestrator",
            "description": "The orchestrator agent",
        });

        let _: AgentConfig = serde_json::from_value(agent).unwrap();
    }

    #[test]
    fn test_use_legacy_mcp_json_old_name() {
        let agent = serde_json::json!({
            "name": "test",
            "useLegacyMcpJson": true
        });

        let config: AgentConfigV2025_08_22 = serde_json::from_value(agent).unwrap();
        assert!(config.use_legacy_mcp_json);
    }

    #[test]
    fn test_use_legacy_mcp_json_new_name() {
        let agent = serde_json::json!({
            "name": "test",
            "includeMcpJson": true
        });

        let config: AgentConfigV2025_08_22 = serde_json::from_value(agent).unwrap();
        assert!(config.use_legacy_mcp_json);
    }

    #[test]
    fn test_use_legacy_mcp_json_both_names() {
        // When both are present, serde will error as they map to the same field
        let agent = serde_json::json!({
            "name": "test",
            "useLegacyMcpJson": false,
            "includeMcpJson": true
        });

        let result = serde_json::from_value::<AgentConfigV2025_08_22>(agent);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate field"));
    }

    #[test]
    fn test_mcp_server_config_http_deser() {
        // Test HTTP server without oauth scopes
        let config = serde_json::json!({
            "url": "https://mcp.api.coingecko.com/sse"
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://mcp.api.coingecko.com/sse");
                assert!(remote.oauth_scopes.is_empty());
            },
            McpServerConfig::Local(_) => panic!("Expected Remote variant"),
        }

        // Test HTTP server with oauth scopes
        let config = serde_json::json!({
            "url": "https://mcp.datadoghq.com/api/unstable/mcp-server/mcp",
            "oauthScopes": ["mcp", "profile", "email"]
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://mcp.datadoghq.com/api/unstable/mcp-server/mcp");
                assert_eq!(remote.oauth_scopes, vec!["mcp", "profile", "email"]);
            },
            McpServerConfig::Local(_) => panic!("Expected Remote variant"),
        }

        // Test HTTP server with empty oauth scopes
        let config = serde_json::json!({
            "url": "https://example-server.modelcontextprotocol.io/mcp",
            "oauthScopes": []
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://example-server.modelcontextprotocol.io/mcp");
                assert!(remote.oauth_scopes.is_empty());
            },
            McpServerConfig::Local(_) => panic!("Expected Remote variant"),
        }
    }

    #[test]
    fn test_mcp_server_config_stdio_deser() {
        let config = serde_json::json!({
            "command": "node",
            "args": ["server.js"]
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Local(local) => {
                assert_eq!(local.command, "node");
                assert_eq!(local.args, vec!["server.js"]);
            },
            McpServerConfig::Remote(_) => panic!("Expected Local variant"),
        }
    }

    #[test]
    fn test_mcp_server_config_infers_stdio_from_command() {
        let config = serde_json::json!({
            "command": "node",
            "args": ["server.js"]
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Local(local) => {
                assert_eq!(local.command, "node");
                assert_eq!(local.args, vec!["server.js"]);
            },
            McpServerConfig::Remote(_) => panic!("Expected Local variant when command is present"),
        }
    }

    #[test]
    fn test_mcp_server_config_infers_http_from_url() {
        let config = serde_json::json!({
            "url": "https://example.com/mcp"
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://example.com/mcp");
            },
            McpServerConfig::Local(_) => panic!("Expected Remote variant when url is present"),
        }
    }

    #[test]
    fn test_mcp_servers_map_deser() {
        let servers = serde_json::json!({
            "coin-gecko": {
                "url": "https://mcp.api.coingecko.com/sse"
            },
            "datadog": {
                "url": "https://mcp.datadoghq.com/api/unstable/mcp-server/mcp",
                "oauthScopes": ["mcp", "profile", "email"]
            },
            "local-server": {
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
            }
        });

        let result: HashMap<String, McpServerConfig> = serde_json::from_value(servers).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains_key("coin-gecko"));
        assert!(result.contains_key("datadog"));
        assert!(result.contains_key("local-server"));
    }

    #[test]
    fn test_mcp_server_config_with_both_command_and_url() {
        // When both command and url are present, it should deserialize as Local (stdio)
        // since LocalMcpServerConfig will match first with untagged enum
        let config = serde_json::json!({
            "command": "node",
            "url": "https://example.com/mcp"
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Local(local) => {
                assert_eq!(local.command, "node");
            },
            McpServerConfig::Remote(_) => panic!("Expected Local variant when both are present"),
        }
    }

    #[test]
    fn test_mcp_server_config_empty_fails() {
        // Empty config should fail to deserialize
        let config = serde_json::json!({});
        let result: Result<McpServerConfig, _> = serde_json::from_value(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_mcp_servers() {
        let mut config = AgentConfig::default();
        let server = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://example.com/mcp".to_string(),
            headers: HashMap::new(),
            oauth_scopes: Vec::new(),
            timeout_ms: 120_000,
            oauth: None,
            disabled: false,
        });

        let overridden = config.add_mcp_servers(vec![("test-server".to_string(), server)]);
        assert!(overridden.is_none());
        assert!(config.mcp_servers().contains_key("test-server"));
        assert!(config.tools().contains(&"@test-server/*".to_string()));
    }

    #[test]
    fn test_add_mcp_servers_override() {
        let mut config = AgentConfig::default();
        let server1 = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://old.com/mcp".to_string(),
            headers: HashMap::new(),
            oauth_scopes: Vec::new(),
            timeout_ms: 120_000,
            oauth: None,
            disabled: false,
        });
        config.add_mcp_servers(vec![("test-server".to_string(), server1)]);

        let server2 = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://new.com/mcp".to_string(),
            headers: HashMap::new(),
            oauth_scopes: Vec::new(),
            timeout_ms: 120_000,
            oauth: None,
            disabled: false,
        });
        let overridden = config.add_mcp_servers(vec![("test-server".to_string(), server2)]);

        assert_eq!(overridden, Some(vec!["test-server".to_string()]));
        match config.mcp_servers().get("test-server").unwrap() {
            McpServerConfig::Remote(r) => assert_eq!(r.url, "https://new.com/mcp"),
            _ => panic!("Expected Remote"),
        }
    }

    #[test]
    fn test_tools_settings_deser() {
        let agent = serde_json::json!({
            "name": "example",
            "toolsSettings": {
                "shell": {
                    "allowedCommands": ["jj *"]
                }
            }
        });

        let config: AgentConfigV2025_08_22 = serde_json::from_value(agent).unwrap();
        assert!(config.tools_settings.is_some());
        let tools_settings = config.tools_settings.unwrap();
        assert_eq!(tools_settings.shell.allowed_commands, vec!["jj *"]);
    }

    #[test]
    fn test_agent_config_enum_tools_settings_deser() {
        let agent = serde_json::json!({
            "name": "example",
            "toolsSettings": {
                "shell": {
                    "allowedCommands": ["jj *"]
                }
            }
        });

        let config: AgentConfig = serde_json::from_value(agent).unwrap();
        assert!(config.tool_settings().is_some());
        let tools_settings = config.tool_settings().unwrap();
        assert_eq!(tools_settings.shell.allowed_commands, vec!["jj *"]);

        // Also testing for alias
        let agent = serde_json::json!({
            "name": "example",
            "toolsSettings": {
                "executeCmd": {
                    "allowedCommands": ["jj *"]
                }
            }
        });

        let config: AgentConfig = serde_json::from_value(agent).unwrap();
        assert!(config.tool_settings().is_some());
        let tools_settings = config.tool_settings().unwrap();
        assert_eq!(tools_settings.shell.allowed_commands, vec!["jj *"]);
    }

    #[test]
    fn test_real_agent_config_file() {
        let json_str = r#"{
          "name": "example",
          "description": "example agent for testing",
          "prompt": null,
          "mcpServers": {},
          "tools": [
            "read",
            "write",
            "shell"
          ],
          "toolAliases": {},
          "allowedTools": [],
          "resources": [
            "file://AGENTS.md",
            "file://README.md"
          ],
          "hooks": {},
          "toolsSettings": {
            "shell": {
              "allowedCommands": ["jj *"]
            }
          },
          "model": null
        }"#;

        let config: AgentConfig = serde_json::from_str(json_str).unwrap();
        assert!(config.tool_settings().is_some());
        let tools_settings = config.tool_settings().unwrap();
        assert_eq!(tools_settings.shell.allowed_commands, vec!["jj *"]);
    }

    #[test]
    fn test_grep_glob_settings_deser() {
        let agent = serde_json::json!({
            "name": "example",
            "toolsSettings": {
                "grep": {
                    "allowedPaths": ["/home/user"],
                    "deniedPaths": ["/secret"]
                },
                "glob": {
                    "allowedPaths": ["/projects"]
                }
            }
        });

        let config: AgentConfig = serde_json::from_value(agent).unwrap();
        let settings = config.tool_settings().unwrap();
        assert_eq!(settings.grep.allowed_paths, vec!["/home/user"]);
        assert_eq!(settings.grep.denied_paths, vec!["/secret"]);
        assert_eq!(settings.glob.allowed_paths, vec!["/projects"]);
    }
}
