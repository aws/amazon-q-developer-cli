use std::collections::HashMap;
use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

use super::ExecutionState;
use super::agent_config::definitions::AgentConfig;
use super::agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopResponseError,
    LoopError,
    SendRequestArgs,
    UserTurnMetadata,
};
use super::agent_loop::types::{
    ImageBlock,
    ToolResultBlock,
    ToolResultContentBlock,
    ToolResultStatus,
    ToolUseBlock,
};
use super::event_log::LogEntry;
use super::mcp::types::Prompt;
use super::mcp::{
    McpManagerError,
    McpServerEvent,
};
use super::task_executor::TaskExecutorEvent;
use super::tools::summary::Summary;
use super::tools::use_subagent::SubagentRequest;
use super::tools::{
    Tool,
    ToolExecutionError,
    ToolExecutionOutput,
};
use super::types::AgentSnapshot;

/// Represents a message from the agent to the client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum AgentEvent {
    /// Update events to be surfaced prior to an agent being fully initialized
    ///
    /// This is the first event(s) the agent will emit.
    InitializeUpdate(InitializeUpdateEvent),

    /// Agent has finished initialization, and is ready to receive requests.
    Initialized,

    /// Real-time updates about the session.
    ///
    /// This includes:
    /// * Assistant content (primarily just Text)
    /// * Tool calls
    /// * User message chunks (for use when replaying a previous conversation)
    Update(UpdateEvent),

    /// The agent has stopped execution.
    Stop(AgentStopReason),

    /// The user turn has ended. Metadata about the turn's execution is provided.
    ///
    /// This event is emitted in the following scenarios:
    /// * The user turn has ended successfully
    /// * The user cancelled the agent's execution
    /// * The agent encountered an error, and the user sends a new prompt.
    ///
    /// Note that a turn can continue even after a [AgentEvent::Stop] for when the agent encounters
    /// an error, and the next prompt chooses to continue the turn.
    EndTurn(UserTurnMetadata),

    /// A permission request to the client for using a specific tool.
    ApprovalRequest {
        /// Id for the approval request
        id: String,
        /// The tool use to be approved or denied
        tool_use: ToolUseBlock,
        /// Tool-specific context about the requested operation
        context: Option<super::tools::ToolContext>,
    },

    /// Lower-level events associated with the agent's execution. Generally only useful for
    /// debugging or telemetry purposes.
    Internal(InternalEvent),

    /// Events from MCP (Model Context Protocol) servers
    Mcp(McpServerEvent),

    /// Summary of a subagent's execution
    SubagentSummary(Summary),

    /// A log entry was appended to the conversation event log
    LogEntryAppended {
        /// The log entry that was appended
        entry: LogEntry,
        /// Index of the entry in the event log
        index: usize,
    },

    /// Request to spawn subagent(s) - handled by the consumer of agent handle
    SpawnSubagentRequest(SubagentRequest),

    /// Compaction-related events
    Compaction(CompactionEvent),
}

/// Events related to conversation compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompactionEvent {
    /// Compaction has started
    Started,
    /// Compaction completed successfully
    Completed,
    /// Compaction failed
    Failed { error: String },
}

impl From<TaskExecutorEvent> for AgentEvent {
    fn from(value: TaskExecutorEvent) -> Self {
        Self::Internal(InternalEvent::TaskExecutor(Box::new(value)))
    }
}

impl From<AgentLoopEvent> for AgentEvent {
    fn from(value: AgentLoopEvent) -> Self {
        Self::Internal(InternalEvent::AgentLoop(Box::new(value)))
    }
}

impl From<ToolCall> for AgentEvent {
    fn from(value: ToolCall) -> Self {
        Self::Update(UpdateEvent::ToolCall(value))
    }
}

