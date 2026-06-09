use std::collections::{
    HashMap,
    HashSet,
};

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

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

    pub fn welcome_message(&self) -> Option<&str> {
        match self {
            AgentConfig::V2025_08_22(a) => a.welcome_message.as_deref(),
        }
    }

    pub fn global_prompt(&self) -> Option<&str> {
        match self {
            AgentConfig::V2025_08_22(a) => a.global_prompt.as_deref(),
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

    pub fn allowed_tools_mut(&mut self) -> &mut HashSet<String> {
        match self {
            AgentConfig::V2025_08_22(a) => &mut a.allowed_tools,
        }
    }

    pub fn hooks(&self) -> &HashMap<HookTrigger, Vec<HookConfig>> {
        match self {
            AgentConfig::V2025_08_22(a) => &a.hooks,
        }
    }

    /// Returns context-file resources (excludes knowledgeBase resources which
    /// are handled by the knowledge indexing system).
    pub fn resources(&self) -> Vec<&ResourcePath> {
        match self {
            AgentConfig::V2025_08_22(a) => a.resources.iter().filter(|r| !r.is_knowledge_base()).collect(),
        }
    }

    /// Returns typed resource paths (needed by `sync_agent_resources`).
    pub fn resource_paths(&self) -> &[ResourcePath] {
        match self {
            AgentConfig::V2025_08_22(a) => &a.resources,
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

    pub fn append_to_global_prompt(&mut self, incoming: &str) {
        match self {
            AgentConfig::V2025_08_22(a) => {
                if let Some(prompt) = a.global_prompt.as_mut() {
                    prompt.push_str("\n\n");
                    prompt.push_str(incoming);
                }
            },
        }
    }

    pub fn prepend_to_global_prompt(&mut self, incoming: &str) {
        match self {
            AgentConfig::V2025_08_22(a) => {
                if let Some(prompt) = a.global_prompt.as_mut() {
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

    /// Adds a resource path to the agent config.
    /// Returns false if the resource already exists (duplicate).
    pub fn add_resource(&mut self, resource: ResourcePath) -> bool {
        match self {
            AgentConfig::V2025_08_22(c) => {
                if c.resources.iter().any(|r| r.source() == resource.source()) {
                    false
                } else {
                    c.resources.push(resource);
                    true
                }
            },
        }
    }

    /// Removes a resource path from the agent config by matching the source string
    pub fn remove_resource(&mut self, path: &str) -> bool {
        match self {
            AgentConfig::V2025_08_22(c) => {
                let before = c.resources.len();
                c.resources.retain(|r| r.source() != path);
                c.resources.len() < before
            },
        }
    }

    /// Removes all session-added resources (non-agent-config resources)
    /// Since we can't distinguish at this level, this clears all resources
    pub fn clear_session_resources(&mut self, original_resources: &[ResourcePath]) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.resources
                    .retain(|r| original_resources.iter().any(|orig| orig.source() == r.source()));
            },
        }
    }

    /// Clears all resources from the agent config
    pub fn clear_all_resources(&mut self) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.resources.clear();
            },
        }
    }

    /// Remove MCP servers whose names don't satisfy the predicate.
    pub fn retain_mcp_servers(&mut self, mut f: impl FnMut(&str) -> bool) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.mcp_servers.retain(|name, _| f(name));
            },
        }
    }

    /// Insert MCP server configs without modifying the tools list.
    /// Use this when the tools list already references the server.
    pub fn insert_mcp_servers(&mut self, servers: impl IntoIterator<Item = (String, McpServerConfig)>) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                for (name, config) in servers {
                    c.mcp_servers.insert(name, config);
                }
            },
        }
    }

    /// Override the `use_legacy_mcp_json` / `includeMcpJson` flag.
    pub fn set_use_legacy_mcp_json(&mut self, value: bool) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.use_legacy_mcp_json = value;
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

    /// Clear all MCP server configurations and remove every MCP reference from the
    /// agent config while preserving built-in tools.
    ///
    /// Used to enforce enterprise MCP governance (Kiro console `MCP` toggle set to off)
    /// on already-loaded agent configs.
    ///
    /// Transformations applied:
    /// - `mcp_servers` emptied
    /// - `use_legacy_mcp_json` set to `false`
    /// - `tools`: `"*"` replaced with `"@builtin"`; entries starting with `@` other than `@builtin`
    ///   (i.e. MCP references) dropped.
    /// - `allowed_tools` / `tool_aliases` / `tools_settings`: MCP references dropped.
    pub fn clear_mcp_configs(&mut self) {
        match self {
            AgentConfig::V2025_08_22(c) => {
                c.mcp_servers.clear();
                c.use_legacy_mcp_json = false;

                c.tools = c
                    .tools
                    .iter()
                    .filter_map(|tool| match tool.as_str() {
                        "*" => Some("@builtin".to_string()),
                        t if !is_mcp_tool_ref(t) => Some(t.to_string()),
                        _ => None,
                    })
                    .collect();

                c.allowed_tools.retain(|tool| !is_mcp_tool_ref(tool));
                c.tool_aliases.retain(|orig, _| !is_mcp_tool_ref(orig));
                // `tools_settings` only contains named fields for built-in tools, so no
                // MCP references can live there — nothing to strip.
            },
        }
    }
}

