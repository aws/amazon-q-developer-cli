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
    /// Spawns a new session
    pub const SESSION_SPAWN: &str = "_session/spawn";
    pub const MESSAGE_SEND: &str = "_message/send";
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
    /// Subagent list update notification
    pub const SUBAGENT_LIST_UPDATE: &str = "_kiro.dev/subagent/list_update";
    /// Agent switched notification
    pub const AGENT_SWITCHED: &str = "_kiro.dev/agent/switched";
    /// Agent not found — requested agent fell back to default
    pub const AGENT_NOT_FOUND: &str = "_kiro.dev/agent/not_found";
    /// Agent config parse error at startup
    pub const AGENT_CONFIG_ERROR: &str = "_kiro.dev/agent/config_error";
    /// MCP governance disabled — admin turned off MCP in the Kiro console
    pub const MCP_GOVERNANCE_DISABLED: &str = "_kiro.dev/mcp/governance_disabled";
    /// Web tools governance disabled — admin turned off web tools in the Kiro console
    pub const WEB_TOOLS_GOVERNANCE_DISABLED: &str = "_kiro.dev/webTools/governance_disabled";
    /// List sessions (temporary extension until sacp adds native session/list)
    pub const SESSION_LIST: &str = "_kiro.dev/session/list";
    /// Session update extension notification (e.g. tool_call_chunk)
    pub const SESSION_UPDATE: &str = "_kiro.dev/session/update";
    /// Goal loop status notification
    pub const GOAL_STATUS: &str = "_kiro.dev/goal/status";
}

/// Goal loop status notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalStatusNotification {
    pub state: String,
    pub iteration: u32,
    pub max_iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub elapsed_secs: u64,
}

/// Status of a backgrounded subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SubagentStatus {
    /// Subagent is actively working
    Working { message: String },
    /// Subagent completed current task, awaits further instruction
    AwaitingInstruction,
    /// Subagent has terminated
    Terminated,
}

/// Information about a backgrounded subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    pub session_id: SessionId,
    pub session_name: String,
    pub agent_name: String,
    pub initial_query: String,
    pub status: SubagentStatus,
    pub group: Option<String>,
    pub role: Option<String>,
    pub depends_on: Vec<String>,
    /// Whether this stage has a loop-back configuration.
    #[serde(default)]
    pub has_loop: bool,
    /// Current loop iteration (0 if not looping).
    #[serde(default)]
    pub loop_iteration: u32,
    /// Maximum loop iterations (0 if not looping).
    #[serde(default)]
    pub loop_max_iterations: u32,
    /// When this session was created (millis since epoch).
    #[serde(default)]
    pub created_at_ms: u64,
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

/// Notification sent when subagent list changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentListUpdateNotification {
    pub subagents: Vec<SubagentInfo>,
    /// Pending stages waiting for dependencies (crew DAG)
    pub pending_stages: Vec<PendingStageInfo>,
}

/// A pending pipeline stage (not yet spawned, waiting for deps).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingStageInfo {
    pub name: String,
    pub role: String,
    pub group: String,
    pub depends_on: Vec<String>,
    pub agent_name: String,
}

/// Agent switched notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSwitchedNotification {
    pub session_id: SessionId,
    pub agent_name: String,
    pub previous_agent_name: Option<String>,
    pub welcome_message: Option<String>,
    pub model: Option<String>,
}

/// Agent not found notification payload — requested agent fell back to default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNotFoundNotification {
    pub session_id: SessionId,
    pub requested_agent: String,
    pub fallback_agent: String,
}

/// Agent config parse error notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigErrorNotification {
    pub session_id: SessionId,
    pub path: Option<String>,
    pub error: String,
}

/// MCP governance disabled notification payload — admin turned off MCP in the Kiro console.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGovernanceDisabledNotification {
    pub session_id: SessionId,
    /// `true` when the governance API call failed (fail-closed), `false` when admin explicitly
    /// disabled.
    pub api_failure: bool,
}

/// Web tools governance disabled notification payload — admin turned off web tools in the Kiro
/// console.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebToolsGovernanceDisabledNotification {
    pub session_id: SessionId,
    /// `true` when the governance API call failed (fail-closed), `false` when admin explicitly
    /// disabled.
    pub api_failure: bool,
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
    /// HTTP client is retrying a request after a backoff delay.
    #[serde(rename_all = "camelCase")]
    RetryWarning {
        attempt: u32,
        max_attempts: u32,
        delay_secs: f64,
        message: String,
    },
    /// The open response stream has gone quiet past the soft idle threshold but is
    /// not being retried — a message-only progress notice, distinct from
    /// `RetryWarning` so no retry-shaped fields have to be fabricated for it.
    #[serde(rename_all = "camelCase")]
    StreamStallNotice { message: String },
    /// An in-flight stream was abandoned after partial assistant output had
    /// already been sent — by an agent-layer transient retry or by the hard-stall
    /// tier cancelling the stream. The response is regenerated from scratch (the
    /// partial is not kept in history), so the client should discard the rendered
    /// partial instead of concatenating the replacement response onto it.
    StreamDiscarded,
    /// A steering message was queued for mid-turn injection.
    ///
    /// `content` is the **full current queue snapshot** (multiple steers are
    /// concatenated on the backend with `"\n\n"`). Each emission carries the
    /// entire queue, so clients should overwrite their local copy rather than
    /// append. `message_id` is the stable id of the steer just queued.
    ///
    /// Wire shape matches the KAS `AgentExecutionUserMessageQueued`
    /// notification (`@kiro/acp-type-covenant` steering/session-update).
    #[serde(rename = "AgentExecutionUserMessageQueued", rename_all = "camelCase")]
    AgentExecutionUserMessageQueued { message_id: String, content: String },
    /// A queued steering message was consumed and injected into the
    /// conversation. Emitted once per steer, carrying that steer's stable
    /// `message_id` and raw `content`.
    ///
    /// Wire shape matches the KAS `AgentExecutionSteeringInjected` notification.
    #[serde(rename = "AgentExecutionSteeringInjected", rename_all = "camelCase")]
    AgentExecutionSteeringInjected { message_id: String, content: String },
    /// The queued steering messages were cleared without being consumed
    /// (cancel, or explicit TUI-initiated clear). `message_ids` lists every
    /// steer dropped from the queue so clients can reconcile out-of-order
    /// delivery. Clients should clear any local queue display on receipt.
    ///
    /// Wire shape matches the KAS `AgentExecutionUserMessageCleared`
    /// notification.
    #[serde(rename = "AgentExecutionUserMessageCleared", rename_all = "camelCase")]
    AgentExecutionUserMessageCleared { message_ids: Vec<String> },
}
