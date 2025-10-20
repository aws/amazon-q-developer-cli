use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use super::ExecutionState;
use super::agent_loop::AgentLoopId;
use super::agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopEventKind,
    AgentLoopResponseError,
    LoopError,
    SendRequestArgs,
};
use super::agent_loop::types::{
    ImageBlock,
    ToolUseBlock,
};
use super::mcp::McpManagerError;
use super::mcp::types::Prompt;
use super::task_executor::TaskExecutorEvent;
use super::tools::Tool;
use super::types::AgentSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AgentEvent {
    /// Agent has finished initialization, and is ready to receive requests.
    ///
    /// This is the first event that the agent will emit.
    Initialized,
    /// Events associated with the agent loop.
    ///
    /// These events contain information about the model's response, including:
    /// - Text content
    /// - Tool uses
    /// - Metadata about a response stream, and about a complete user turn
    AgentLoop(AgentLoopEvent),
    /// The exact request sent to the backend
    RequestSent(SendRequestArgs),
    /// An unknown error occurred with the model backend that could not be handled by the agent.
    RequestError(LoopError),
    /// The agent has changed state.
    StateChange { from: ExecutionState, to: ExecutionState },
    /// A tool use was requested by the model, and the permission was evaluated
    ToolPermissionEvalResult { tool: Tool, result: PermissionEvalResult },
    /// Events specific to tool and hook execution
    TaskExecutor(TaskExecutorEvent),
    ApprovalRequest {
        /// Id for the approval request
        id: String,
        /// The tool use to be approved or denied
        tool_use: ToolUseBlock,
        /// Tool-specific context about the requested operation
        context: Option<super::tools::ToolContext>,
    },
}

impl AgentEvent {
    pub fn agent_loop(id: AgentLoopId, kind: AgentLoopEventKind) -> Self {
        Self::AgentLoop(AgentLoopEvent { id, kind })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentRequest {
    /// Send a new prompt
    SendPrompt(SendPromptArgs),
    /// Interrupt the agent's execution
    ///
    /// This will always end the current user turn.
    Interrupt,
    SendApprovalResult(SendApprovalResultArgs),
    /// Creates a serializable snapshot of the agent's current state
    CreateSnapshot,
    /// Compact the conversation history
    Compact,
    GetMcpPrompts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPromptArgs {
    /// Input content
    pub content: Vec<InputItem>,
}

impl SendPromptArgs {
    /// Returns the text items of the content joined as a single string, if any text items exist.
    pub fn text(&self) -> Option<String> {
        let text = self
            .content
            .as_slice()
            .iter()
            .filter_map(|c| match c {
                InputItem::Text(t) => Some(t.clone()),
                InputItem::Image(_) => None,
            })
            .collect::<Vec<_>>();
        if !text.is_empty() { Some(text.join("")) } else { None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendApprovalResultArgs {
    /// Id of the approval request
    pub id: String,
    /// Whether or not the request is approved
    pub result: ApprovalResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalResult {
    Approve,
    Deny { reason: Option<String> },
}

/// Result of evaluating tool permissions, indicating whether a tool should be allowed,
/// require user confirmation, or be denied with specific reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionEvalResult {
    /// Tool is allowed to execute without user confirmation
    Allow,
    /// Tool requires user confirmation before execution
    Ask,
    /// Denial with specific reasons explaining why the tool was denied
    ///
    /// Tools are free to overload what these reasons are
    Deny { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputItem {
    Text(String),
    Image(ImageBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AgentResponse {
    Success,
    Snapshot(AgentSnapshot),
    McpPrompts(HashMap<String, Vec<Prompt>>),
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentError {
    #[error("Agent is not idle")]
    NotIdle,
    #[error("{}", .0)]
    AgentLoopError(#[from] LoopError),
    #[error("{}", .0)]
    AgentLoopResponse(#[from] AgentLoopResponseError),
    #[error("An error occurred with an MCP server: {}", .0)]
    McpManager(#[from] McpManagerError),
    #[error("The agent channel has closed")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}

impl From<String> for AgentError {
    fn from(value: String) -> Self {
        Self::Custom(value)
    }
}
