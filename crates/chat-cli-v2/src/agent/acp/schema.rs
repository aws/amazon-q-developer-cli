//! ACP custom extension types with derive macros.

use agent::tui_commands::{
    CommandOptionsResponse,
    CommandResult,
    TuiCommand,
};
use sacp::{
    JrNotification,
    JrRequest,
    JrResponsePayload,
};
use serde::{
    Deserialize,
    Serialize,
};

/// Request to execute a TUI command
#[derive(Debug, Clone, Serialize, Deserialize, JrRequest)]
#[request(method = "_kiro.dev/commands/execute", response = CommandExecuteResponse)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecuteRequest {
    pub session_id: String,
    pub command: TuiCommand,
}

/// Response - transparent wrapper for wire compatibility
#[derive(Debug, Clone, Serialize, Deserialize, JrResponsePayload)]
#[serde(transparent)]
pub struct CommandExecuteResponse(pub CommandResult);

impl From<CommandResult> for CommandExecuteResponse {
    fn from(result: CommandResult) -> Self {
        Self(result)
    }
}

/// Command kind for options request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TuiCommandKind {
    Model,
    Agent,
    Context,
    Compact,
    Clear,
    Quit,
    Usage,
    Mcp,
    Tools,
    Prompts,
}

/// Request to get command options (autocomplete)
#[derive(Debug, Clone, Serialize, Deserialize, JrRequest)]
#[request(method = "_kiro.dev/commands/options", response = CommandOptionsResponseWrapper)]
#[serde(rename_all = "camelCase")]
pub struct CommandOptionsRequest {
    pub session_id: String,
    pub command: TuiCommandKind,
    #[serde(default)]
    pub partial: String,
}

/// Response wrapper for command options
#[derive(Debug, Clone, Serialize, Deserialize, JrResponsePayload)]
#[serde(transparent)]
pub struct CommandOptionsResponseWrapper(pub CommandOptionsResponse);

impl From<CommandOptionsResponse> for CommandOptionsResponseWrapper {
    fn from(resp: CommandOptionsResponse) -> Self {
        Self(resp)
    }
}

/// Notification to advertise available commands
#[derive(Debug, Clone, Serialize, Deserialize, JrNotification)]
#[notification(method = "_kiro.dev/commands/available")]
#[serde(rename_all = "camelCase")]
pub struct CommandsAvailableNotification {
    pub session_id: String,
    pub commands: Vec<AvailableCommand>,
    #[serde(default)]
    pub prompts: Vec<PromptInfo>,
    #[serde(default)]
    pub tools: Vec<ToolAdvertisement>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerAdvertisement>,
}

/// A command available for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCommand {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, serde_json::Value>>,
}

/// A prompt available for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptInfo {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Vec<PromptArgumentInfo>,
    pub server_name: String,
}

/// Argument information for a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgumentInfo {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

/// A tool advertised for slash command autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAdvertisement {
    pub name: String,
    pub description: String,
    pub source: String,
}

/// An MCP server advertised for slash command autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerAdvertisement {
    pub name: String,
    pub status: String,
    pub tool_count: usize,
}

/// Metadata update sent as a session notification (extensible)
#[derive(Debug, Clone, Serialize, Deserialize, JrNotification)]
#[notification(method = "_kiro.dev/metadata")]
#[serde(rename_all = "camelCase")]
pub struct MetadataNotification {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage_percentage: Option<f32>,
    // Future fields can be added here
}

// ---------------------------------------------------------------------------
// session/list types — defined locally because sacp 10.x does not yet have
// native ListSessionsRequest / ListSessionsResponse support.
//
// TODO: Replace these with types from `sacp::schema` (or re-exported from
// `agent_client_protocol_schema`) once sacp adds first-class session/list
// support. The wire format intentionally matches the ACP session/list RFD
// (https://agentclientprotocol.com/rfds/session-list) and the types in
// `agent-client-protocol-schema` ≥ 0.11.
// ---------------------------------------------------------------------------

/// Request parameters for `session/list`.
///
/// NOTE: The method is registered as `_kiro.dev/session/list` to match the Kiro
/// extension method namespace. This is a temporary extension until sacp adds
/// native session/list support, at which point this should use `session/list`.
/// TODO: Change method to `session/list` once sacp adds native handler support.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JrRequest)]
#[request(method = "_kiro.dev/session/list", response = ListSessionsResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsRequest {
    /// Filter sessions by working directory. Required.
    pub cwd: Option<std::path::PathBuf>,
    /// Opaque cursor for pagination (unused for now).
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Response from `session/list`.
#[derive(Debug, Clone, Serialize, Deserialize, JrResponsePayload)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionInfoEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// A single session in the `session/list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoEntry {
    pub session_id: String,
    pub cwd: std::path::PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
