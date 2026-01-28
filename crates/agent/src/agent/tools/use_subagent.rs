//! Spawn subagent tool implementation.
//!
//! This tool allows the agent to spawn subagent sessions. The actual session
//! spawning is handled by the ACP layer, which listens for `SpawnSubagentRequest`
//! events.

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

/// Request to spawn one or more subagents
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum UseSubagent {
    /// List available agents for delegation
    ListAgents,
    /// Invoke one or more subagents
    InvokeSubagents {
        /// The subagents to spawn
        subagents: Vec<SubagentInvocation>,
    },
}

/// Parameters for a single subagent invocation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInvocation {
    /// The query or task for the subagent
    pub query: String,
    /// Optional agent name to use (defaults to default agent)
    #[serde(default)]
    pub agent_name: Option<String>,
    /// Optional context to help the subagent understand the task
    #[serde(default)]
    pub relevant_context: Option<String>,
}

/// Response from subagent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResponse {
    /// The result to return as tool output
    pub output: ToolExecutionOutput,
}

type InnerSender = oneshot::Sender<Result<SubagentResponse, String>>;

/// Wrapper for oneshot sender that implements Clone via Arc<Mutex<Option<...>>>
#[derive(Debug, Clone, Default)]
pub struct ResponseSender(Arc<Mutex<Option<InnerSender>>>);

impl ResponseSender {
    pub fn new(tx: oneshot::Sender<Result<SubagentResponse, String>>) -> Self {
        Self(Arc::new(Mutex::new(Some(tx))))
    }

    /// Take the sender and send a response. Returns Err if already taken.
    pub async fn send(self, response: Result<SubagentResponse, String>) -> Result<(), String> {
        let mut guard = self.0.lock().await;
        if let Some(tx) = guard.take() {
            tx.send(response).map_err(|_e| "Receiver dropped".to_string())
        } else {
            Err("Response already sent".to_string())
        }
    }
}

/// Event emitted when subagent spawn is requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRequest {
    /// The spawn request details
    pub request: UseSubagent,
    /// Channel to send the response back (can only be used once)
    #[serde(skip)]
    pub response_tx: ResponseSender,
}

const TOOL_DESCRIPTION: &str = r#"
A tool for delegating tasks to specialized subagents.

COMMANDS:
- ListAgents: Query available agents for task delegation
- InvokeSubagents: Spawn one or more subagents to handle tasks

WHEN TO USE:
- When a task would benefit from parallel execution
- When a task requires specialized agent capabilities
- When you want to offload complex subtasks

NOTES:
- Up to 4 subagents can be spawned at once
- Each subagent runs independently with its own context
- Results are returned when all subagents complete (or are backgrounded by user)
"#;

const TOOL_SCHEMA: &str = r#"
{
  "type": "object",
  "properties": {
    "command": {
      "type": "string",
      "enum": [
        "ListAgents",
        "InvokeSubagents"
      ],
      "description": "The commands to run. Allowed options are `ListAgents` to query available agents, or `InvokeSubagents` to invoke one or more subagents"
    },
    "content": {
      "description": "Required for `InvokeSubagents` command. Contains subagents array and optional conversation ID.",
      "type": "object",
      "properties": {
        "subagents": {
          "type": "array",
          "description": "Array of subagent invocations to execute in parallel. Each invocation specifies a query, optional agent name, and optional context.",
          "items": {
            "type": "object",
            "properties": {
              "query": {
                "type": "string",
                "description": "The query or task to be handled by the subagent"
              },
              "agent_name": {
                "type": "string",
                "description": "Optional name of the specific agent to use. If not provided, uses the default agent"
              },
              "relevant_context": {
                "type": "string",
                "description": "Optional additional context that should be provided to the subagent to help it understand the task better"
              }
            },
            "required": [
              "query"
            ]
          }
        }
      },
      "required": [
        "subagents"
      ]
    }
  },
  "required": [
    "command"
  ]
}
"#;

impl BuiltInToolTrait for UseSubagent {
    fn name() -> BuiltInToolName {
        BuiltInToolName::SpawnSubagent
    }

    fn description() -> std::borrow::Cow<'static, str> {
        TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        TOOL_SCHEMA.into()
    }
}

impl UseSubagent {
    /// Execute the spawn subagent tool.
    ///
    /// This emits a `SpawnSubagentRequest` event and waits for the ACP layer
    /// to handle it and return a response.
    pub async fn execute(&self, event_tx: broadcast::Sender<AgentEvent>) -> ToolExecutionResult {
        let (response_tx, response_rx) = oneshot::channel();

        let request = SubagentRequest {
            request: self.clone(),
            response_tx: ResponseSender::new(response_tx),
        };

        // Emit the spawn request event
        event_tx
            .send(AgentEvent::SpawnSubagentRequest(request))
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to send spawn request: {e}")))?;

        // Wait for response from ACP layer
        match response_rx.await {
            Ok(Ok(response)) => Ok(response.output),
            Ok(Err(e)) => Err(ToolExecutionError::Custom(e)),
            Err(_) => Err(ToolExecutionError::Custom(
                "Subagent spawn request was dropped".to_string(),
            )),
        }
    }

    pub fn get_canonical_name() -> CanonicalToolName {
        CanonicalToolName::BuiltIn(BuiltInToolName::SpawnSubagent)
    }
}
