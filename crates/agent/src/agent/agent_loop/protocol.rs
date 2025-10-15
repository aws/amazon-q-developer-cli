use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;

use super::model::AgentLoopModel;
use super::types::{
    Message,
    MetadataEvent,
    StreamError,
    StreamEvent,
    ToolSpec,
    ToolUseBlock,
};
use super::{
    AgentLoopId,
    InvalidToolUse,
    LoopState,
};

#[derive(Debug)]
pub enum AgentLoopRequest {
    GetExecutionState,
    SendRequest {
        model: Box<dyn AgentLoopModel>,
        args: SendRequestArgs,
    },
    /// Ends the agent loop
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequestArgs {
    pub messages: Vec<Message>,
    pub tool_specs: Option<Vec<ToolSpec>>,
    pub system_prompt: Option<String>,
}

impl SendRequestArgs {
    pub fn new(messages: Vec<Message>, tool_specs: Option<Vec<ToolSpec>>, system_prompt: Option<String>) -> Self {
        Self {
            messages,
            tool_specs,
            system_prompt,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentLoopResponse {
    Success,
    ExecutionState(LoopState),
    StreamMetadata(Vec<StreamMetadata>),
    PendingToolUses(Option<Vec<ToolUseBlock>>),
    Metadata(Box<UserTurnMetadata>),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentLoopResponseError {
    #[error("A response stream is currently being consumed")]
    StreamCurrentlyExecuting,
    #[error("The agent loop has already exited")]
    AgentLoopExited,
    #[error("{}", .0)]
    Custom(String),
}

impl<T> From<mpsc::error::SendError<T>> for AgentLoopResponseError {
    fn from(value: mpsc::error::SendError<T>) -> Self {
        Self::Custom(format!("channel failure: {}", value))
    }
}

/// An event about a specific agent loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoopEvent {
    /// The identifier of the agent loop
    pub id: AgentLoopId,
    /// The kind of event
    pub kind: AgentLoopEventKind,
}

impl AgentLoopEvent {
    pub fn new(id: AgentLoopId, kind: AgentLoopEventKind) -> Self {
        Self { id, kind }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentLoopEventKind {
    /// Text returned by the assistant.
    AssistantText(String),
    /// Contains content regarding the reasoning that is carried out by the model. Reasoning refers
    /// to a Chain of Thought (CoT) that the model generates to enhance the accuracy of its final
    /// response.
    ReasoningContent(String),
    /// Notification that a tool use is being received
    ToolUseStart {
        /// Tool use id
        id: String,
        /// Tool name
        name: String,
    },
    /// A valid tool use was received
    ToolUse(ToolUseBlock),
    /// A single request/response stream has completed processing.
    ///
    /// When emitted, the agent loop is in either of the states:
    /// 1. User turn is ongoing (due to tool uses or a stream error), and the loop is ready to
    ///    receive a new request.
    /// 2. User turn has ended, in which case a [AgentLoopEventKind::UserTurnEnd] event is emitted
    ///    afterwards. The loop is still able to receive new requests which will continue the user
    ///    turn.
    ResponseStreamEnd {
        /// The result of having parsed the entire stream.
        ///
        /// On success, a new assistant response message is available for storing in the
        /// conversation history. Otherwise, the corresponding [LoopError] is returned.
        result: Result<Message, LoopError>,
        /// Metadata about the stream.
        metadata: StreamMetadata,
    },
    /// Metadata for the entire user turn.
    ///
    /// This is the last event that the agent loop will emit, unless another request is sent that
    /// continues the turn.
    UserTurnEnd(UserTurnMetadata),
    /// The agent loop has changed states
    LoopStateChange { from: LoopState, to: LoopState },
    /// Low level event. Generally only useful for [AgentLoop].
    StreamEvent(StreamEvent),
    /// Low level event. Generally only useful for [AgentLoop].
    StreamError(StreamError),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum LoopError {
    /// The response stream produced invalid JSON.
    #[error("The model produced invalid JSON")]
    InvalidJson {
        /// Received assistant text
        assistant_text: String,
        /// Tool uses that consist of invalid JSON
        invalid_tools: Vec<InvalidToolUse>,
    },
    /// Errors associated with the underlying response stream.
    ///
    /// Most errors will be sourced from here.
    #[error("{}", .0)]
    Stream(#[from] StreamError),
}

/// Contains useful metadata about a single model response stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    /// Tool uses returned from this stream
    pub tool_uses: Vec<ToolUseBlock>,
    /// Metadata about the underlying stream
    pub stream: Option<MetadataEvent>,
}

#[derive(Debug, Clone)]
pub struct ResponseStreamEnd {
    /// The response message
    pub message: Message,
    /// Metadata about the response stream
    pub metadata: Option<MetadataEvent>,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{}", source)]
pub struct AgentLoopError {
    #[source]
    source: StreamError,
}

/// Metadata and statistics about the agent loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTurnMetadata {
    /// Identifier of the associated agent loop
    pub loop_id: AgentLoopId,
    /// Final result of the user turn
    ///
    /// Only [None] if the loop never executed anything - ie, end reason is [EndReason::DidNotRun]
    pub result: Option<Result<Message, LoopError>>,
    /// The id of each message as part of the user turn, in order
    ///
    /// Messages with no id will be included in this vector as [None]
    pub message_ids: Vec<Option<String>>,
    /// The number of requests sent to the model
    pub total_request_count: u32,
    /// The number of tool use / tool result pairs in the turn
    pub number_of_cycles: u32,
    /// Total length of time spent in the user turn until completion
    pub turn_duration: Option<Duration>,
    /// Why the user turn ended
    pub end_reason: EndReason,
    pub end_timestamp: DateTime<Utc>,
}

/// The reason why a user turn ended
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EndReason {
    /// Loop ended before handling any requests
    DidNotRun,
    /// The loop ended because the model responded with no tool uses
    UserTurnEnd,
    /// Loop was waiting for tool use results to be provided
    ToolUseRejected,
    /// Loop errored out
    Error,
    /// Loop was executing but was subsequently cancelled
    Cancelled,
}
