use std::sync::Arc;
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
use typeshare::typeshare;

use super::model::Model;
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
        model: Arc<dyn Model>,
        args: SendRequestArgs,
    },
    /// Ends the agent loop
    Cancel,
}

/// Represents a request to send to the backend model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequestArgs {
    pub messages: Vec<Message>,
    pub tool_specs: Option<Vec<ToolSpec>>,
    pub system_prompt: Option<String>,
    /// Context usage percentage reported by the backend for the most recent completed
    /// turn, if any.
    ///
    /// Used to synthesize a context overflow before dispatching (see
    /// [`SYNTHETIC_OVERFLOW_THRESHOLD`]). The compaction request hardcodes `None` so it
    /// can never trigger against itself. Post-compaction retries are `None` because
    /// successful compaction clears the stored reading before the retry is formatted,
    /// not because of anything at this call site.
    ///
    /// [`SYNTHETIC_OVERFLOW_THRESHOLD`]: crate::agent::consts::SYNTHETIC_OVERFLOW_THRESHOLD
    #[serde(default)]
    pub context_usage_percentage: Option<f32>,
}

impl SendRequestArgs {
    pub fn new(messages: Vec<Message>, tool_specs: Option<Vec<ToolSpec>>, system_prompt: Option<String>) -> Self {
        Self {
            messages,
            tool_specs,
            system_prompt,
            context_usage_percentage: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentLoopResponse {
    Success,
    ExecutionState(LoopState),
    StreamMetadata(Vec<StreamMetadata>),
    PendingToolUses(Option<Vec<ToolUseBlock>>),
    UserTurnMetadata(Box<UserTurnMetadata>),
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
        Self::Custom(format!("channel failure: {value}"))
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
#[serde(tag = "kind", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum AgentLoopEventKind {
    /// Text returned by the assistant.
    AssistantText(String),
    /// Contains content regarding the reasoning that is carried out by the model. Reasoning refers
    /// to a Chain of Thought (CoT) that the model generates to enhance the accuracy of its final
    /// response.
    ReasoningContent(String),
    /// Streaming thinking text from extended thinking models (interleaved thinking).
    ThinkingText(String),
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
    /// This event encompasses:
    /// * Successful requests and response streams
    /// * Errors in sending the request
    /// * Errors while processing the response stream
    ///
    /// Success or failure is given by the `result` field.
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
    /// The response stream has been silent for longer than the soft idle threshold.
    ///
    /// Informational only — the stream remains open and may still recover. Consumers can
    /// surface a "still waiting" indicator, or ignore the event entirely.
    StreamStallWarning {
        /// How long the stream has been silent.
        idle: Duration,
    },
    /// The response stream produced an event again after a [`Self::StreamStallWarning`]
    /// had fired, ending the stall episode without a hard cancel.
    ///
    /// Informational only; carries the silent gap that just ended so consumers can
    /// record observed stall durations.
    StreamStallResumed {
        /// Length of the silent gap that just ended.
        idle: Duration,
    },
    /// The stream ended in an error or EOF after a [`Self::StreamStallWarning`] had
    /// fired: the stall episode is over, but not because the stream recovered.
    ///
    /// Informational only; exists so every warned episode gets an episode-end record
    /// and the stall series stays reconcilable (soft stalls == episode ends).
    StreamStallFailed {
        /// Length of the silent gap the failure ended.
        idle: Duration,
    },
    /// A cancel landed while a [`Self::StreamStallWarning`] was outstanding: the
    /// user (or a teardown) abandoned the stalled stream before it recovered or
    /// died on its own — the natural response to the "model has gone quiet"
    /// notice, and distinct signal for tuning the soft threshold.
    ///
    /// Informational only; the third episode-end producer alongside
    /// [`Self::StreamStallResumed`] and [`Self::StreamStallFailed`].
    StreamStallCancelled {
        /// Length of the silent gap the cancel ended.
        idle: Duration,
    },
    /// The agent loop has changed states
    LoopStateChange { from: LoopState, to: LoopState },
    /// Low level event. Generally only useful for [AgentLoop].
    ///
    /// This reflects the exact event the agent loop parses from a [Model::stream] response as part
    /// of executing a user turn.
    Stream(StreamResult),
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", content = "data")]
#[serde(rename_all = "lowercase")]
pub enum StreamResult {
    Ok(StreamEvent),
    #[serde(rename = "error")]
    Err(StreamError),
}

impl StreamResult {
    pub fn unwrap_err(self) -> StreamError {
        match self {
            StreamResult::Ok(t) => panic!("called `StreamResult::unwrap_err()` on an `Ok` value: {:?}", &t),
            StreamResult::Err(e) => e,
        }
    }
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
        /// Tool uses that were successfully parsed before the invalid ones
        valid_tools: Vec<ToolUseBlock>,
    },
    /// The response stream completed cleanly but produced no content (no text, no tool
    /// uses, no thinking blocks).
    ///
    /// # Context
    ///
    /// This is the client-side fingerprint of Bedrock's `stopReason=content_filtered`
    /// outcome: HTTP 200, MeteringEvent + MetadataEvent only, no AssistantResponseEvent or
    /// ToolUseEvent.
    #[error("Kiro failed to generate a response")]
    EmptyResponse,
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
    /// Number of HTTP-level attempts (1 = no retry, 2+ = retried). `None` when unknown
    /// (e.g. validation errors that short-circuit before the request is dispatched, or
    /// in contexts where the transport layer doesn't report it).
    #[serde(default)]
    pub request_attempts: Option<u32>,
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
    /// Ids of messages exchanged with the model during this turn, in order.
    ///
    /// Each request in the turn contributes a `[user_prompt_id, assistant_response_id]` pair: the
    /// id of the user prompt (or tool results) actively sent as that request's prompt, followed by
    /// the id of the assistant response. Either entry is `None` when the corresponding message had
    /// no id.
    ///
    /// These ids match the `Message.id` of the corresponding `LogEntry` in the conversation log.
    /// Synthetic messages appended to the log purely as history (e.g. placeholders inserted on
    /// cancel, timeout, or InvalidJson recovery) are not included here, since they were never the
    /// active prompt or response of a request.
    pub message_ids: Vec<Option<String>>,
    /// The number of requests sent to the model
    pub total_request_count: u32,
    /// The number of tool use / tool result pairs in the turn
    pub number_of_cycles: u32,
    /// The number of tool uses that belong to a native tool
    pub builtin_tool_uses: u32,
    /// Total length of time spent in the user turn until completion
    pub turn_duration: Option<Duration>,
    /// Why the user turn ended
    pub end_reason: LoopEndReason,
    pub end_timestamp: DateTime<Utc>,
    /// Input token count associated with the turn
    pub input_token_count: u32,
    /// Output token count associated with the turn
    pub output_token_count: u32,
    /// Cache-read input token count associated with the turn
    #[serde(default)]
    pub cache_read_input_token_count: u32,
    /// Cache-write input token count associated with the turn
    #[serde(default)]
    pub cache_write_input_token_count: u32,
    /// Model used for the turn
    #[serde(default)]
    pub model: Option<String>,
    /// Total assistant response length in bytes
    #[serde(default)]
    pub assistant_response_length: usize,
    /// Total HTTP attempts across requests in the turn
    #[serde(default)]
    pub request_attempts: Option<u32>,
    /// Context usage percentage (0-100)
    pub context_usage_percentage: Option<f32>,
    /// Context usage percentage reported by the turn's **final** response, or `None` if
    /// that response reported none.
    ///
    /// Distinct from [`Self::context_usage_percentage`], which keeps the last value
    /// reported by any request in the turn. A compaction can run part way through a
    /// turn, so only the final response's reading describes the history as it stands
    /// when the turn ends. Used to decide whether to synthesize a context overflow on
    /// the next request; the aggregate above remains the telemetry value.
    #[serde(default)]
    pub final_context_usage_percentage: Option<f32>,
    /// Metering usage (credits) accumulated across all requests in this turn
    #[serde(default)]
    pub metering_usage: Vec<super::types::MeteringUsageInfo>,
    /// Byte length of the original user prompt text for this turn.
    #[serde(default)]
    pub user_prompt_length: usize,
}

/// The reason why a user turn ended
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display, strum::AsRefStr)]
pub enum LoopEndReason {
    /// Loop ended before handling any requests
    DidNotRun,
    /// The loop ended because the model responded with no tool uses
    UserTurnEnd,
    /// Loop was waiting for tool use results to be provided
    ToolUseRejected,
    /// Loop errored out
    Error,
    /// Loop was processing a response stream but was cancelled
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_request_args_new() {
        let args = SendRequestArgs::new(vec![], None, None);
        assert!(args.messages.is_empty());
        assert!(args.tool_specs.is_none());
        assert!(args.system_prompt.is_none());
    }

