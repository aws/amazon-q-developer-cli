//! ACP custom extension types with derive macros.

use agent::agent_loop::types::MeteringUsageInfo;
use agent::tui_commands::{
    CommandOptionsResponse,
    CommandResult,
    TuiCommand,
};
use sacp::{
    JsonRpcNotification,
    JsonRpcRequest,
    JsonRpcResponse,
};
use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

/// Request to execute a TUI command
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/commands/execute", response = CommandExecuteResponse)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecuteRequest {
    pub session_id: String,
    pub command: TuiCommand,
}

/// Response - transparent wrapper for wire compatibility
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
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
    Feedback,
    Chat,
    Rewind,
    Effort,
}

/// Request to get command options (autocomplete)
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/commands/options", response = CommandOptionsResponseWrapper)]
#[serde(rename_all = "camelCase")]
pub struct CommandOptionsRequest {
    pub session_id: String,
    pub command: TuiCommandKind,
    #[serde(default)]
    pub partial: String,
}

/// Response wrapper for command options
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
#[serde(transparent)]
pub struct CommandOptionsResponseWrapper(pub CommandOptionsResponse);

impl From<CommandOptionsResponse> for CommandOptionsResponseWrapper {
    fn from(resp: CommandOptionsResponse) -> Self {
        Self(resp)
    }
}

/// Notification to advertise available commands
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "_kiro.dev/metadata")]
#[serde(rename_all = "camelCase")]
pub struct MetadataNotification {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage_percentage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metering_usage: Option<Vec<MeteringUsageInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

// ---------------------------------------------------------------------------
// session/list ext-method types served by chat_cli_v2's own ACP agent.
//
// `agent-client-protocol` ≥ 0.10.4 (used by KAS clients in this workspace)
// has native `session/list` types that should be preferred for new clients.
// The types below remain because chat_cli_v2's agent still exposes session
// listing through the legacy `_kiro.dev/session/list` ext_method - migrate
// them to the native `acp::ListSessions{Request,Response}` once the agent
// implements `Agent::list_sessions` directly.
// ---------------------------------------------------------------------------

/// Request parameters for `_kiro.dev/session/list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/session/list", response = ListSessionsResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsRequest {
    /// Filter sessions by working directory. Required.
    pub cwd: Option<std::path::PathBuf>,
    /// Opaque cursor for pagination (unused for now).
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Response from `_kiro.dev/session/list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
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
    /// Number of log entries for this session. `None` when the serving agent
    /// does not track or expose a count - e.g. KAS does not currently include
    /// it in `session/list` responses. V2 always populates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<usize>,
}

/// Request to list user settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/settings/list", response = SettingsListResponse)]
pub struct SettingsListRequest {}

/// Response containing all user settings as a flat JSON map.
/// Keys use the same dotted names as the settings file (e.g. "chat.greeting.enabled").
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
#[serde(transparent)]
pub struct SettingsListResponse(pub serde_json::Map<String, serde_json::Value>);

/// Request to set a single user setting.
/// The key uses the same dotted names as the settings file (e.g.
/// "chat.disableTrustAllConfirmation"). The write is performed with file-level locking so it is
/// safe to call from the TUI process while the Rust backend may also be writing settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/settings/set", response = SettingsSetResponse)]
pub struct SettingsSetRequest {
    pub key: String,
    pub value: serde_json::Value,
}

/// Response for settings/set — empty on success.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
pub struct SettingsSetResponse {}

/// Request to terminate a session
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_kiro.dev/session/terminate", response = TerminateSessionResponse)]
#[serde(rename_all = "camelCase")]
pub struct TerminateSessionRequest {
    pub session_id: String,
}

/// Response for session terminate
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
pub struct TerminateSessionResponse {}

/// Process health telemetry payload sent from TUI every 60s.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessHealthPayload {
    pub rss_mb: Option<f64>,
    pub heap_used_mb: Option<f64>,
    pub peak_rss_mb: Option<f64>,
    pub cpu_user_pct: Option<f64>,
    pub cpu_system_pct: Option<f64>,
    pub last_render_ms: Option<f64>,
    pub max_render_ms: Option<f64>,
    pub renders_per_min: Option<i64>,
    pub full_redraws_per_min: Option<i64>,
    pub yoga_node_count: Option<i64>,
    pub event_loop_p99_ms: Option<f64>,
    pub input_latency_p95_ms: Option<f64>,
    pub session_duration_sec: Option<i64>,
    pub cpu_cores: Option<i64>,
    pub total_memory_mb: Option<i64>,
    pub terminal: Option<String>,
    pub session_id: Option<String>,
    pub version: String,
    pub platform: String,
}

/// How a mode change was initiated. Add a new variant when adding a new entry point —
/// the wire format is the camelCase variant name. Keeping this as an enum (rather than a
/// free-form string) gives us spell-check at the call site and a single documented set of
/// known values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[typeshare]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum ModeChangeSource {
    /// User pressed Shift+Tab to toggle in/out of `kiro_planner`.
    ShiftTab,
    /// User invoked a slash command (`/agent`, `/plan`).
    SlashCommand,
}

/// Telemetry payload sent from the TUI when the active agent (= ACP session mode) changes.
/// Caller is responsible for skipping no-op changes (`from_mode == to_mode`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "_kiro.dev/telemetry/modeChanged")]
#[typeshare]
#[serde(rename_all = "camelCase")]
pub struct ModeChangedNotification {
    /// Agent name the user was on before the change.
    pub from_mode: String,
    /// Agent name the user is on after the change.
    pub to_mode: String,
    /// How the change was initiated.
    pub source: ModeChangeSource,
    /// ACP session id, used as `amazonqConversationId` on the metric.
    pub session_id: Option<String>,
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip + string conversions for [`ModeChangeSource`].
    /// Exercises both the serde JSON path and the strum string path so that an accidental
    /// rename or `rename_all` change is caught at test time rather than silently breaking
    /// the wire format with the TUI.
    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());
            assert_eq!($variant, $text.parse::<$ty>().unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_mode_change_source_ser_deser() {
        test_ser_deser!(ModeChangeSource, ModeChangeSource::ShiftTab, "shiftTab");
        test_ser_deser!(ModeChangeSource, ModeChangeSource::SlashCommand, "slashCommand");
    }
}
