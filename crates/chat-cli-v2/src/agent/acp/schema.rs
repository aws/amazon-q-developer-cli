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