    #[test]
    fn test_send_request_args_with_data() {
        let args = SendRequestArgs::new(vec![], Some(vec![]), Some("sys".to_string()));
        assert_eq!(args.system_prompt, Some("sys".to_string()));
        assert!(args.tool_specs.is_some());
    }

    #[test]
    fn test_agent_loop_response_error_display() {
        assert_eq!(
            AgentLoopResponseError::AgentLoopExited.to_string(),
            "The agent loop has already exited"
        );
        assert_eq!(
            AgentLoopResponseError::StreamCurrentlyExecuting.to_string(),
            "A response stream is currently being consumed"
        );
        assert_eq!(AgentLoopResponseError::Custom("x".into()).to_string(), "x");
    }

    #[test]
    fn test_agent_loop_response_error_from_send_error() {
        let (tx, rx) = mpsc::channel::<i32>(1);
        drop(rx);
        // tokio::runtime::Runtime needed for blocking_send
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { tx.send(1).await });
        if let Err(send_err) = result {
            let e: AgentLoopResponseError = send_err.into();
            assert!(matches!(e, AgentLoopResponseError::Custom(_)));
        }
    }

    #[test]
    fn test_loop_end_reason_display_and_parse() {
        assert_eq!(LoopEndReason::DidNotRun.to_string(), "DidNotRun");
        assert_eq!(LoopEndReason::UserTurnEnd.to_string(), "UserTurnEnd");
        assert_eq!(LoopEndReason::ToolUseRejected.to_string(), "ToolUseRejected");
        assert_eq!(LoopEndReason::Error.to_string(), "Error");
        assert_eq!(LoopEndReason::Cancelled.to_string(), "Cancelled");

        let parsed: LoopEndReason = "DidNotRun".parse().unwrap();
        assert_eq!(parsed, LoopEndReason::DidNotRun);
    }

