use sacp::schema::SessionId;
use serde::{
    Deserialize,
    Serialize,
};

/// Extension method names (prefixed with underscore per ACP spec)
pub mod methods {
    /// Terminates a session
    pub const SESSION_TERMINATE: &str = "_session/terminate";
    /// OAuth request notification from MCP server
    pub const MCP_OAUTH_REQUEST: &str = "_kiro.dev/mcp/oauth_request";
    /// MCP server initialized notification
    pub const MCP_SERVER_INITIALIZED: &str = "_kiro.dev/mcp/server_initialized";
    /// Compaction status notification
    pub const COMPACTION_STATUS: &str = "_kiro.dev/compaction/status";
    /// Clear status notification
    pub const CLEAR_STATUS: &str = "_kiro.dev/clear/status";
}

/// Notification to terminate a subagent session.
///
/// Sent from TUI to agent when user explicitly kills a subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminateSessionNotification {
    pub session_id: SessionId,
}

/// Status of a backgrounded subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SubagentStatus {
    /// Subagent is actively working
    Working { message: String },
    /// Subagent completed current task, awaits further instruction
    AwaitingInstruction,
}

/// Information about a backgrounded subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    pub session_id: SessionId,
    pub agent_name: String,
    pub initial_query: String,
    pub status: SubagentStatus,
}

/// OAuth request notification payload for MCP servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOauthRequestNotification {
    pub session_id: SessionId,
    pub server_name: String,
    pub oauth_url: String,
}

/// MCP server initialized notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInitializedNotification {
    pub session_id: SessionId,
    pub server_name: String,
}

/// Compaction status notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionStatusNotification {
    pub session_id: SessionId,
    pub status: CompactionStatus,
}

/// Status of a compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CompactionStatus {
    Started,
    Completed,
    Failed { error: String },
}

/// Clear status notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearStatusNotification {
    pub session_id: SessionId,
}