/// Returns `true` if `s` is a reference to an MCP server or MCP tool
/// (e.g. `@my-server` or `@my-server/tool`), as opposed to the built-in tool
/// bucket marker `@builtin` / `@builtin/*`.
fn is_mcp_tool_ref(s: &str) -> bool {
    if s == "@builtin" || s.starts_with("@builtin/") {
        return false;
    }
    s.starts_with('@')
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
    /// A global prompt for guiding the agent's behavior.
    #[serde(rename = "prompt", alias = "systemPrompt", default)]
    pub global_prompt: Option<String>,
    /// Welcome message displayed when the agent is activated.
    #[serde(default, alias = "welcomeMessage")]
    pub welcome_message: Option<String>,

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
    // NOTE: serde alias attributes must stay in sync with ALIAS_GROUPS below.
    // Aliases cover: snake_case (V1 configs), camelCase (serde default), and display names.
    #[serde(default, alias = "read", alias = "fs_read")]
    pub fs_read: FsReadSettings,
    #[serde(default, alias = "write", alias = "fs_write")]
    pub fs_write: FsWriteSettings,
    #[serde(
        default,
        alias = "execute_bash",
        alias = "executeBash",
        alias = "executeCmd",
        alias = "execute_cmd"
    )]
    pub shell: ExecuteCmdSettings,
    #[serde(default)]
    pub grep: GrepSettings,
    #[serde(default)]
    pub glob: GlobSettings,
    #[serde(default, alias = "use_aws", alias = "aws")]
    pub use_aws: UseAwsSettings,
    #[serde(
        default,
        alias = "agent_crew",
        alias = "agentCrew",
        alias = "subagent",
        alias = "use_subagent"
    )]
    pub crew: AgentCrewSettings,
    #[serde(default, alias = "web_fetch")]
    pub web_fetch: WebFetchSettings,
}

impl ToolsSettings {
    /// Groups of JSON keys that are aliases for the same field.
    ///
    /// Used by [`normalize_agent_json`](super::load::normalize_agent_json) to deduplicate
    /// before serde deserialization (which rejects duplicate alias keys).
    /// First element is the canonical name (tool's display name), followed by aliases.
    ///
    /// Aliases cover: snake_case (V1 configs), camelCase (serde default), and display names.
    /// This ensures configs written for V1 or with any alias form work correctly in V2.
    pub const ALIAS_GROUPS: &[&[&str]] = &[
        &["read", "fsRead", "fs_read"],
        &["write", "fsWrite", "fs_write"],
        &["shell", "execute_bash", "executeBash", "executeCmd", "execute_cmd"],
        &["use_aws", "useAws", "aws"],
        &["crew", "agent_crew", "agentCrew", "subagent", "use_subagent"],
        &["web_fetch", "webFetch"],
    ];
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
    #[serde(default)]
    pub allow_read_only: bool,
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
    #[serde(default)]
    pub allow_read_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GlobSettings {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
    #[serde(default)]
    pub allow_read_only: bool,
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

/// Settings for the agent_crew tool controlling which agents can be used in crew pipelines.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCrewSettings {
    /// Which agents can be used as crew stage roles. Supports exact names and glob patterns
    /// (e.g. `"test-*"`). If empty, all agents are available.
    #[serde(default)]
    pub available_agents: Vec<AgentIdentifier>,
    /// Which agents are auto-approved without user confirmation. Supports exact names and glob
    /// patterns.
    #[serde(default, alias = "trustedAgents")]
    pub trusted_agents: Vec<AgentIdentifier>,
}

/// Settings for the web_fetch tool controlling URL-based permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebFetchSettings {
    /// URL regex patterns to auto-allow without prompting.
    #[serde(default)]
    pub trusted: Vec<String>,
    /// URL regex patterns to deny (takes precedence over trusted).
    #[serde(default)]
    pub blocked: Vec<String>,
}

/// Identifies an agent by exact name or glob pattern.
#[derive(Debug, Clone)]
pub enum AgentIdentifier {
    ExactName(String),
    NameGlob(regex::Regex, String),
}

impl AgentIdentifier {
    /// Returns true if this identifier matches the given agent name.
    pub fn matches(&self, name: &str) -> bool {
        match self {
            AgentIdentifier::ExactName(n) => n == name,
            AgentIdentifier::NameGlob(r, _) => r.is_match(name),
        }
    }

    /// Returns true if any identifier in the slice matches the given name.
    pub fn any_matches(identifiers: &[AgentIdentifier], name: &str) -> bool {
        identifiers.iter().any(|id| id.matches(name))
    }
}

impl PartialEq for AgentIdentifier {
    fn eq(&self, other: &AgentIdentifier) -> bool {
        match (self, other) {
            (AgentIdentifier::NameGlob(_, a), AgentIdentifier::NameGlob(_, b)) => a == b,
            (AgentIdentifier::ExactName(a), AgentIdentifier::ExactName(b)) => a == b,
            _ => false,
        }
    }
}

impl<'de> Deserialize<'de> for AgentIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.contains('*') {
            // Convert glob pattern to regex: anchor and replace * with .*
            let regex_str = format!("^{}$", s.replace('*', ".*"));
            let r = regex::Regex::new(&regex_str).map_err(serde::de::Error::custom)?;
            Ok(AgentIdentifier::NameGlob(r, s))
        } else {
            Ok(AgentIdentifier::ExactName(s))
        }
    }
}

impl Serialize for AgentIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AgentIdentifier::ExactName(name) => serializer.serialize_str(name),
            AgentIdentifier::NameGlob(_, pattern) => serializer.serialize_str(pattern),
        }
    }
}

impl JsonSchema for AgentIdentifier {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AgentIdentifier".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        generator.subschema_for::<String>()
    }
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
    /// Placeholder for registry-type servers (`"type": "registry"`).
    /// These are resolved into Local/Remote by `resolve_registry_servers_for_agent_config`
    /// before the agent launches MCP servers.
    Registry(RegistryMcpServerConfig),
}

/// Minimal config that captures `{"type": "registry"}` entries so they survive
/// deserialization. V1 uses a flat struct with all-optional fields; V2 uses an
/// untagged enum, so we need an explicit variant whose required field (`type`)
/// matches the JSON.
///
/// Optional override fields (`env`, `headers`, `timeout`) allow agent.json authors
/// to customise registry servers without losing those values during deserialization.
/// These are merged into the concrete Local/Remote config by
/// `resolve_registry_servers_for_agent_config`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryMcpServerConfig {
    /// Must be `"registry"`.
    #[serde(rename = "type")]
    pub server_type: String,
    /// Optional environment variable overrides (merged on top of registry defaults).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Optional HTTP header overrides for remote servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// Optional timeout override in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Optional OAuth scope overrides (remote servers only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_scopes: Vec<String>,
    /// Optional OAuth client configuration overrides (remote servers only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
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
    /// List of tool names from this server to disable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
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
    /// List of tool names from this server to disable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
}