    #[test]
    fn test_loop_end_reason_serde() {
        let r = LoopEndReason::Error;
        let json = serde_json::to_string(&r).unwrap();
        let parsed: LoopEndReason = serde_json::from_str(&json).unwrap();
        assert_eq!(r, parsed);
    }

    #[test]
    fn test_agent_loop_event_new() {
        let id = AgentLoopId::new(crate::agent::AgentId::default());
        let e = AgentLoopEvent::new(id, AgentLoopEventKind::AssistantText("hello".to_string()));
        assert!(matches!(e.kind, AgentLoopEventKind::AssistantText(_)));
    }

    #[test]
    fn test_agent_loop_event_kind_serde_assistant_text() {
        let e = AgentLoopEventKind::AssistantText("Hi".to_string());
        let json = serde_json::to_string(&e).unwrap();
        let parsed: AgentLoopEventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, AgentLoopEventKind::AssistantText(_)));
    }

    #[test]
    fn test_agent_loop_event_kind_serde_reasoning() {
        let e = AgentLoopEventKind::ReasoningContent("thinking...".to_string());
        let json = serde_json::to_string(&e).unwrap();
        let parsed: AgentLoopEventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, AgentLoopEventKind::ReasoningContent(_)));
    }

    #[test]
    fn test_agent_loop_event_kind_serde_tool_use_start() {
        let e = AgentLoopEventKind::ToolUseStart {
            id: "id1".into(),
            name: "fs_read".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("toolUseStart"));
    }

    #[test]
    fn test_stream_result_unwrap_err_on_err() {
        use super::super::types::{
            StreamError,
            StreamErrorKind,
        };
        let err = StreamError {
            original_request_id: None,
            original_status_code: None,
            original_message: Some("x".into()),
            kind: StreamErrorKind::Interrupted,
            source: None,
        };
        let r = StreamResult::Err(err);
        let unwrapped = r.unwrap_err();
        assert_eq!(unwrapped.original_message, Some("x".into()));
    }

    #[test]
    fn test_loop_error_display_invalid_json() {
        let err = LoopError::InvalidJson {
            assistant_text: "x".into(),
            invalid_tools: vec![],
            valid_tools: vec![],
        };
        assert_eq!(err.to_string(), "The model produced invalid JSON");
    }
}
