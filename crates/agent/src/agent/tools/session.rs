//! Session management tool for agent-to-agent orchestration.
//!
//! This tool allows agents to spawn persistent sessions and manage session
//! groups. The actual session operations are handled by the ACP layer.

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
        /// Optional model override for the session
        #[serde(default)]
        model: Option<String>,
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
Manage persistent agent sessions for orchestration. Sessions are long-lived agents,
unlike subagents which are ephemeral.

COMMANDS:
- spawn_session: Create a new persistent session with any agent config
- list_sessions: List all active sessions
- get_session_status: Get detailed status of a session
- interrupt: Cancel a session's current work and redirect it
- inject_context: Silently add context to a session (no turn triggered)
- manage_group: Create/manage groups of sessions
- revive_session: Re-spawn a terminated worker with a new task

WHEN TO USE:
- Use spawn_session for complex, multi-step tasks (code reviews, refactoring, research)
- Handle simple tasks yourself — only spawn sessions when the work genuinely benefits from delegation
- Use interrupt to redirect a session that's going off track
- Use revive_session to re-spawn a terminated worker with a new task

HOW RESULTS ARRIVE:
- Worker results are consolidated and returned when the group completes
- For checking on a specific worker, use get_session_status

NOTES:
- Sessions persist and maintain full conversation history
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
    "model": {
      "type": "string",
      "description": "Optional model override for spawn_session"
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
      "description": "Target session ID or name for get_session_status, interrupt, inject_context, manage_group add/remove."
    },
    "message": {
      "type": "string",
      "description": "Message content for interrupt"
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
      "enum": ["create", "add", "remove", "list"],
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
            model: Some("claude-sonnet".to_string()),
            name: Some("test-session".to_string()),
            role: None,
            group: None,
            persistent: None,
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains(r#""command":"spawn_session""#));
        assert!(json.contains(r#""agent_name":"test-agent""#));
        assert!(json.contains(r#""task":"test task""#));
        assert!(json.contains(r#""model":"claude-sonnet""#));
    }

    #[test]
    fn test_session_tool_deserialize_spawn() {
        let json = r#"{"command":"spawn_session","agent_name":"test","task":"work"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::SpawnSession { agent_name, task, .. } 
            if agent_name == "test" && task == "work"));
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
    fn test_session_filter_serde() {
        for f in [
            SessionFilter::Active,
            SessionFilter::Idle,
            SessionFilter::Busy,
            SessionFilter::Terminated,
            SessionFilter::All,
        ] {
            let json = serde_json::to_string(&f).unwrap();
            let parsed: SessionFilter = serde_json::from_str(&json).unwrap();
            assert_eq!(f, parsed);
        }
    }

    #[test]
    fn test_group_action_serde() {
        for a in [
            GroupAction::Create,
            GroupAction::Add,
            GroupAction::Remove,
            GroupAction::List,
        ] {
            let json = serde_json::to_string(&a).unwrap();
            let parsed: GroupAction = serde_json::from_str(&json).unwrap();
            assert_eq!(a, parsed);
        }
    }

    #[test]
    fn test_built_in_tool_trait() {
        assert!(!SessionTool::description().is_empty());
        let schema: serde_json::Value = serde_json::from_str(&SessionTool::input_schema()).unwrap();
        assert_eq!(schema["properties"]["model"]["type"], "string");
    }

    #[test]
    fn test_get_canonical_name() {
        let name = SessionTool::get_canonical_name();
        assert!(matches!(
            name,
            CanonicalToolName::BuiltIn(BuiltInToolName::SessionManagement)
        ));
    }

    #[test]
    fn test_deserialize_get_session_status() {
        let json = r#"{"command":"get_session_status","target":"s1","verbose":true}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(
            matches!(tool, SessionTool::GetSessionStatus { target, verbose } if target == "s1" && verbose == Some(true))
        );
    }

    #[test]
    fn test_deserialize_interrupt() {
        let json = r#"{"command":"interrupt","target":"s1","message":"stop"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::Interrupt { target, message } if target == "s1" && message == "stop"));
    }

    #[test]
    fn test_deserialize_inject_context() {
        let json = r#"{"command":"inject_context","target":"s1","context":"ctx"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::InjectContext { target, context } if target == "s1" && context == "ctx"));
    }

    #[test]
    fn test_deserialize_manage_group() {
        let json = r#"{"command":"manage_group","action":"create","group":"g1"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(
            matches!(tool, SessionTool::ManageGroup { action: GroupAction::Create, group: Some(g), .. } if g == "g1")
        );
    }

    #[test]
    fn test_deserialize_revive_session() {
        let json = r#"{"command":"revive_session","target":"s1","task":"new task"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::ReviveSession { target, task } if target == "s1" && task == "new task"));
    }

    #[test]
    fn test_deserialize_wait_for_group() {
        let json = r#"{"command":"wait_for_group","group":"g1"}"#;
        let tool: SessionTool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, SessionTool::WaitForGroup { group } if group == "g1"));
    }

    #[test]
    fn test_serialize_all_variants_roundtrip() {
        let variants: Vec<SessionTool> = vec![
            SessionTool::SpawnSession {
                agent_name: "a".into(),
                task: "t".into(),
                model: None,
                name: None,
                role: Some("r".into()),
                group: Some("g".into()),
                persistent: Some(true),
            },
            SessionTool::ListSessions {
                filter: Some(SessionFilter::Busy),
            },
            SessionTool::GetSessionStatus {
                target: "t".into(),
                verbose: Some(false),
            },
            SessionTool::Interrupt {
                target: "t".into(),
                message: "m".into(),
            },
            SessionTool::InjectContext {
                target: "t".into(),
                context: "c".into(),
            },
            SessionTool::ManageGroup {
                action: GroupAction::Add,
                group: Some("g".into()),
                target: Some("t".into()),
                role: Some("r".into()),
            },
            SessionTool::ReviveSession {
                target: "t".into(),
                task: "task".into(),
            },
            SessionTool::WaitForGroup { group: "g".into() },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: SessionTool = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[tokio::test]
    async fn test_response_sender_send_success() {
        let (tx, rx) = oneshot::channel();
        let sender = SessionResponseSender::new(tx);
        let resp = SessionToolResponse {
            output: ToolExecutionOutput::new(vec![]),
        };
        sender.send(Ok(resp)).await.unwrap();
        assert!(rx.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_response_sender_send_twice_fails() {
        let (tx, _rx) = oneshot::channel();
        let sender = SessionResponseSender::new(tx);
        let resp = SessionToolResponse {
            output: ToolExecutionOutput::new(vec![]),
        };
        sender.clone().send(Ok(resp.clone())).await.unwrap();
        let result = sender.send(Ok(resp)).await;
        assert_eq!(result.unwrap_err(), "Response already sent");
    }

    #[tokio::test]
    async fn test_response_sender_receiver_dropped() {
        let (tx, rx) = oneshot::channel();
        let sender = SessionResponseSender::new(tx);
        drop(rx);
        let resp = SessionToolResponse {
            output: ToolExecutionOutput::new(vec![]),
        };
        let result = sender.send(Ok(resp)).await;
        assert_eq!(result.unwrap_err(), "Receiver dropped");
    }

    #[tokio::test]
    async fn test_execute_channel_closed() {
        let (event_tx, _rx) = broadcast::channel::<AgentEvent>(1);
        drop(_rx);
        let tool = SessionTool::ListSessions { filter: None };
        let result = tool.execute(event_tx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_response_dropped() {
        let (event_tx, mut event_rx) = broadcast::channel::<AgentEvent>(1);
        let tool = SessionTool::ListSessions { filter: None };

        let handle = tokio::spawn(async move { tool.execute(event_tx).await });

        if let Ok(AgentEvent::SessionToolRequest(req)) = event_rx.recv().await {
            drop(req.response_tx);
        }

        let result = handle.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_success() {
        let (event_tx, mut event_rx) = broadcast::channel::<AgentEvent>(1);
        let tool = SessionTool::ListSessions { filter: None };

        let handle = tokio::spawn(async move { tool.execute(event_tx).await });

        if let Ok(AgentEvent::SessionToolRequest(req)) = event_rx.recv().await {
            let resp = SessionToolResponse {
                output: ToolExecutionOutput::new(vec![]),
            };
            req.response_tx.send(Ok(resp)).await.unwrap();
        }

        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_error_response() {
        let (event_tx, mut event_rx) = broadcast::channel::<AgentEvent>(1);
        let tool = SessionTool::ListSessions { filter: None };

        let handle = tokio::spawn(async move { tool.execute(event_tx).await });

        if let Ok(AgentEvent::SessionToolRequest(req)) = event_rx.recv().await {
            req.response_tx.send(Err("test error".into())).await.unwrap();
        }

        let result = handle.await.unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_session_response_sender_default() {
        let sender = SessionResponseSender::default();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let result = rt.block_on(sender.send(Err("x".into())));
        assert_eq!(result.unwrap_err(), "Response already sent");
    }

    #[test]
    fn test_session_tool_request_debug() {
        let req = SessionToolRequest {
            request: SessionTool::ListSessions { filter: None },
            response_tx: SessionResponseSender::default(),
        };
        let debug = format!("{:?}", req);
        assert!(debug.contains("ListSessions"));
    }
}
