use sacp::schema::{
    SessionId,
    ToolKind,
};
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
    /// MCP server init failure notification
    pub const MCP_SERVER_INIT_FAILURE: &str = "_kiro.dev/mcp/server_init_failure";
    /// Rate limit error notification
    pub const RATE_LIMIT_ERROR: &str = "_kiro.dev/error/rate_limit";
    /// Compaction status notification
    pub const COMPACTION_STATUS: &str = "_kiro.dev/compaction/status";
    /// Clear status notification
    pub const CLEAR_STATUS: &str = "_kiro.dev/clear/status";
    /// Agent switched notification
    pub const AGENT_SWITCHED: &str = "_kiro.dev/agent/switched";
    /// List sessions (temporary extension until sacp adds native session/list)
    pub const SESSION_LIST: &str = "_kiro.dev/session/list";
    /// Session update extension notification (e.g. tool_call_chunk)
    pub const SESSION_UPDATE: &str = "_kiro.dev/session/update";
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

/// MCP server init failure notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInitFailureNotification {
    pub session_id: SessionId,
    pub server_name: String,
    pub error: String,
}

/// Rate limit error notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitErrorNotification {
    pub session_id: SessionId,
    pub message: String,
}

/// Compaction status notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionStatusNotification {
    pub session_id: SessionId,
    pub status: CompactionStatus,
    pub summary: Option<String>,
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

/// Agent switched notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSwitchedNotification {
    pub session_id: SessionId,
    pub agent_name: String,
    pub previous_agent_name: Option<String>,
    pub welcome_message: Option<String>,
}

/// Extension session update notification payload.
///
/// Mirrors the ACP `session/update` envelope but delivered via extension channel
/// for Kiro-specific update types not yet in the ACP spec.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtSessionUpdateNotification {
    pub session_id: SessionId,
    pub update: ExtSessionUpdate,
}

/// Extension session update types.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "sessionUpdate", rename_all = "snake_case")]
pub enum ExtSessionUpdate {
    /// Early notification that a tool call is being streamed.
    #[serde(rename_all = "camelCase")]
    ToolCallChunk {
        tool_call_id: String,
        title: String,
        kind: ToolKind,
    },
}