impl From<&Summary> for AgentEvent {
    fn from(value: &Summary) -> Self {
        Self::SubagentSummary(value.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateEvent {
    /// A chunk of the user’s message being streamed.
    UserContent(ContentChunk),
    /// A chunk of the agent’s response being streamed.
    AgentContent(ContentChunk),
    /// A chunk of the agent’s internal reasoning being streamed.
    AgentThought(ContentChunk),
    /// Sent once at the beginning of a tool use.
    ToolCall(ToolCall),
    /// Sent (optionally multiple times) to report the status of a tool execution.
    ToolCallUpdate { content: ContentChunk },
    /// Sent once at the end of a tool execution.
    ToolCallFinished {
        /// The tool that was executed
        tool_call: ToolCall,
        /// The tool execution result
        result: ToolCallResult,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InitializeUpdateEvent {
    Mcp(McpServerEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStopReason {
    /// The turn ended successfully.
    EndTurn,
    /// The turn ended because the agent reached the maximum number of allowed agent requests
    /// between user turns.
    MaxTurnRequests,
    /// The turn was cancelled by the client via a cancellation message.
    Cancelled,
    /// The turn ended because the agent encountered an error.
    Error(AgentError),
}

/// Represents a message from the client to the agent
#[derive(Debug, Clone)]
pub enum AgentRequest {
    /// Send a new prompt
    SendPrompt(SendPromptArgs),
    /// Interrupt the agent's execution
    ///
    /// This will always end the current user turn.
    Cancel,
    SendApprovalResult(SendApprovalResultArgs),
    /// Creates a serializable snapshot of the agent's current state
    CreateSnapshot,
    GetMcpPrompts,
    Terminate,
    /// Swap to a different agent configuration
    SwapAgent(Box<SwapAgentArgs>),
    /// Manually trigger conversation compaction
    CompactConversation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPromptArgs {
    /// Input content
    pub content: Vec<ContentChunk>,
    /// Whether or not the user turn should be continued. Only applies when the agent is in an
    /// errored state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_continue_turn: Option<bool>,
}

impl SendPromptArgs {
    /// Returns the text items of the content joined as a single string, if any text items exist.
    pub fn text(&self) -> Option<String> {
        let text = self
            .content
            .as_slice()
            .iter()
            .filter_map(|c| match c {
                ContentChunk::Text(t) => Some(t.clone()),
                ContentChunk::Image(_) => None,
                ContentChunk::ResourceLink(_) => None,
            })
            .collect::<Vec<_>>();
        if !text.is_empty() { Some(text.join("")) } else { None }
    }

    pub fn should_continue_turn(&self) -> bool {
        self.should_continue_turn.is_some_and(|v| v)
    }
}

impl From<String> for SendPromptArgs {
    fn from(value: String) -> Self {
        Self {
            content: vec![ContentChunk::Text(value)],
            should_continue_turn: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Identifier for the tool call.
    pub id: String,
    /// The tool to execute
    pub tool: Tool,
    /// Original tool use as requested by the model.
    pub tool_use_block: ToolUseBlock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallResult {
    Success(ToolExecutionOutput),
    Error(ToolExecutionError),
    Cancelled,
}

impl ToolCallResult {
    pub fn to_tool_result_block(&self, tool_use_id: &str) -> ToolResultBlock {
        match self {
            ToolCallResult::Success(output) => ToolResultBlock {
                tool_use_id: tool_use_id.to_string(),
                content: output
                    .items
                    .iter()
                    .map(|item| match item {
                        super::tools::ToolExecutionOutputItem::Text(t) => ToolResultContentBlock::Text(t.clone()),
                        super::tools::ToolExecutionOutputItem::Json(v) => ToolResultContentBlock::Json(v.clone()),
                        super::tools::ToolExecutionOutputItem::Image(img) => ToolResultContentBlock::Image(img.clone()),
                    })
                    .collect(),
                status: ToolResultStatus::Success,
            },
            ToolCallResult::Error(err) => ToolResultBlock {
                tool_use_id: tool_use_id.to_string(),
                content: vec![ToolResultContentBlock::Text(err.to_string())],
                status: ToolResultStatus::Error,
            },
            ToolCallResult::Cancelled => ToolResultBlock {
                tool_use_id: tool_use_id.to_string(),
                content: vec![ToolResultContentBlock::Text(
                    "Tool use was cancelled by the user".to_string(),
                )],
                status: ToolResultStatus::Error,
            },
        }
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
pub enum ContentChunk {
    Text(String),
    Image(ImageBlock),
    ResourceLink(String),
}

impl From<String> for ContentChunk {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<ImageBlock> for ContentChunk {
    fn from(value: ImageBlock) -> Self {
        Self::Image(value)
    }
}

/// Arguments for swapping to a different agent configuration
#[derive(Debug, Clone)]
pub struct SwapAgentArgs {
    /// The new agent configuration to use
    pub agent_config: AgentConfig,
    /// Path to workspace-level mcp.json
    pub local_mcp_path: Option<PathBuf>,
    /// Path to global mcp.json
    pub global_mcp_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AgentResponse {
    Success,
    Snapshot(AgentSnapshot),
    McpPrompts(HashMap<String, Vec<Prompt>>),
    TerminateAcknowledged,
    SwapComplete,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalEvent {
    /// Low-level events associated with the agent loop.
    ///
    /// These events contain information about the model's response, including:
    /// - Text content
    /// - Tool uses
    /// - Metadata about a response stream, and about a complete user turn
    AgentLoop(Box<AgentLoopEvent>),
    /// The exact request sent to the backend
    RequestSent(SendRequestArgs),
    /// The agent has changed state.
    StateChange { from: ExecutionState, to: ExecutionState },
    /// A tool use was requested by the model, and the permission was evaluated
    ToolPermissionEvalResult { tool: Tool, result: PermissionEvalResult },
    /// Events specific to tool and hook execution
    TaskExecutor(Box<TaskExecutorEvent>),
}
