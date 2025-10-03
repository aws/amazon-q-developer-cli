//! This is largely based on https://docs.ag-ui.com/concepts/events
//! They do not have a rust SDK so for now we are handrolling these types

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

/// Role of a message sender
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    Developer,
    System,
    Assistant,
    User,
    Tool,
}

/// Base properties shared by all events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<Value>,
}

// ============================================================================
// Lifecycle Events
// ============================================================================

/// Signals the start of an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStarted {
    pub thread_id: String,
    pub run_id: String,
    // Extended fields (draft)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
}

/// Signals the successful completion of an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFinished {
    pub thread_id: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    // Extended fields (draft)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>, // "success" or "interrupt"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<Value>,
}

/// Signals an error during an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Signals the start of a step within an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepStarted {
    pub step_name: String,
}

/// Signals the completion of a step within an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepFinished {
    pub step_name: String,
}

// ============================================================================
// Text Message Events
// ============================================================================

/// Signals the start of a text message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMessageStart {
    pub message_id: String,
    pub role: MessageRole,
}

/// Represents a chunk of content in a streaming text message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMessageContent {
    pub message_id: String,
    pub delta: String,
}

/// Signals the end of a text message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMessageEnd {
    pub message_id: String,
}

/// A self-contained text message event that combines start, content, and end
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMessageChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MessageRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
}

// ============================================================================
// Tool Call Events
// ============================================================================

/// Signals the start of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallStart {
    pub tool_call_id: String,
    pub tool_call_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
}

/// Represents a chunk of argument data for a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallArgs {
    pub tool_call_id: String,
    pub delta: Value,
}

/// Signals the end of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEnd {
    pub tool_call_id: String,
}

/// Provides the result of a tool call execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub message_id: String,
    pub tool_call_id: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MessageRole>,
}

// ============================================================================
// State Management Events
// ============================================================================

/// Provides a complete snapshot of an agent's state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub snapshot: Value,
}

/// Provides a partial update to an agent's state using JSON Patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub delta: Vec<Value>, // Array of JSON Patch operations (RFC 6902)
}

/// Message object for MessagesSnapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Provides a snapshot of all messages in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesSnapshot {
    pub messages: Vec<Message>,
}

// ============================================================================
// Special Events
// ============================================================================

/// Used to pass through events from external systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Raw {
    pub event: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Used for application-specific custom events
#[derive(Debug, Clone, Serialize, Default, Deserialize)]
pub struct Custom {
    pub name: String,
    pub value: Value,
}

// ============================================================================
// Draft Events - Activity Events
// ============================================================================

/// Provides the complete activity state at a point in time (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshotEvent {
    pub message_id: String,
    pub activity_type: String, // e.g., "PLAN", "SEARCH", "SCRAPE"
    pub content: Value,
}

/// Provides incremental updates to the activity state using JSON Patch operations (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDeltaEvent {
    pub message_id: String,
    pub activity_type: String, // e.g., "PLAN", "SEARCH", "SCRAPE"
    pub patch: Vec<Value>,     // JSON Patch operations (RFC 6902)
}

// ============================================================================
// Draft Events - Reasoning Events
// ============================================================================

/// Marks the start of reasoning (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningStart {
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
}

/// Signals the start of a reasoning message (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningMessageStart {
    pub message_id: String,
    pub role: MessageRole,
}

/// Represents a chunk of content in a streaming reasoning message (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningMessageContent {
    pub message_id: String,
    pub delta: String,
}

/// Signals the end of a reasoning message (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningMessageEnd {
    pub message_id: String,
}

/// A convenience event to auto start/close reasoning messages (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningMessageChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
}

/// Marks the end of reasoning (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEnd {
    pub message_id: String,
}

// ============================================================================
// Draft Events - Meta Events
// ============================================================================

/// A side-band annotation event that can occur anywhere in the stream (DRAFT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaEvent {
    pub meta_type: String, // e.g., "thumbs_up", "tag"
    pub payload: Value,
}

// ============================================================================
// Main Event Enum
// ============================================================================

