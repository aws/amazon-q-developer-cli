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
#[derive(Debug, Clone, Serialize, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMessageContent {
    pub message_id: String,
    pub delta: Vec<u8>,
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
    // bespoke fields
    pub mcp_server_name: Option<String>,
    pub is_trusted: bool,
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

/// Signifies a rejection to a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRejection {
    pub tool_call_id: String,
    pub name: String,
    pub reason: String,
}

/// Represents a request for permission to execute a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallPermissionRequest {
    pub tool_call_id: String,
    pub name: String,
    pub input: serde_json::Value,
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

/// Legacy pass-through output for compatibility with older event systems.
///
/// This enum represents different types of output that can be passed through
/// from legacy systems that haven't been fully migrated to the new event protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LegacyPassThroughOutput {
    /// Standard output stream data
    Stdout(Vec<u8>),
    /// Standard error stream data  
    Stderr(Vec<u8>),
}

impl Default for LegacyPassThroughOutput {
    fn default() -> Self {
        Self::Stderr(Default::default())
    }
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

/// Events related to MCP (Model Context Protocol) server lifecycle and operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpEvent {
    Loading { server_name: String },
    LoadSuccess { server_name: String },
    LoadFailure { server_name: String, error: String },
    OauthRequest { server_name: String, oauth_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub agent_id: u16,
    pub kind: AgentEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentEventKind {
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
    // bespoke variant
    ToolCallRejection(ToolCallRejection),
    ToolCallPermissionRequest(ToolCallPermissionRequest),

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

    // Bespoke MCP Events
    McpEvent(McpEvent),
}

// ============================================================================
// Main Event Enum
// ============================================================================

/// Main event enum that encompasses all event types in the Agent UI Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    AgentEvent(AgentEvent),
    LegacyPassThrough(LegacyPassThroughOutput),
}

impl SessionEvent {
    /// Get the event type string for this event
    pub fn event_type(&self) -> &'static str {
        match self {
            SessionEvent::AgentEvent(agent_evt) => match agent_evt.kind {
                // Lifecycle Events
                AgentEventKind::RunStarted(_) => "runStarted",
                AgentEventKind::RunFinished(_) => "runFinished",
                AgentEventKind::RunError(_) => "runError",
                AgentEventKind::StepStarted(_) => "stepStarted",
                AgentEventKind::StepFinished(_) => "stepFinished",

                // Text Message Events
                AgentEventKind::TextMessageStart(_) => "textMessageStart",
                AgentEventKind::TextMessageContent(_) => "textMessageContent",
                AgentEventKind::TextMessageEnd(_) => "textMessageEnd",
                AgentEventKind::TextMessageChunk(_) => "textMessageChunk",

                // Tool Call Events
                AgentEventKind::ToolCallStart(_) => "toolCallStart",
                AgentEventKind::ToolCallArgs(_) => "toolCallArgs",
                AgentEventKind::ToolCallEnd(_) => "toolCallEnd",
                AgentEventKind::ToolCallResult(_) => "toolCallResult",
                AgentEventKind::ToolCallRejection(_) => "toolCallRejection",
                AgentEventKind::ToolCallPermissionRequest(_) => "toolCallPermissionRequest",

                // State Management Events
                AgentEventKind::StateSnapshot(_) => "stateSnapshot",
                AgentEventKind::StateDelta(_) => "stateDelta",
                AgentEventKind::MessagesSnapshot(_) => "messagesSnapshot",

                // Special Events
                AgentEventKind::Raw(_) => "raw",
                AgentEventKind::Custom(_) => "custom",

                // Draft Events - Activity Events
                AgentEventKind::ActivitySnapshotEvent(_) => "activitySnapshotEvent",
                AgentEventKind::ActivityDeltaEvent(_) => "activityDeltaEvent",

                // Draft Events - Reasoning Events
                AgentEventKind::ReasoningStart(_) => "reasoningStart",
                AgentEventKind::ReasoningMessageStart(_) => "reasoningMessageStart",
                AgentEventKind::ReasoningMessageContent(_) => "reasoningMessageContent",
                AgentEventKind::ReasoningMessageEnd(_) => "reasoningMessageEnd",
                AgentEventKind::ReasoningMessageChunk(_) => "reasoningMessageChunk",
                AgentEventKind::ReasoningEnd(_) => "reasoningEnd",

                // Draft Events - Meta Events
                AgentEventKind::MetaEvent(_) => "metaEvent",

                AgentEventKind::McpEvent(_) => "mcpEvent",
            },
            SessionEvent::LegacyPassThrough(_) => "legacyPassThrough",
        }
    }

    pub fn is_compatible_with_legacy_event_loop(&self) -> bool {
        matches!(self, SessionEvent::LegacyPassThrough { .. })
    }

    /// Check if this is a lifecycle event
    pub fn is_lifecycle_event(&self) -> bool {
        matches!(
            self,
            SessionEvent::AgentEvent(AgentEvent {
                kind: AgentEventKind::RunStarted(_)
                    | AgentEventKind::RunFinished(_)
                    | AgentEventKind::RunError(_)
                    | AgentEventKind::StepStarted(_)
                    | AgentEventKind::StepFinished(_),
                ..
            })
        )
    }

    /// Check if this is a text message event
    pub fn is_text_message_event(&self) -> bool {
        matches!(
            self,
            SessionEvent::AgentEvent(AgentEvent {
                kind: AgentEventKind::TextMessageStart(_)
                    | AgentEventKind::TextMessageContent(_)
                    | AgentEventKind::TextMessageEnd(_)
                    | AgentEventKind::TextMessageChunk(_),
                ..
            })
        )
    }

    /// Check if this is a tool call event
    pub fn is_tool_call_event(&self) -> bool {
        matches!(
            self,
            SessionEvent::AgentEvent(AgentEvent {
                kind: AgentEventKind::ToolCallStart(_)
                    | AgentEventKind::ToolCallArgs(_)
                    | AgentEventKind::ToolCallEnd(_)
                    | AgentEventKind::ToolCallResult(_),
                ..
            })
        )
    }

    /// Check if this is a state management event
    pub fn is_state_management_event(&self) -> bool {
        matches!(
            self,
            SessionEvent::AgentEvent(AgentEvent {
                kind: AgentEventKind::StateSnapshot(_)
                    | AgentEventKind::StateDelta(_)
                    | AgentEventKind::MessagesSnapshot(_),
                ..
            })
        )
    }

    /// Check if this is a draft event (experimental/unstable)
    pub fn is_draft_event(&self) -> bool {
        matches!(
            self,
            SessionEvent::AgentEvent(AgentEvent {
                kind: AgentEventKind::ActivitySnapshotEvent(_)
                    | AgentEventKind::ActivityDeltaEvent(_)
                    | AgentEventKind::ReasoningStart(_)
                    | AgentEventKind::ReasoningMessageStart(_)
                    | AgentEventKind::ReasoningMessageContent(_)
                    | AgentEventKind::ReasoningMessageEnd(_)
                    | AgentEventKind::ReasoningMessageChunk(_)
                    | AgentEventKind::ReasoningEnd(_)
                    | AgentEventKind::MetaEvent(_),
                ..
            })
        )
    }
}

/// This is a stop gap until we adopt ACP
/// This will likely be done when UI revamp is done
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub struct InputEvent {
    pub agent_id: Option<u16>,
    pub kind: InputEventKind,
}

/// This is a stop gap until we adopt ACP
/// This will likely be done when UI revamp is done
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InputEventKind {
    Text(String),
    Interrupt,
    ToolApproval(String),
    ToolRejection(String),
}
