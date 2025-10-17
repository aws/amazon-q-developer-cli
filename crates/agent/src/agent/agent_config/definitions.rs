use std::collections::{
    HashMap,
    HashSet,
};

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::consts::BUILTIN_VIBER_AGENT_NAME;
use crate::agent::tools::BuiltInToolName;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
// #[serde(tag = "specVersion")]
#[serde(untagged)]
pub enum Config {
    #[serde(rename = "2025_08_22")]
    V2025_08_22(AgentConfigV2025_08_22),
}

impl Default for Config {
    fn default() -> Self {
        Self::V2025_08_22(AgentConfigV2025_08_22::default())
    }
}

impl Config {
    pub fn name(&self) -> &str {
        match self {
            Config::V2025_08_22(a) => a.name.as_str(),
        }
    }

    pub fn system_prompt(&self) -> Option<&str> {
        match self {
            Config::V2025_08_22(a) => a.system_prompt.as_deref(),
        }
    }

    pub fn tools(&self) -> Vec<String> {
        match self {
            Config::V2025_08_22(a) => a.tools.clone(),
        }
    }

    pub fn tool_aliases(&self) -> &HashMap<String, String> {
        match self {
            Config::V2025_08_22(a) => &a.tool_aliases,
        }
    }

    pub fn tool_settings(&self) -> Option<&ToolSettings> {
        match self {
            Config::V2025_08_22(a) => a.tool_settings.as_ref(),
        }
    }

    pub fn allowed_tools(&self) -> &HashSet<String> {
        match self {
            Config::V2025_08_22(a) => &a.allowed_tools,
        }
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        match self {
            Config::V2025_08_22(a) => &a.hooks,
        }
    }

    pub fn resources(&self) -> &Vec<String> {
        match self {
            Config::V2025_08_22(a) => &a.resources,
        }
    }

    pub fn mcp_servers(&self) -> &HashMap<String, McpServerConfig> {
        match self {
            Config::V2025_08_22(a) => &a.mcp_servers,
        }
    }

    pub fn use_legacy_mcp_json(&self) -> bool {
        match self {
            Config::V2025_08_22(a) => a.use_legacy_mcp_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(description = "An Agent is a declarative way of configuring a given instance of q chat.")]
pub struct AgentConfigV2025_08_22 {
    #[serde(rename = "$schema", default = "default_schema")]
    pub schema: String,
    /// Name of the agent.
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
    /// directory
    /// @mcp_server_name/tool_name
    /// #agent_name
    #[serde(default)]
    pub tools: Vec<String>,
    /// Tool aliases for remapping tool names
    #[serde(default)]
    pub tool_aliases: HashMap<String, String>,
    /// Settings for specific tools
    #[serde(default)]
    pub tool_settings: Option<ToolSettings>,
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
    #[serde(default)]
    pub use_legacy_mcp_json: bool,

    // context files
    /// Files to include in the agent's context
    #[serde(default)]
    pub resources: Vec<String>,

    // permissioning stuff
    /// List of tools the agent is explicitly allowed to use
    #[serde(default)]
    pub allowed_tools: HashSet<String>,
}

impl Default for AgentConfigV2025_08_22 {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            name: BUILTIN_VIBER_AGENT_NAME.to_string(),
            description: Some("The default agent for Q CLI".to_string()),
            system_prompt: Some("You are Q, an expert programmer dedicated to becoming the greatest vibe-coding assistant in the world.".to_string()),
            tools: vec!["@builtin".to_string()],
            tool_settings: Default::default(),
            tool_aliases: Default::default(),
            tool_schema: Default::default(),
            hooks: Default::default(),
            model_preferences: Default::default(),
            mcp_servers: Default::default(),
            use_legacy_mcp_json: false,

            resources: Default::default(),
            allowed_tools: HashSet::from([BuiltInToolName::FsRead.to_string()]),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ToolSettings {
    pub file_read: FileReadSettings,
    pub file_write: FileWriteSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FileReadSettings {
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FileWriteSettings {
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
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
    StreamableHTTP(StreamableHTTPMcpServerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
pub struct StreamableHTTPMcpServerConfig {
    /// The URL endpoint for HTTP-based MCP servers
    pub url: String,
    /// HTTP headers to include when communicating with HTTP-based MCP servers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Timeout for each mcp request in ms
    #[serde(alias = "timeout")]
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
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

        let _: Config = serde_json::from_value(agent).unwrap();
    }
}