/// Main event enum that encompasses all event types in the Agent UI Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    // Lifecycle Events
    RunStarted(RunStarted),
    RunFinished(RunFinished),
    RunError(RunError),
    StepStarted(StepStarted),
    StepFinished(StepFinished),

    // Text Message Events
    TextMessageStart(TextMessageStart),
    TextMessageContent(TextMessageContent),
    TextMessageEnd(TextMessageEnd),
    TextMessageChunk(TextMessageChunk),

    // Tool Call Events
    ToolCallStart(ToolCallStart),
    ToolCallArgs(ToolCallArgs),
    ToolCallEnd(ToolCallEnd),
    ToolCallResult(ToolCallResult),

    // State Management Events
    StateSnapshot(StateSnapshot),
    StateDelta(StateDelta),
    MessagesSnapshot(MessagesSnapshot),

    // Special Events
    Raw(Raw),
    Custom(Custom),

    // Draft Events - Activity Events
    ActivitySnapshotEvent(ActivitySnapshotEvent),
    ActivityDeltaEvent(ActivityDeltaEvent),

    // Draft Events - Reasoning Events
    ReasoningStart(ReasoningStart),
    ReasoningMessageStart(ReasoningMessageStart),
    ReasoningMessageContent(ReasoningMessageContent),
    ReasoningMessageEnd(ReasoningMessageEnd),
    ReasoningMessageChunk(ReasoningMessageChunk),
    ReasoningEnd(ReasoningEnd),

    // Draft Events - Meta Events
    MetaEvent(MetaEvent),
}

impl Event {
    /// Get the event type string for this event
    pub fn event_type(&self) -> &'static str {
        match self {
            // Lifecycle Events
            Event::RunStarted(_) => "runStarted",
            Event::RunFinished(_) => "runFinished",
            Event::RunError(_) => "runError",
            Event::StepStarted(_) => "stepStarted",
            Event::StepFinished(_) => "stepFinished",

            // Text Message Events
            Event::TextMessageStart(_) => "textMessageStart",
            Event::TextMessageContent(_) => "textMessageContent",
            Event::TextMessageEnd(_) => "textMessageEnd",
            Event::TextMessageChunk(_) => "textMessageChunk",

            // Tool Call Events
            Event::ToolCallStart(_) => "toolCallStart",
            Event::ToolCallArgs(_) => "toolCallArgs",
            Event::ToolCallEnd(_) => "toolCallEnd",
            Event::ToolCallResult(_) => "toolCallResult",

            // State Management Events
            Event::StateSnapshot(_) => "stateSnapshot",
            Event::StateDelta(_) => "stateDelta",
            Event::MessagesSnapshot(_) => "messagesSnapshot",

            // Special Events
            Event::Raw(_) => "raw",
            Event::Custom(_) => "custom",

            // Draft Events - Activity Events
            Event::ActivitySnapshotEvent(_) => "activitySnapshotEvent",
            Event::ActivityDeltaEvent(_) => "activityDeltaEvent",

            // Draft Events - Reasoning Events
            Event::ReasoningStart(_) => "reasoningStart",
            Event::ReasoningMessageStart(_) => "reasoningMessageStart",
            Event::ReasoningMessageContent(_) => "reasoningMessageContent",
            Event::ReasoningMessageEnd(_) => "reasoningMessageEnd",
            Event::ReasoningMessageChunk(_) => "reasoningMessageChunk",
            Event::ReasoningEnd(_) => "reasoningEnd",

            // Draft Events - Meta Events
            Event::MetaEvent(_) => "metaEvent",
        }
    }

    pub fn is_compatible_with_legacy_event_loop(&self) -> bool {
        matches!(
            self,
            Event::Custom(_) | Event::TextMessageContent(_) | Event::TextMessageChunk(_)
        )
    }

    /// Check if this is a lifecycle event
    pub fn is_lifecycle_event(&self) -> bool {
        matches!(
            self,
            Event::RunStarted(_)
                | Event::RunFinished(_)
                | Event::RunError(_)
                | Event::StepStarted(_)
                | Event::StepFinished(_)
        )
    }

    /// Check if this is a text message event
    pub fn is_text_message_event(&self) -> bool {
        matches!(
            self,
            Event::TextMessageStart(_)
                | Event::TextMessageContent(_)
                | Event::TextMessageEnd(_)
                | Event::TextMessageChunk(_)
        )
    }

    /// Check if this is a tool call event
    pub fn is_tool_call_event(&self) -> bool {
        matches!(
            self,
            Event::ToolCallStart(_) | Event::ToolCallArgs(_) | Event::ToolCallEnd(_) | Event::ToolCallResult(_)
        )
    }

    /// Check if this is a state management event
    pub fn is_state_management_event(&self) -> bool {
        matches!(
            self,
            Event::StateSnapshot(_) | Event::StateDelta(_) | Event::MessagesSnapshot(_)
        )
    }

    /// Check if this is a draft event (experimental/unstable)
    pub fn is_draft_event(&self) -> bool {
        matches!(
            self,
            Event::ActivitySnapshotEvent(_)
                | Event::ActivityDeltaEvent(_)
                | Event::ReasoningStart(_)
                | Event::ReasoningMessageStart(_)
                | Event::ReasoningMessageContent(_)
                | Event::ReasoningMessageEnd(_)
                | Event::ReasoningMessageChunk(_)
                | Event::ReasoningEnd(_)
                | Event::MetaEvent(_)
        )
    }
}
