//! Session management tool for agent-to-agent orchestration.
//!
//! This tool allows agents to spawn persistent sessions, send messages,
//! read their inbox, and manage session groups. The actual session
//! operations are handled by the ACP layer.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    Mutex,
    broadcast,
    oneshot,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionResult,
};
use crate::agent_config::parse::CanonicalToolName;
use crate::protocol::AgentEvent;

/// Priority level for inter-session messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    #[default]
    Normal,
    Escalation,
}

/// Filter for listing sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionFilter {
    Active,
    Idle,
    Busy,
    Terminated,
    All,
}

/// Action for managing session groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupAction {
    Create,
    Add,
    Remove,
    List,
    Broadcast,
}

/// Session management tool — all orchestration operations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SessionTool {
    /// Spawn a new persistent session
    SpawnSession {
        /// Agent config name to use
        agent_name: String,
        /// Initial task/prompt for the session
        task: String,
        /// Optional friendly name (auto-assigned if omitted)
        #[serde(default)]
        name: Option<String>,
        /// Optional role description
        #[serde(default)]
        role: Option<String>,
        /// Optional group to add the session to
        #[serde(default)]
        group: Option<String>,
        /// If true, session stays alive after task (persistent helper). If false, terminates after
        /// task (ephemeral worker).
        #[serde(default)]
        persistent: Option<bool>,
    },
    /// Send a message to another session's inbox
    SendMessage {
        /// Target session ID or name (omit for escalation auto-route to parent)
        #[serde(default)]
        target: Option<String>,
        /// Message content
        message: String,
        /// Priority: normal (default) or escalation
        #[serde(default)]
        priority: MessagePriority,
    },
    /// Read messages from this session's inbox
    ReadMessages {
        /// Max messages to return (default 5)
        #[serde(default = "default_read_limit")]
        limit: usize,
    },
    /// List all active sessions
    ListSessions {
        /// Optional filter: active, idle, busy, terminated, all
        #[serde(default)]
        filter: Option<SessionFilter>,
    },
    /// Get detailed status of a specific session
    GetSessionStatus {
        /// Session ID or name
        target: String,
        /// Show full details including live activity (default: false)
        #[serde(default)]
        verbose: Option<bool>,
    },
    /// Interrupt a session and redirect it with a new message
    Interrupt {
        /// Target session ID or name
        target: String,
        /// New direction/message
        message: String,
    },
    /// Silently inject context into a session (no turn triggered)
    InjectContext {
        /// Target session ID or name
        target: String,
        /// Context content to inject
        context: String,
    },
    /// Manage session groups
    ManageGroup {
        /// Action to perform on the group
        action: GroupAction,
        /// Group name
        #[serde(default)]
        group: Option<String>,
        /// Session ID or name (for add/remove)
        #[serde(default)]
        target: Option<String>,
        /// Role within group (for add)
        #[serde(default)]
        role: Option<String>,
        /// Message content (for broadcast)
        #[serde(default)]
        message: Option<String>,
    },
    /// Revive a terminated session with a new task (keeps same name/group)
    ReviveSession {
        /// Session name to revive
        target: String,
        /// New task/prompt for the revived session
        task: String,
    },
    /// Register pending pipeline stages for DAG execution (internal, used by agent_crew)
    RegisterPendingStages {
        /// Group name these stages belong to
        group: String,
        /// Stages waiting for their dependencies
        pending_stages: Vec<crate::agent::tools::agent_crew::PendingStageSpec>,
    },
    /// Wait for all sessions in a group to complete (internal, used by agent_crew blocking mode)
    WaitForGroup {
        /// Group name to wait for completion
        group: String,
    },
}

fn default_read_limit() -> usize {
    5
}

/// Response from session tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolResponse {
    pub output: ToolExecutionOutput,
}

type InnerSender = oneshot::Sender<Result<SessionToolResponse, String>>;

/// Wrapper for oneshot sender that implements Clone via Arc<Mutex<Option<...>>>
#[derive(Debug, Clone, Default)]
pub struct SessionResponseSender(Arc<Mutex<Option<InnerSender>>>);

impl SessionResponseSender {
    pub fn new(tx: oneshot::Sender<Result<SessionToolResponse, String>>) -> Self {
        Self(Arc::new(Mutex::new(Some(tx))))
    }

    pub async fn send(self, response: Result<SessionToolResponse, String>) -> Result<(), String> {
        let mut guard = self.0.lock().await;
        if let Some(tx) = guard.take() {
            tx.send(response).map_err(|_e| "Receiver dropped".to_string())
        } else {
            Err("Response already sent".to_string())
        }
    }
}

/// Event emitted when a session tool operation is requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolRequest {
    pub request: SessionTool,
    #[serde(skip)]
    pub response_tx: SessionResponseSender,
}

const TOOL_DESCRIPTION: &str = r#"
Manage persistent agent sessions for orchestration. Sessions are long-lived agents
that communicate via inbox messaging, unlike subagents which are ephemeral.

COMMANDS:
- spawn_session: Create a new persistent session with any agent config
- send_message: Send a message to another session's inbox
- read_messages: Read messages from your inbox
- list_sessions: List all active sessions
- get_session_status: Get detailed status of a session
- interrupt: Cancel a session's current work and redirect it
- inject_context: Silently add context to a session (no turn triggered)
- manage_group: Create/manage groups of sessions
- revive_session: Re-spawn a terminated worker with a new task

WHEN TO USE:
- Use spawn_session for complex, multi-step tasks (code reviews, refactoring, research)
- Handle simple tasks yourself — only spawn sessions when the work genuinely benefits from delegation
- Use send_message for async communication between sessions
- Use read_messages when your system prompt shows unread messages — results arrive automatically
- Use interrupt to redirect a session that's going off track
- Use revive_session to re-spawn a terminated worker with a new task