pub fn default_timeout() -> u64 {
    120 * 1000
}

/// Default OAuth scopes for registry-type MCP servers when no overrides are set.
///
/// Empty scope sets break Dynamic Client Registration on some authorization
/// servers, so we fall back to a standard OpenID Connect scope set.
pub fn default_legacy_oauth_scopes() -> Vec<String> {
    ["openid", "email", "profile", "offline_access"]
        .into_iter()
        .map(String::from)
        .collect()
}

impl McpServerConfig {
    /// Returns the list of disabled tool names for this server.
    pub fn disabled_tools(&self) -> &[String] {
        match self {
            McpServerConfig::Local(c) => &c.disabled_tools,
            McpServerConfig::Remote(c) => &c.disabled_tools,
            McpServerConfig::Registry(_) => &[],
        }
    }

    /// Returns true if this is a registry placeholder that needs resolution.
    pub fn is_registry(&self) -> bool {
        matches!(self, McpServerConfig::Registry(_))
    }

    /// Returns the registry overrides if this is a Registry variant.
    pub fn registry_overrides(&self) -> Option<&RegistryMcpServerConfig> {
        match self {
            McpServerConfig::Registry(r) => Some(r),
            _ => None,
        }
    }
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

#[typeshare]
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
            McpServerConfig::Local(_) | McpServerConfig::Registry(_) => panic!("Expected Remote variant"),
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
            McpServerConfig::Local(_) | McpServerConfig::Registry(_) => panic!("Expected Remote variant"),
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
            McpServerConfig::Local(_) | McpServerConfig::Registry(_) => panic!("Expected Remote variant"),
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
            McpServerConfig::Remote(_) | McpServerConfig::Registry(_) => panic!("Expected Local variant"),
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
            McpServerConfig::Remote(_) | McpServerConfig::Registry(_) => {
                panic!("Expected Local variant when command is present")
            },
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
            McpServerConfig::Local(_) | McpServerConfig::Registry(_) => {
                panic!("Expected Remote variant when url is present")
            },
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
            McpServerConfig::Remote(_) | McpServerConfig::Registry(_) => {
                panic!("Expected Local variant when both are present")
            },
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
            disabled_tools: Vec::new(),
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
            disabled_tools: Vec::new(),
        });
        config.add_mcp_servers(vec![("test-server".to_string(), server1)]);

        let server2 = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://new.com/mcp".to_string(),
            headers: HashMap::new(),
            oauth_scopes: Vec::new(),
            timeout_ms: 120_000,
            oauth: None,
            disabled: false,
            disabled_tools: Vec::new(),
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

    #[test]
    fn test_add_resource() {
        let mut config = AgentConfig::default();
        let resource: ResourcePath = "file://test.md".parse().unwrap();
        assert!(config.add_resource(resource));
        assert_eq!(config.resources().len(), 1);
        assert_eq!(config.resources()[0].as_ref(), "file://test.md");
    }

    #[test]
    fn test_add_resource_dedup() {
        let mut config = AgentConfig::default();
        let r1: ResourcePath = "file://test.md".parse().unwrap();
        let r2: ResourcePath = "file://test.md".parse().unwrap();
        assert!(config.add_resource(r1));
        assert!(!config.add_resource(r2));
        assert_eq!(config.resources().len(), 1);
    }

    #[test]
    fn test_remove_resource() {
        let mut config = AgentConfig::default();
        let r1: ResourcePath = "file://a.md".parse().unwrap();
        let r2: ResourcePath = "file://b.md".parse().unwrap();
        config.add_resource(r1);
        config.add_resource(r2);
        assert_eq!(config.resources().len(), 2);

        let removed = config.remove_resource("file://a.md");
        assert!(removed);
        assert_eq!(config.resources().len(), 1);
        assert_eq!(config.resources()[0].as_ref(), "file://b.md");
    }

    #[test]
    fn test_remove_resource_not_found() {
        let mut config = AgentConfig::default();
        let removed = config.remove_resource("file://nonexistent.md");
        assert!(!removed);
    }

    #[test]
    fn test_clear_session_resources() {
        let mut config = AgentConfig::default();
        let original: ResourcePath = "file://AGENTS.md".parse().unwrap();
        let session: ResourcePath = "file://session.md".parse().unwrap();
        config.add_resource(original.clone());
        config.add_resource(session);
        assert_eq!(config.resources().len(), 2);

        config.clear_session_resources(&[original]);
        assert_eq!(config.resources().len(), 1);
        assert_eq!(config.resources()[0].as_ref(), "file://AGENTS.md");
    }

    #[test]
    fn test_clear_all_resources() {
        let mut config = AgentConfig::default();
        config.add_resource("file://AGENTS.md".parse().unwrap());
        config.add_resource("file://session.md".parse().unwrap());
        assert_eq!(config.resources().len(), 2);

        config.clear_all_resources();
        assert_eq!(config.resources().len(), 0);
    }

    #[test]
    fn test_disabled_tools_deser() {
        let config: LocalMcpServerConfig = serde_json::from_value(serde_json::json!({
            "command": "mcp-server",
            "disabledTools": ["tool_a", "tool_b"]
        }))
        .unwrap();
        assert_eq!(config.disabled_tools, vec!["tool_a", "tool_b"]);
    }

    #[test]
    fn test_disabled_tools_default_empty() {
        let config: LocalMcpServerConfig = serde_json::from_value(serde_json::json!({
            "command": "mcp-server"
        }))
        .unwrap();
        assert!(config.disabled_tools.is_empty());
    }

    #[test]
    fn test_disabled_tools_helper() {
        let local = McpServerConfig::Local(LocalMcpServerConfig {
            command: "cmd".to_string(),
            args: Vec::new(),
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: vec!["tool_a".to_string()],
        });
        assert_eq!(local.disabled_tools(), &["tool_a"]);

        let remote = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://example.com".to_string(),
            headers: HashMap::new(),
            timeout_ms: 120_000,
            oauth_scopes: Vec::new(),
            oauth: None,
            disabled: false,
            disabled_tools: vec!["tool_b".to_string()],
        });
        assert_eq!(remote.disabled_tools(), &["tool_b"]);
    }

    #[test]
    fn test_disabled_tools_skipped_in_serialization_when_empty() {
        let config = LocalMcpServerConfig {
            command: "cmd".to_string(),
            args: Vec::new(),
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: Vec::new(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert!(json.get("disabledTools").is_none());
    }

    #[test]
    fn test_disabled_tools_present_in_serialization_when_nonempty() {
        let config = LocalMcpServerConfig {
            command: "cmd".to_string(),
            args: Vec::new(),
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: vec!["tool_a".to_string()],
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["disabledTools"], serde_json::json!(["tool_a"]));
    }

    #[test]
    fn test_resource_paths_returns_typed_resources() {
        use crate::agent_config::types::ComplexResource;
        let agent: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "test",
            "resources": [
                "file://readme.md",
                {
                    "type": "knowledgeBase",
                    "source": "file://~/docs",
                    "name": "my-docs",
                    "description": "My documentation",
                    "autoUpdate": true,
                    "include": ["**/*.md"],
                    "exclude": ["drafts/**"]
                }
            ]
        }))
        .unwrap();

        let paths = agent.resource_paths();
        assert_eq!(paths.len(), 2);
        assert!(matches!(&paths[0], ResourcePath::FilePath(_)));
        assert!(matches!(
            &paths[1],
            ResourcePath::Complex(ComplexResource::KnowledgeBase { name: Some(n), auto_update: Some(true), .. }) if n == "my-docs"
        ));
    }

    #[test]
    fn test_resources_excludes_knowledge_base() {
        let agent: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "test",
            "resources": [
                "file://readme.md",
                "skill://my-skill",
                {
                    "type": "knowledgeBase",
                    "source": "file://./docs/*.md",
                    "name": "my-docs"
                }
            ]
        }))
        .unwrap();

        // resources() should exclude knowledgeBase entries
        let resources = agent.resources();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].as_ref(), "file://readme.md");
        assert_eq!(resources[1].as_ref(), "skill://my-skill");

        // resource_paths() should include all entries
        assert_eq!(agent.resource_paths().len(), 3);
    }

    #[test]
    fn test_remote_mcp_server_with_oauth_client_id() {
        let config = serde_json::json!({
            "url": "https://mcp.slack.com/mcp",
            "oauth": {
                "clientId": "my-slack-app-id"
            },
            "oauthScopes": ["search:read", "channels:read"]
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert_eq!(
                    remote.oauth.as_ref().unwrap().client_id.as_deref(),
                    Some("my-slack-app-id")
                );
                assert_eq!(remote.oauth_scopes, vec!["search:read", "channels:read"]);
            },
            _ => panic!("Expected Remote variant"),
        }
    }

    #[test]
    fn test_remote_mcp_server_oauth_without_client_id() {
        let config = serde_json::json!({
            "url": "https://example.com/mcp",
            "oauth": {
                "redirectUri": "127.0.0.1:8080"
            }
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match result {
            McpServerConfig::Remote(remote) => {
                assert!(remote.oauth.as_ref().unwrap().client_id.is_none());
                assert_eq!(
                    remote.oauth.as_ref().unwrap().redirect_uri.as_deref(),
                    Some("127.0.0.1:8080")
                );
            },
            _ => panic!("Expected Remote variant"),
        }
    }

    #[test]
    fn test_clear_mcp_configs_removes_servers_and_mcp_refs() {
        let mut config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "test",
            "tools": ["fs_read", "@s1", "@s2/do_thing", "@builtin/grep"],
            "allowedTools": ["fs_read", "@s1/read_file"],
            "toolAliases": {
                "@s1/foo": "foo_alias",
                "fs_read": "read_alias"
            },
            "mcpServers": {
                "s1": { "command": "x", "args": [], "env": {} }
            },
            "useLegacyMcpJson": true
        }))
        .unwrap();

        config.clear_mcp_configs();

        assert!(config.mcp_servers().is_empty(), "mcp_servers not cleared");
        assert!(!config.use_legacy_mcp_json(), "use_legacy_mcp_json not reset");

        // @s1 and @s2/do_thing dropped, @builtin/grep kept, fs_read kept
        let tools = config.tools();
        assert!(tools.contains(&"fs_read".to_string()));
        assert!(tools.contains(&"@builtin/grep".to_string()));
        assert!(!tools.iter().any(|t| t == "@s1"));
        assert!(!tools.iter().any(|t| t == "@s2/do_thing"));

        // allowed_tools MCP ref dropped
        assert!(config.allowed_tools().contains("fs_read"));
        assert!(!config.allowed_tools().contains("@s1/read_file"));

        // tool_aliases MCP ref dropped
        assert!(config.tool_aliases().contains_key("fs_read"));
        assert!(!config.tool_aliases().contains_key("@s1/foo"));
    }

    #[test]
    fn test_clear_mcp_configs_transforms_wildcard_to_builtin() {
        let mut config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "test",
            "tools": ["*", "@s1/tool"],
        }))
        .unwrap();

        config.clear_mcp_configs();

        let tools = config.tools();
        assert!(
            tools.contains(&"@builtin".to_string()),
            "wildcard not rewritten to @builtin"
        );
        assert!(!tools.iter().any(|t| t == "*"), "wildcard should be replaced, not kept");
        assert!(!tools.iter().any(|t| t.starts_with("@s1")));
    }

    #[test]
    fn test_is_mcp_tool_ref_distinguishes_builtin() {
        assert!(!is_mcp_tool_ref("fs_read"));
        assert!(!is_mcp_tool_ref("@builtin"));
        assert!(!is_mcp_tool_ref("@builtin/grep"));
        assert!(!is_mcp_tool_ref("@builtin/*"));
        assert!(is_mcp_tool_ref("@myserver"));
        assert!(is_mcp_tool_ref("@myserver/tool"));
        assert!(is_mcp_tool_ref("@myserver/*"));
    }

    #[test]
    fn test_registry_mcp_server_config_deser_with_overrides() {
        // Minimal registry entry (just type)
        let config = serde_json::json!({"type": "registry"});
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match &result {
            McpServerConfig::Registry(reg) => {
                assert_eq!(reg.server_type, "registry");
                assert!(reg.env.is_none());
                assert!(reg.headers.is_none());
                assert!(reg.timeout.is_none());
            },
            _ => panic!("Expected Registry variant"),
        }

        // Registry entry with env overrides
        let config = serde_json::json!({
            "type": "registry",
            "env": {"MIMER_DISABLE_CVE": "true", "API_TOKEN": "secret123"}
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match &result {
            McpServerConfig::Registry(reg) => {
                let env = reg.env.as_ref().unwrap();
                assert_eq!(env.get("MIMER_DISABLE_CVE").unwrap(), "true");
                assert_eq!(env.get("API_TOKEN").unwrap(), "secret123");
            },
            _ => panic!("Expected Registry variant"),
        }

        // Registry entry with headers and timeout
        let config = serde_json::json!({
            "type": "registry",
            "headers": {"Authorization": "Bearer tok"},
            "timeout": 30000
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match &result {
            McpServerConfig::Registry(reg) => {
                let headers = reg.headers.as_ref().unwrap();
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer tok");
                assert_eq!(reg.timeout, Some(30000));
            },
            _ => panic!("Expected Registry variant"),
        }

        // Full agent.json mcpServers entry with all overrides
        let config = serde_json::json!({
            "type": "registry",
            "env": {"NODE_ENV": "production"},
            "headers": {"X-Custom": "value"},
            "timeout": 60000
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        match &result {
            McpServerConfig::Registry(reg) => {
                assert_eq!(reg.env.as_ref().unwrap().get("NODE_ENV").unwrap(), "production");
                assert_eq!(reg.headers.as_ref().unwrap().get("X-Custom").unwrap(), "value");
                assert_eq!(reg.timeout, Some(60000));
            },
            _ => panic!("Expected Registry variant"),
        }
    }

    #[test]
    fn test_registry_does_not_match_local_or_remote() {
        // Ensure a config with "type": "registry" + env doesn't accidentally match Local
        let config = serde_json::json!({
            "type": "registry",
            "env": {"FOO": "bar"}
        });
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        assert!(result.is_registry());

        // Ensure Local still works with command
        let config = serde_json::json!({"command": "npx", "args": ["-y", "server"]});
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        assert!(!result.is_registry());

        // Ensure Remote still works with url
        let config = serde_json::json!({"url": "https://example.com/mcp"});
        let result: McpServerConfig = serde_json::from_value(config).unwrap();
        assert!(!result.is_registry());
    }

    #[test]
    fn test_hook_trigger_display_and_parse() {
        use std::str::FromStr;
        let cases = [
            (HookTrigger::AgentSpawn, "agentSpawn"),
            (HookTrigger::UserPromptSubmit, "userPromptSubmit"),
            (HookTrigger::PreToolUse, "preToolUse"),
            (HookTrigger::PostToolUse, "postToolUse"),
            (HookTrigger::Stop, "stop"),
        ];
        for (variant, expected) in cases {
            assert_eq!(variant.to_string(), expected);
            assert_eq!(HookTrigger::from_str(expected).unwrap(), variant);
        }
    }

    #[test]
    fn test_hook_trigger_serde_roundtrip() {
        for trigger in [
            HookTrigger::AgentSpawn,
            HookTrigger::UserPromptSubmit,
            HookTrigger::PreToolUse,
            HookTrigger::PostToolUse,
            HookTrigger::Stop,
        ] {
            let json = serde_json::to_value(&trigger).unwrap();
            let back: HookTrigger = serde_json::from_value(json).unwrap();
            assert_eq!(back, trigger);
        }
    }

    #[test]
    fn test_hook_config_shell_command_serde() {
        let json = serde_json::json!({
            "command": "echo hello",
            "timeout_ms": 5000,
            "max_output_size": 2048,
            "cache_ttl_seconds": 60,
            "matcher": "fs_*"
        });
        let hook: HookConfig = serde_json::from_value(json).unwrap();
        match &hook {
            HookConfig::ShellCommand(cmd) => {
                assert_eq!(cmd.command, "echo hello");
                assert_eq!(cmd.opts.timeout_ms, 5000);
                assert_eq!(cmd.opts.max_output_size, 2048);
                assert_eq!(cmd.opts.cache_ttl_seconds, 60);
                assert_eq!(cmd.opts.matcher.as_deref(), Some("fs_*"));
            },
            _ => panic!("Expected ShellCommand"),
        }
        assert_eq!(hook.matcher(), Some("fs_*"));
        // roundtrip
        let rt = serde_json::to_value(&hook).unwrap();
        let back: HookConfig = serde_json::from_value(rt).unwrap();
        assert_eq!(back, hook);
    }

    #[test]
    fn test_hook_config_tool_serde() {
        let json = serde_json::json!({
            "tool_name": "my_tool",
            "args": {"key": "value"}
        });
        let hook: HookConfig = serde_json::from_value(json).unwrap();
        match &hook {
            HookConfig::Tool(t) => {
                assert_eq!(t.tool_name, "my_tool");
                assert_eq!(t.args, serde_json::json!({"key": "value"}));
            },
            _ => panic!("Expected Tool"),
        }
    }

    #[test]
    fn test_hook_config_shell_defaults() {
        let json = serde_json::json!({"command": "ls"});
        let hook: HookConfig = serde_json::from_value(json).unwrap();
        let opts = hook.opts();
        assert_eq!(opts.timeout_ms, 10_000);
        assert_eq!(opts.max_output_size, 1024 * 10);
        assert_eq!(opts.cache_ttl_seconds, 0);
        assert!(opts.matcher.is_none());
    }

    #[test]
    fn test_base_hook_config_default() {
        let d = BaseHookConfig::default();
        assert_eq!(d.timeout_ms, 10_000);
        assert_eq!(d.max_output_size, 10240);
        assert_eq!(d.cache_ttl_seconds, 0);
        assert!(d.matcher.is_none());
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 120_000);
    }

    #[test]
    fn test_default_legacy_oauth_scopes() {
        let scopes = default_legacy_oauth_scopes();
        assert_eq!(scopes, vec!["openid", "email", "profile", "offline_access"]);
    }

    #[test]
    fn test_use_aws_settings_default() {
        let d = UseAwsSettings::default();
        assert!(d.allowed_services.is_empty());
        assert!(d.denied_services.is_empty());
        assert!(d.auto_allow_readonly);
    }

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.name(), "");
        assert!(config.description().is_none());
        assert!(config.welcome_message().is_none());
        assert!(config.global_prompt().is_none());
        assert!(config.tools().is_empty());
        assert!(config.tool_aliases().is_empty());
        assert!(config.tool_settings().is_none());
        assert!(config.allowed_tools().is_empty());
        assert!(config.hooks().is_empty());
        assert!(config.resources().is_empty());
        assert!(config.mcp_servers().is_empty());
        assert!(!config.use_legacy_mcp_json());
        assert!(config.model().is_none());
    }

    #[test]
    fn test_agent_config_new_empty() {
        let config = AgentConfig::new_empty();
        assert!(config.allowed_tools().is_empty());
        assert!(config.resources().is_empty());
    }

    #[test]
    fn test_agent_config_debug() {
        let config = AgentConfig::default();
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("V2025_08_22"));
    }

    #[test]
    fn test_agent_config_accessors_with_values() {
        let config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "my-agent",
            "description": "A test agent",
            "welcomeMessage": "Hello!",
            "prompt": "Be helpful",
            "tools": ["read", "write"],
            "model": "claude-sonnet"
        }))
        .unwrap();
        assert_eq!(config.name(), "my-agent");
        assert_eq!(config.description(), Some("A test agent"));
        assert_eq!(config.welcome_message(), Some("Hello!"));
        assert_eq!(config.global_prompt(), Some("Be helpful"));
        assert_eq!(config.tools(), vec!["read", "write"]);
        assert_eq!(config.model(), Some("claude-sonnet"));
    }

    #[test]
    fn test_append_to_global_prompt() {
        let mut config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "t",
            "prompt": "base"
        }))
        .unwrap();
        config.append_to_global_prompt("extra");
        assert_eq!(config.global_prompt(), Some("base\n\nextra"));
    }

    #[test]
    fn test_append_to_global_prompt_none() {
        let mut config = AgentConfig::default();
        config.append_to_global_prompt("extra");
        // No prompt set, so nothing happens
        assert!(config.global_prompt().is_none());
    }

    #[test]
    fn test_prepend_to_global_prompt() {
        let mut config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "t",
            "prompt": "base"
        }))
        .unwrap();
        config.prepend_to_global_prompt("prefix");
        assert_eq!(config.global_prompt(), Some("prefix\n\nbase"));
    }

    #[test]
    fn test_prepend_to_global_prompt_none() {
        let mut config = AgentConfig::default();
        config.prepend_to_global_prompt("prefix");
        assert!(config.global_prompt().is_none());
    }

    #[test]
    fn test_set_tools() {
        let mut config = AgentConfig::default();
        config.set_tools(vec!["a".into(), "b".into()]);
        assert_eq!(config.tools(), vec!["a", "b"]);
    }

    #[test]
    fn test_allowed_tools_mut() {
        let mut config = AgentConfig::default();
        config.allowed_tools_mut().insert("tool_x".into());
        assert!(config.allowed_tools().contains("tool_x"));
    }

    #[test]
    fn test_add_hook() {
        let mut config = AgentConfig::default();
        let hook = HookConfig::ShellCommand(CommandHook {
            command: "echo hi".into(),
            opts: BaseHookConfig::default(),
        });
        config.add_hook(HookTrigger::AgentSpawn, hook.clone());
        assert_eq!(config.hooks().get(&HookTrigger::AgentSpawn).unwrap().len(), 1);
    }

    #[test]
    fn test_retain_mcp_servers() {
        let mut config = AgentConfig::default();
        let s1 = McpServerConfig::Local(LocalMcpServerConfig {
            command: "a".into(),
            args: vec![],
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: vec![],
        });
        let s2 = McpServerConfig::Local(LocalMcpServerConfig {
            command: "b".into(),
            args: vec![],
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: vec![],
        });
        config.add_mcp_servers(vec![("keep".into(), s1), ("drop".into(), s2)]);
        config.retain_mcp_servers(|name| name == "keep");
        assert!(config.mcp_servers().contains_key("keep"));
        assert!(!config.mcp_servers().contains_key("drop"));
    }

    #[test]
    fn test_insert_mcp_servers_no_tools_modification() {
        let mut config = AgentConfig::default();
        let s = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://x.com".into(),
            headers: HashMap::new(),
            timeout_ms: 120_000,
            oauth_scopes: vec![],
            oauth: None,
            disabled: false,
            disabled_tools: vec![],
        });
        config.insert_mcp_servers(vec![("srv".into(), s)]);
        assert!(config.mcp_servers().contains_key("srv"));
        // tools list should NOT have @srv/* added
        assert!(!config.tools().contains(&"@srv/*".to_string()));
    }

    #[test]
    fn test_set_use_legacy_mcp_json() {
        let mut config = AgentConfig::default();
        assert!(!config.use_legacy_mcp_json());
        config.set_use_legacy_mcp_json(true);
        assert!(config.use_legacy_mcp_json());
    }

    /// Regression test: oauth blocks on registry-type entries must survive
    /// deserialization round-trip.
    #[test]
    fn test_registry_mcp_server_config_preserves_oauth_block() {
        let input = serde_json::json!({
            "type": "registry",
            "oauth": {
                "oauthScopes": ["read:user", "write:user"]
            }
        });

        let parsed: McpServerConfig = serde_json::from_value(input.clone()).unwrap();
        let McpServerConfig::Registry(_reg) = &parsed else {
            panic!("Expected Registry variant, got {:?}", parsed);
        };

        // Round-trip back to JSON and compare.
        let round_tripped = serde_json::to_value(&parsed).unwrap();

        assert_eq!(
            round_tripped.get("oauth"),
            Some(&serde_json::json!({"oauthScopes": ["read:user", "write:user"]})),
            "oauth block must round-trip through Registry variant"
        );
    }

    #[test]
    fn test_agent_identifier_exact_matches() {
        let id: AgentIdentifier = serde_json::from_value(serde_json::json!("my-agent")).unwrap();
        assert!(matches!(&id, AgentIdentifier::ExactName(n) if n == "my-agent"));
        assert!(id.matches("my-agent"));
        assert!(!id.matches("other"));
    }

    #[test]
    fn test_agent_identifier_glob_matches() {
        let id: AgentIdentifier = serde_json::from_value(serde_json::json!("test-*")).unwrap();
        assert!(matches!(&id, AgentIdentifier::NameGlob(_, p) if p == "test-*"));
        assert!(id.matches("test-foo"));
        assert!(id.matches("test-"));
        assert!(!id.matches("other-foo"));
    }

    #[test]
    fn test_agent_identifier_serialize_roundtrip() {
        let exact: AgentIdentifier = serde_json::from_value(serde_json::json!("exact")).unwrap();
        let json = serde_json::to_value(&exact).unwrap();
        assert_eq!(json, serde_json::json!("exact"));

        let glob: AgentIdentifier = serde_json::from_value(serde_json::json!("prefix-*")).unwrap();
        let json = serde_json::to_value(&glob).unwrap();
        assert_eq!(json, serde_json::json!("prefix-*"));
    }

    #[test]
    fn test_agent_identifier_any_matches() {
        let ids: Vec<AgentIdentifier> = serde_json::from_value(serde_json::json!(["exact", "test-*"])).unwrap();
        assert!(AgentIdentifier::any_matches(&ids, "exact"));
        assert!(AgentIdentifier::any_matches(&ids, "test-foo"));
        assert!(!AgentIdentifier::any_matches(&ids, "other"));
    }

    #[test]
    fn test_agent_identifier_partial_eq() {
        let a: AgentIdentifier = serde_json::from_value(serde_json::json!("x")).unwrap();
        let b: AgentIdentifier = serde_json::from_value(serde_json::json!("x")).unwrap();
        let c: AgentIdentifier = serde_json::from_value(serde_json::json!("y")).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);

        let g1: AgentIdentifier = serde_json::from_value(serde_json::json!("a-*")).unwrap();
        let g2: AgentIdentifier = serde_json::from_value(serde_json::json!("a-*")).unwrap();
        let g3: AgentIdentifier = serde_json::from_value(serde_json::json!("b-*")).unwrap();
        assert_eq!(g1, g2);
        assert_ne!(g1, g3);

        // Different variant types are not equal
        assert_ne!(a, g1);
    }

    #[test]
    fn test_resource_path_file_serde_roundtrip() {
        let r: ResourcePath = serde_json::from_value(serde_json::json!("file://test.md")).unwrap();
        assert!(matches!(&r, ResourcePath::FilePath(_)));
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json, serde_json::json!("file://test.md"));
    }

    #[test]
    fn test_resource_path_skill_serde_roundtrip() {
        let r: ResourcePath = serde_json::from_value(serde_json::json!("skill://my-skill")).unwrap();
        assert!(matches!(&r, ResourcePath::Skill(_)));
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json, serde_json::json!("skill://my-skill"));
    }

    #[test]
    fn test_resource_path_complex_serde_roundtrip() {
        let input = serde_json::json!({
            "type": "knowledgeBase",
            "source": "file://./docs",
            "name": "docs",
            "description": "My docs",
            "autoUpdate": true,
            "include": ["*.md"],
            "exclude": ["drafts/*"]
        });
        let r: ResourcePath = serde_json::from_value(input.clone()).unwrap();
        assert!(matches!(&r, ResourcePath::Complex(_)));
        assert_eq!(r.source(), "file://./docs");
        assert!(r.is_knowledge_base());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["source"], "file://./docs");
        assert_eq!(json["name"], "docs");
    }

    #[test]
    fn test_resource_path_invalid_prefix() {
        let r = serde_json::from_value::<ResourcePath>(serde_json::json!("https://bad"));
        assert!(r.is_err());
    }

    #[test]
    fn test_mcp_server_config_registry_overrides_helper() {
        let reg = McpServerConfig::Registry(RegistryMcpServerConfig {
            server_type: "registry".into(),
            env: Some(HashMap::from([("K".into(), "V".into())])),
            headers: None,
            timeout: Some(5000),
            oauth_scopes: vec![],
            oauth: None,
        });
        let overrides = reg.registry_overrides().unwrap();
        assert_eq!(overrides.timeout, Some(5000));
        assert!(overrides.env.as_ref().unwrap().contains_key("K"));

        // Non-registry returns None
        let local = McpServerConfig::Local(LocalMcpServerConfig {
            command: "x".into(),
            args: vec![],
            env: None,
            timeout_ms: 120_000,
            disabled: false,
            disabled_tools: vec![],
        });
        assert!(local.registry_overrides().is_none());
    }

    #[test]
    fn test_mcp_server_disabled_tools_registry() {
        let reg = McpServerConfig::Registry(RegistryMcpServerConfig {
            server_type: "registry".into(),
            env: None,
            headers: None,
            timeout: None,
            oauth_scopes: vec![],
            oauth: None,
        });
        assert!(reg.disabled_tools().is_empty());
    }

    #[test]
    fn test_local_mcp_server_timeout_alias() {
        // "timeout" alias should map to timeout_ms
        let config: LocalMcpServerConfig = serde_json::from_value(serde_json::json!({
            "command": "srv",
            "timeout": 5000
        }))
        .unwrap();
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn test_remote_mcp_server_timeout_alias() {
        let config: RemoteMcpServerConfig = serde_json::from_value(serde_json::json!({
            "url": "https://x.com",
            "timeout": 3000
        }))
        .unwrap();
        assert_eq!(config.timeout_ms, 3000);
    }

    #[test]
    fn test_local_mcp_server_disabled_flag() {
        let config: LocalMcpServerConfig = serde_json::from_value(serde_json::json!({
            "command": "srv",
            "disabled": true
        }))
        .unwrap();
        assert!(config.disabled);
    }

    #[test]
    fn test_remote_mcp_server_disabled_flag() {
        let config: RemoteMcpServerConfig = serde_json::from_value(serde_json::json!({
            "url": "https://x.com",
            "disabled": true
        }))
        .unwrap();
        assert!(config.disabled);
    }

    #[test]
    fn test_tools_settings_aliases_deser() {
        // V1-style snake_case aliases
        let agent: AgentConfigV2025_08_22 = serde_json::from_value(serde_json::json!({
            "name": "t",
            "toolsSettings": {
                "fs_read": { "allowedPaths": ["/a"] },
                "fs_write": { "deniedPaths": ["/b"] },
                "execute_bash": { "denyByDefault": true },
                "aws": { "allowedServices": ["s3"] },
                "agent_crew": { "availableAgents": ["test-*"] }
            }
        }))
        .unwrap();
        let ts = agent.tools_settings.unwrap();
        assert_eq!(ts.fs_read.allowed_paths, vec!["/a"]);
        assert_eq!(ts.fs_write.denied_paths, vec!["/b"]);
        assert!(ts.shell.deny_by_default);
        assert_eq!(ts.use_aws.allowed_services, vec!["s3"]);
        assert_eq!(ts.crew.available_agents.len(), 1);
    }

    #[test]
    fn test_agent_config_serde_roundtrip() {
        let config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "roundtrip",
            "description": "desc",
            "prompt": "be nice",
            "tools": ["read"],
            "allowedTools": ["read"],
            "model": "fast"
        }))
        .unwrap();
        let json = serde_json::to_value(&config).unwrap();
        let back: AgentConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back.name(), "roundtrip");
        assert_eq!(back.model(), Some("fast"));
    }

    #[test]
    fn test_hooks_in_full_config() {
        let config: AgentConfig = serde_json::from_value(serde_json::json!({
            "name": "t",
            "hooks": {
                "agentSpawn": [{"command": "echo spawn"}],
                "stop": [{"command": "echo done"}]
            }
        }))
        .unwrap();
        assert_eq!(config.hooks().len(), 2);
        assert!(config.hooks().contains_key(&HookTrigger::AgentSpawn));
        assert!(config.hooks().contains_key(&HookTrigger::Stop));
    }

    #[test]
    fn test_default_schema_value() {
        // Default::default() gives empty string; serde default gives the URL
        let config = AgentConfigV2025_08_22::default();
        assert_eq!(config.schema, "");
        // When deserialized without $schema, serde uses default_schema()
        let config: AgentConfigV2025_08_22 = serde_json::from_value(serde_json::json!({"name": "t"})).unwrap();
        assert!(config.schema.contains("agent-v1.json"));
    }

    #[test]
    fn test_input_schema_debug() {
        let schema = InputSchema(serde_json::json!({"type": "object"}));
        let dbg = format!("{:?}", schema);
        assert!(dbg.contains("InputSchema"));
    }

    #[test]
    fn test_agent_crew_settings_deser() {
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["agent-a", "test-*"],
            "trustedAgents": ["trusted-one"]
        }))
        .unwrap();
        assert_eq!(settings.available_agents.len(), 2);
        assert_eq!(settings.trusted_agents.len(), 1);
        assert!(settings.available_agents[0].matches("agent-a"));
        assert!(settings.available_agents[1].matches("test-foo"));
    }

    #[test]
    fn test_registry_mcp_with_oauth_scopes() {
        let config: McpServerConfig = serde_json::from_value(serde_json::json!({
            "type": "registry",
            "oauthScopes": ["read", "write"]
        }))
        .unwrap();
        match config {
            McpServerConfig::Registry(r) => {
                assert_eq!(r.oauth_scopes, vec!["read", "write"]);
            },
            _ => panic!("Expected Registry"),
        }
    }

    #[test]
    fn test_mcp_servers_struct_deser() {
        let json = serde_json::json!({
            "mcpServers": {
                "s1": {"command": "x", "args": []}
            }
        });
        let servers: McpServers = serde_json::from_value(json).unwrap();
        assert!(servers.mcp_servers.contains_key("s1"));
    }

    #[test]
    fn test_local_mcp_env_field() {
        let config: LocalMcpServerConfig = serde_json::from_value(serde_json::json!({
            "command": "srv",
            "env": {"FOO": "bar", "BAZ": "qux"}
        }))
        .unwrap();
        let env = config.env.unwrap();
        assert_eq!(env.get("FOO").unwrap(), "bar");
        assert_eq!(env.get("BAZ").unwrap(), "qux");
    }

    #[test]
    fn test_remote_mcp_headers() {
        let config: RemoteMcpServerConfig = serde_json::from_value(serde_json::json!({
            "url": "https://x.com",
            "headers": {"Authorization": "Bearer tok"}
        }))
        .unwrap();
        assert_eq!(config.headers.get("Authorization").unwrap(), "Bearer tok");
    }
}