HOW RESULTS ARRIVE:
- Workers deliver results to your inbox automatically when they finish
- Your system prompt shows unread message counts — use read_messages when you see them
- For checking on a specific worker, use get_session_status

NOTES:
- Sessions persist and maintain full conversation history
- Messages appear in the target's system prompt as unread count
- Use use_subagent for one-off tasks; use sessions for ongoing collaboration
"#;

const TOOL_SCHEMA: &str = r#"
{
  "type": "object",
  "properties": {
    "command": {
      "type": "string",
      "enum": [
        "spawn_session",
        "send_message",
        "read_messages",
        "list_sessions",
        "get_session_status",
        "interrupt",
        "inject_context",
        "manage_group",
        "revive_session"
      ],
      "description": "The session management operation to perform"
    },
    "agent_name": {
      "type": "string",
      "description": "Agent config name for spawn_session"
    },
    "task": {
      "type": "string",
      "description": "Initial task/prompt for spawn_session"
    },
    "name": {
      "type": "string",
      "description": "Optional friendly name for spawn_session"
    },
    "role": {
      "type": "string",
      "description": "Optional role description for spawn_session or manage_group add"
    },
    "target": {
      "type": "string",
      "description": "Target session ID or name for send_message, get_session_status, interrupt, inject_context, manage_group add/remove. Omit for escalation auto-route to parent."
    },
    "message": {
      "type": "string",
      "description": "Message content for send_message, interrupt, or manage_group broadcast"
    },
    "priority": {
      "type": "string",
      "enum": ["normal", "escalation"],
      "description": "Message priority for send_message. 'escalation' auto-routes to parent if no target specified."
    },
    "limit": {
      "type": "integer",
      "description": "Max messages to return for read_messages (default 5)"
    },
    "filter": {
      "type": "string",
      "enum": ["active", "idle", "busy", "terminated", "all"],
      "description": "Optional filter for list_sessions"
    },
    "verbose": {
      "type": "boolean",
      "description": "Show full details for get_session_status including live activity (default: false)"
    },
    "context": {
      "type": "string",
      "description": "Context content for inject_context"
    },
    "action": {
      "type": "string",
      "enum": ["create", "add", "remove", "list", "broadcast"],
      "description": "Action for manage_group"
    },
    "group": {
      "type": "string",
      "description": "Group name for spawn_session or manage_group"
    },
    "persistent": {
      "type": "boolean",
      "description": "If true, session stays alive after completing its task (persistent helper). Default false (ephemeral worker that terminates after task)."
    }
  },
  "required": ["command"]
}
"#;

impl BuiltInToolTrait for SessionTool {
    fn name() -> BuiltInToolName {
        BuiltInToolName::SessionManagement
    }

    fn description() -> std::borrow::Cow<'static, str> {
        TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        TOOL_SCHEMA.into()
    }
}

impl SessionTool {
    /// Execute the session tool by emitting a request event and waiting for the ACP layer.
    pub async fn execute(&self, event_tx: broadcast::Sender<AgentEvent>) -> ToolExecutionResult {
        let (response_tx, response_rx) = oneshot::channel();

        let request = SessionToolRequest {
            request: self.clone(),
            response_tx: SessionResponseSender::new(response_tx),
        };

        event_tx
            .send(AgentEvent::SessionToolRequest(request))
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to send session tool request: {e}")))?;

        match response_rx.await {
            Ok(Ok(response)) => Ok(response.output),
            Ok(Err(e)) => Err(ToolExecutionError::Custom(e)),
            Err(_) => Err(ToolExecutionError::Custom(
                "Session tool request was dropped".to_string(),
            )),
        }
    }

    pub fn get_canonical_name() -> CanonicalToolName {
        CanonicalToolName::BuiltIn(BuiltInToolName::SessionManagement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_tool_serialize_spawn() {
        let tool = SessionTool::SpawnSession {
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            name: Some("test-session".to_string()),
            role: None,
            group: None,
            persistent: None,
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains(r#""command":"spawn_session""#));
        assert!(json.contains(r#""agent_name":"test-agent""#));
        assert!(json.contains(r#""task":"test task""#));
    }

    #[test]
    fn test_session_tool_serialize_send_message() {
        let tool = SessionTool::SendMessage {
            target: Some("session-1".to_string()),
            message: "hello".to_string(),
            priority: MessagePriority::Normal,
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains(r#""command":"send_message""#));
        assert!(json.contains(r#""target":"session-1""#));
        assert!(json.contains(r#""message":"hello""#));
    }

    #[test]
    fn test_session_tool_serialize_read_messages() {
        let tool = SessionTool::ReadMessages { limit: 5 };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains(r#""command":"read_messages""#));
        assert!(json.contains(r#""limit":5"#));
    }

    #[test]
    fn test_session_tool_deserialize_spawn() {
        let json = r#"{"command":"spawn_session","agent_name":"test","task":"work"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::SpawnSession { agent_name, task, .. } 
            if agent_name == "test" && task == "work"));
    }

    #[test]
    fn test_session_tool_deserialize_send_message() {
        let json = r#"{"command":"send_message","target":"s1","message":"hi"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::SendMessage { target: Some(t), message, .. }
            if t == "s1" && message == "hi"));
    }

    #[test]
    fn test_session_tool_deserialize_list_sessions() {
        let json = r#"{"command":"list_sessions"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::ListSessions { .. }));
    }

    #[test]
    fn test_session_tool_name() {
        assert_eq!(SessionTool::name(), BuiltInToolName::SessionManagement);
    }

    #[test]
    fn test_default_read_limit() {
        assert_eq!(default_read_limit(), 5);
    }
}
