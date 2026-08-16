use std::collections::HashMap;
use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use super::ExecutionState;
use super::agent_config::LoadedAgentConfig;
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
use super::tools::session::SessionToolRequest;
use super::tools::summary::Summary;
use super::tools::{
    Tool,
    ToolCallIdentity,
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
    ApprovalRequest(ApprovalRequest),

    /// Lower-level events associated with the agent's execution. Generally only useful for
    /// debugging or telemetry purposes.
    Internal(InternalEvent),

    /// Events from MCP (Model Context Protocol) servers
    Mcp(McpServerEvent),

    /// Summary of a subagent's execution
    SubagentSummary(Summary),

    /// Agent invoked the goal tool (complete, status, create, clear)
    GoalAction(crate::agent::tools::goal::GoalTool),

    /// A log entry was appended to the conversation event log
    LogEntryAppended {
        /// The log entry that was appended
        entry: LogEntry,
        /// Index of the entry in the event log
        index: usize,
    },

    /// Request for session management operations - handled by the ACP layer
    SessionToolRequest(SessionToolRequest),

    /// Compaction-related events
    Compaction(CompactionEvent),

    /// Clear-related events
    Clear(ClearEvent),

    /// A steering message was queued (for TUI display).
    ///
    /// `content` is the **full current queue snapshot** (multiple steers
    /// concatenated with `"\n\n"`). Consumers SHOULD overwrite their local
    /// copy rather than append, since each emission carries the complete
    /// queue state. `message_id` is the stable `steer-<uuid>` id of the steer
    /// that was just queued, so consumers can correlate it with the matching
    /// consume/clear notification by id.
    ///
    /// Maps onto the KAS `AgentExecutionUserMessageQueued` ACP notification.
    SteeringQueued { message_id: String, content: String },

    /// A queued steering message was consumed and injected into the
    /// conversation (either at a tool boundary or at the start of an
    /// auto-started turn at end-of-turn drain).
    ///
    /// Emitted once **per** queued steer (not once per drain), carrying that
    /// steer's stable `message_id` and its raw `content`. This mirrors the
    /// KAS `AgentExecutionSteeringInjected` notification, which tracks each
    /// steer by id. The drained steers are still concatenated into a single
    /// LLM continuation request — only the notifications are per-steer.
    SteeringConsumed { message_id: String, content: String },

    /// The queued steering messages were cleared without being consumed
    /// (e.g. via cancel, or via explicit user clear from the TUI).
    /// Consumers SHOULD clear their local queue display on receipt.
    ///
    /// `message_ids` lists the stable ids of every steer dropped from the
    /// queue, so a client can reconcile out-of-order delivery. Maps onto the
    /// KAS `AgentExecutionUserMessageCleared` notification.
    SteeringCleared { message_ids: Vec<String> },
}

/// Events related to conversation compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompactionEvent {
    /// Compaction has started
    Started,
    /// Compaction completed successfully
    Completed,
    /// Distinguishes automatic recovery retries from manual compaction.
    ContextRecoveryAttempt { final_attempt: bool },
    /// Compaction failed
    Failed { error: String },
}

/// Events related to conversation clear
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearEvent;

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
    ToolCallUpdate { id: String, content: ContentChunk },
    /// Sent once at the end of a tool execution.
    ToolCallFinished {
        /// The tool that was executed
        tool_call: ToolCall,
        /// The tool execution result
        result: ToolCallResult,
    },
    /// Sent when a tool call fails before execution (parse error, denied, hook rejection).
    /// Unlike ToolCallFinished, this doesn't require a fully parsed Tool.
    ToolCallFailed {
        /// The tool_use_id from the model's response
        tool_use_id: String,
        /// The tool name as requested by the model
        tool_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_identity: Option<ToolCallIdentity>,
        /// The raw input (arguments) the model generated for this tool call.
        /// Included so clients can surface the attempted arguments when the
        /// tool could not be executed.
        raw_input: serde_json::Value,
        /// Why the tool call failed
        reason: ToolCallFailureReason,
        /// Error message
        error: String,
    },
}

/// Why a tool call failed before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallFailureReason {
    /// The tool use block could not be parsed into a valid tool.
    ParseError,
    /// The tool's arguments were forbidden by guardrails.
    PermissionDenied,
    /// A pre-execution hook rejected the tool call.
    HookRejected,
    /// The model called the `dummy` placeholder for a tool that is not
    /// available to the current agent; the call is answered with guidance
    /// instead of executing.
    ToolUnavailable,
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
    /// Get file-based prompts from .kiro/prompts/ directories
    GetFilePrompts,
    /// Get invocable skills from agent_config resources (skill:// URIs)
    GetSkills,
    /// Resolve a skill by name, returning its content (frontmatter stripped)
    ResolveSkill {
        name: String,
    },
    /// Get a specific MCP prompt with arguments
    GetMcpPrompt {
        name: String,
        arguments: HashMap<String, String>,
    },
    Terminate,
    /// Swap to a different agent configuration
    SwapAgent(Box<SwapAgentArgs>),
    /// Push a new MCP registry snapshot to the running agent.
    ///
    /// The agent replaces its stored registry, re-applies it to the current
    /// agent config, and reloads MCP servers. The host does not need to
    /// pre-rewrite the agent config — that's the registry's job. The agent
    /// reuses the `local_mcp_path` / `global_mcp_path` it was constructed
    /// with, so they are not part of this request.
    RefreshMcpRegistry(Box<dyn super::mcp::McpRegistry>),
    /// Reconcile the running MCP servers against a freshly-loaded agent config,
    /// surgically (start added servers, stop removed ones, restart changed ones,
    /// leave unchanged ones running) instead of tearing everything down.
    ///
    /// This is the event-driven path used when a watched config file changes:
    /// it swaps in the new config and reconciles MCP without disturbing servers
    /// whose config is unchanged. Returns [`AgentError::NotIdle`] if the agent
    /// is not idle; callers defer until the next idle window.
    ReconcileMcpServers(Box<LoadedAgentConfig>),
    /// Manually trigger conversation compaction
    CompactConversation,
    /// Clear conversation history
    ClearConversation,
    /// Get information about configured MCP servers
    GetMcpServerInfo,
    /// Force (re-)authentication for a single remote (HTTP) MCP server.
    ReauthMcpServer {
        server_name: String,
    },
    /// Abort an in-flight forced authentication for a single remote (HTTP) MCP server.
    AbortMcpServerAuth {
        server_name: String,
    },
    /// Remove the persisted OAuth credentials (token + dynamic client
    /// registration) for a single remote MCP server. Only valid for remote (HTTP)
    /// servers. Does not stop or relaunch the server.
    RemoveMcpServerCredentials {
        server_name: String,
    },
    /// Get information about available tools
    GetToolInfo,
    /// Add a resource path to the agent's context
    AddResource(String),
    /// Remove a resource path from the agent's context
    RemoveResource(String),
    /// Get the list of current resource paths
    GetResources,
    /// Clear all session-added resources (keep original agent config resources)
    ClearSessionResources,
    /// Get the text of the last assistant message, if any
    GetLastAssistantMessage,
    /// Trust all tools for this session (auto-approve everything)
    TrustAllTools,
    /// Trust specific tools by name (auto-approve future uses)
    TrustTools(Vec<String>),
    /// Untrust specific tools by name (revert to per-request confirmation)
    UntrustTools(Vec<String>),
    /// Reset all tool permissions to config defaults
    ResetToolPermissions,
    /// Set trust_all_tools on this agent (used for "allow all for session")
    SetTrustAllTools(bool),
    InvalidateCachedToolSpecs,
    /// Queue a steering message for injection at the next tool boundary.
    SteerMessage {
        message: String,
    },
    /// Clear any queued steering message without consuming it.
    ClearSteering,
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

/// A permission request to the client for using a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Id for the approval request
    pub id: String,
    /// The tool use block from the model
    pub tool_use: ToolUseBlock,
    /// The parsed tool being requested
    pub tool: Tool,
    /// Tool-specific context about the requested operation
    pub context: Option<super::tools::ToolContext>,
    /// Available permission options with tool-specific labels
    pub options: Vec<PermissionOption>,
    /// Granular trust scopes the user can choose from (e.g., specific paths, directories)
    #[serde(default)]
    pub trust_options: Vec<TrustOption>,
}

/// A permission option presented to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOption {
    /// The option identifier
    pub id: PermissionOptionId,
    /// Display label for this option (tool-specific)
    pub label: String,
    /// Hint for how the client should treat this option
    pub kind: PermissionOptionHint,
}

/// Permission option identifiers for tool approval.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PermissionOptionId {
    AllowOnce,
    AllowAlwaysTool,
    AllowAlwaysToolArgs,
    RejectOnce,
    RejectAlwaysTool,
    RejectAlwaysToolArgs,
    #[strum(default)]
    Custom(String),
}

impl PermissionOptionId {
    /// Returns true if this is an allow option.
    pub fn is_allow(&self) -> bool {
        matches!(
            self,
            Self::AllowOnce | Self::AllowAlwaysTool | Self::AllowAlwaysToolArgs
        )
    }

    /// Returns true if this is a reject option.
    pub fn is_reject(&self) -> bool {
        matches!(
            self,
            Self::RejectOnce | Self::RejectAlwaysTool | Self::RejectAlwaysToolArgs
        )
    }
}

/// Hint for how the client should treat a permission option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionHint {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendApprovalResultArgs {
    /// Id of the approval request
    pub id: String,
    /// Whether or not the request is approved
    pub result: ApprovalResult,
}

/// Result of a user's approval decision for a tool use request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResult {
    /// The permission option selected by the user
    pub option_id: PermissionOptionId,
    /// Optional reason for rejection
    pub reason: Option<String>,
    /// The specific trust scope selected (for AllowAlwaysToolArgs)
    #[serde(default)]
    pub trust_option: Option<TrustOption>,
}

/// A trust scope option that a tool can offer when permission is Ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustOption {
    /// Label shown in the selector
    pub label: String,
    /// User-friendly display
    pub display: String,
    /// The tool setting key to store the pattern in
    pub setting_key: String,
    /// The patterns to store for matching
    pub patterns: Vec<String>,
}

/// Result of evaluating tool permissions, indicating whether a tool should be allowed,
/// require user confirmation, or be denied with specific reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionEvalResult {
    /// Tool is allowed to execute without user confirmation
    Allow,
    /// Tool requires user confirmation before execution.
    /// Optionally carries granular trust options the user can choose from.
    Ask { trust_options: Vec<TrustOption> },
    /// Denial with specific reasons explaining why the tool was denied
    ///
    /// Tools are free to overload what these reasons are
    Deny { reason: String },
}

impl PermissionEvalResult {
    /// Create an Ask result with no trust options (default behavior).
    pub fn ask() -> Self {
        Self::Ask { trust_options: vec![] }
    }

    /// Create an Ask result with trust options.
    pub fn ask_with_options(trust_options: Vec<TrustOption>) -> Self {
        Self::Ask { trust_options }
    }
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
    pub agent_config: LoadedAgentConfig,
    /// Skip the same-name short-circuit check (used for registry refresh)
    pub force: bool,
    /// Updated knowledge provider for the new agent (if different from current)
    pub knowledge_provider: Option<Arc<dyn super::tools::KnowledgeProvider>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AgentResponse {
    Success,
    Snapshot(AgentSnapshot),
    McpPrompts(HashMap<String, Vec<Prompt>>),
    FilePrompts(HashMap<String, Vec<Prompt>>),
    Skills(HashMap<String, Vec<Prompt>>),
    SkillContent(Option<String>),
    McpPrompt(Vec<serde_json::Value>),
    TerminateAcknowledged,
    SwapComplete,
    McpServerInfo(Vec<super::tui_commands::McpServerInfo>),
    ToolInfo(Vec<super::tui_commands::ToolInfo>),
    Resources(Vec<String>),
    LastAssistantMessage(Option<String>),
    ToolTrustResult { changed: Vec<String>, invalid: Vec<String> },
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
    StateChange {
        from: Box<ExecutionState>,
        to: Box<ExecutionState>,
    },
    /// A tool use was requested by the model, and the permission was evaluated
    ToolPermissionEvalResult {
        tool_use_id: String,
        tool: Tool,
        result: PermissionEvalResult,
    },
    /// Events specific to tool and hook execution
    TaskExecutor(Box<TaskExecutorEvent>),
    /// Terminal outcome of a bounded stall-continuation retry sequence.
    StreamStallRetry {
        outcome: StallRetryOutcome,
        /// How many continuation retries had been issued when the sequence ended.
        attempt_number: u32,
        /// Whether any assistant output had streamed before a stall in this sequence
        /// (output the user saw and the retry threw away).
        partial_output: bool,
        /// Producer of the stall that (most recently) drove this sequence, so the
        /// whole stall metric family can be gated on the same source.
        source: crate::agent::agent_loop::types::StreamTimeoutSource,
    },
    /// A retried stream produced its first event after a hard stall cancel.
    StreamStallRecovery {
        recovery: std::time::Duration,
        /// Producer of the stall this recovery closes out.
        source: crate::agent::agent_loop::types::StreamTimeoutSource,
    },
    /// The idle watchdog cancelled a compaction stream. A dedicated event rather
    /// than a forwarded compaction `ResponseStreamEnd`: the raw stream end drives
    /// the message-level request series, which is reserved for requests that
    /// carry actual chat messages.
    CompactionStreamStalled {
        /// The hard idle threshold that elapsed — the observed idle gap.
        idle: std::time::Duration,
    },
    /// A hard-stall cancel abandoned the in-flight stream and a continuation
    /// request is being issued. Clients that rendered the abandoned stream's
    /// partial output must discard it: history replaces it with a timeout
    /// notice, so leaving it on screen shows text the model has no record of.
    StreamStallContinuation {
        /// Whether the abandoned stream had streamed visible output (text,
        /// thinking, or a tool-use start) before the stall.
        partial_output: bool,
    },
    /// An agent-layer retry of a transient backend failure (throttle/5xx/network)
    /// is being issued after a backoff wait.
    TransientRetry {
        /// Typed failure class; closed set so telemetry dimensions can never
        /// receive an out-of-schema label.
        class: crate::agent::error_recovery::TransientErrorClass,
        /// 1-based attempt number within the current turn.
        attempt_number: u32,
        /// How long the agent waited before this retry.
        backoff: std::time::Duration,
        /// Whether assistant output had already streamed (and been rendered) before
        /// the failure — the clean re-send regenerates the response from scratch.
        partial_output: bool,
    },
    /// A scheduled transient retry actually fired and its request is being
    /// re-sent. This — not [`Self::TransientRetry`], which fires at schedule
    /// time and drives the banner — is what retry-volume telemetry counts, so
    /// retries suppressed by a cancel during the backoff are not counted.
    TransientRetryExecuted {
        /// Typed failure class of the failure being retried.
        class: crate::agent::error_recovery::TransientErrorClass,
        /// 1-based attempt number within the current turn.
        attempt_number: u32,
        /// Whether assistant output had streamed before the failure.
        partial_output: bool,
    },
    /// A blocking subagent/crew stage was cancelled because its wall-clock deadline
    /// expired; the parent continues with whatever partial results completed.
    SubagentDeadlineExpired {
        /// The configured deadline that elapsed.
        deadline: std::time::Duration,
    },
}

/// Terminal outcome of a bounded stall-continuation retry sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StallRetryOutcome {
    /// A stream completed after one or more stall retries.
    Recovered,
    /// The retry budget was exhausted and the turn errored.
    Exhausted,
    /// The turn was torn down (cancel or unrelated terminal error) mid-sequence,
    /// before the retried stream proved recovery or the budget ran out.
    Cancelled,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());
            assert_eq!($variant, $ty::from_str($text).unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_permission_option_id_ser_deser() {
        test_ser_deser!(PermissionOptionId, PermissionOptionId::AllowOnce, "allow_once");
        test_ser_deser!(
            PermissionOptionId,
            PermissionOptionId::AllowAlwaysTool,
            "allow_always_tool"
        );
        test_ser_deser!(
            PermissionOptionId,
            PermissionOptionId::AllowAlwaysToolArgs,
            "allow_always_tool_args"
        );
        test_ser_deser!(PermissionOptionId, PermissionOptionId::RejectOnce, "reject_once");
        test_ser_deser!(
            PermissionOptionId,
            PermissionOptionId::RejectAlwaysTool,
            "reject_always_tool"
        );
        test_ser_deser!(
            PermissionOptionId,
            PermissionOptionId::RejectAlwaysToolArgs,
            "reject_always_tool_args"
        );

        // Custom variant - FromStr falls back to Custom for unknown strings
        assert_eq!(
            PermissionOptionId::from_str("my_custom_option").unwrap(),
            PermissionOptionId::Custom("my_custom_option".to_string())
        );
        assert_eq!(
            "my_custom_option",
            PermissionOptionId::Custom("my_custom_option".to_string()).to_string()
        );
    }

    #[test]
    fn test_permission_option_id_is_allow() {
        assert!(PermissionOptionId::AllowOnce.is_allow());
        assert!(PermissionOptionId::AllowAlwaysTool.is_allow());
        assert!(PermissionOptionId::AllowAlwaysToolArgs.is_allow());
        assert!(!PermissionOptionId::RejectOnce.is_allow());
        assert!(!PermissionOptionId::Custom("x".to_string()).is_allow());
    }

    #[test]
    fn test_permission_option_id_is_reject() {
        assert!(PermissionOptionId::RejectOnce.is_reject());
        assert!(PermissionOptionId::RejectAlwaysTool.is_reject());
        assert!(PermissionOptionId::RejectAlwaysToolArgs.is_reject());
        assert!(!PermissionOptionId::AllowOnce.is_reject());
        assert!(!PermissionOptionId::Custom("x".to_string()).is_reject());
    }

    #[test]
    fn test_permission_option_hint_serde() {
        for hint in [
            PermissionOptionHint::AllowOnce,
            PermissionOptionHint::AllowAlways,
            PermissionOptionHint::RejectOnce,
            PermissionOptionHint::RejectAlways,
        ] {
            let json = serde_json::to_string(&hint).unwrap();
            let parsed: PermissionOptionHint = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, hint);
        }
    }

    #[test]
    fn test_compaction_event_serde() {
        for ev in [
            CompactionEvent::Started,
            CompactionEvent::Completed,
            CompactionEvent::ContextRecoveryAttempt { final_attempt: false },
            CompactionEvent::ContextRecoveryAttempt { final_attempt: true },
            CompactionEvent::Failed { error: "x".to_string() },
        ] {
            let json = serde_json::to_string(&ev).unwrap();
            let _: CompactionEvent = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_tool_call_failure_reason_serde() {
        for r in [
            ToolCallFailureReason::ParseError,
            ToolCallFailureReason::PermissionDenied,
            ToolCallFailureReason::HookRejected,
        ] {
            let json = serde_json::to_string(&r).unwrap();
            let _: ToolCallFailureReason = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_clear_event() {
        let e = ClearEvent;
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.is_empty());
    }

    #[test]
    fn test_approval_result_equality() {
        let a = ApprovalResult {
            option_id: PermissionOptionId::AllowOnce,
            reason: None,
            trust_option: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_agent_event_from_summary() {
        use crate::agent::tools::summary::Summary;
        let summary = Summary {
            task_description: "task".to_string(),
            context_summary: None,
            task_result: "done".to_string(),
            result_type: None,
        };
        let _e: AgentEvent = (&summary).into();
    }

    #[test]
    fn test_send_prompt_args_text_with_text() {
        let args = SendPromptArgs {
            content: vec![
                ContentChunk::Text("hello".to_string()),
                ContentChunk::Text(" world".to_string()),
            ],
            should_continue_turn: None,
        };
        assert_eq!(args.text(), Some("hello world".to_string()));
    }

    #[test]
    fn test_send_prompt_args_text_no_text() {
        let args = SendPromptArgs {
            content: vec![],
            should_continue_turn: None,
        };
        assert_eq!(args.text(), None);
    }

    #[test]
    fn test_send_prompt_args_should_continue_turn() {
        let args = SendPromptArgs {
            content: vec![],
            should_continue_turn: Some(true),
        };
        assert!(args.should_continue_turn());
        let args2 = SendPromptArgs {
            content: vec![],
            should_continue_turn: Some(false),
        };
        assert!(!args2.should_continue_turn());
        let args3 = SendPromptArgs {
            content: vec![],
            should_continue_turn: None,
        };
        assert!(!args3.should_continue_turn());
    }

    #[test]
    fn test_send_prompt_args_from_string() {
        let args: SendPromptArgs = "hi".to_string().into();
        assert_eq!(args.text(), Some("hi".to_string()));
    }

    #[test]
    fn test_tool_call_result_cancelled() {
        let r = ToolCallResult::Cancelled;
        let block = r.to_tool_result_block("tu1");
        assert_eq!(block.tool_use_id, "tu1");
        assert!(matches!(block.status, ToolResultStatus::Error));
    }

    #[test]
    fn test_tool_call_result_error() {
        let r = ToolCallResult::Error(crate::tools::ToolExecutionError::Custom("oops".to_string()));
        let block = r.to_tool_result_block("tu2");
        assert_eq!(block.tool_use_id, "tu2");
        assert!(matches!(block.status, ToolResultStatus::Error));
    }

    #[test]
    fn test_tool_call_result_success_text() {
        use crate::agent::tools::{
            ToolExecutionOutput,
            ToolExecutionOutputItem,
        };
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text("hello".to_string())]);
        let r = ToolCallResult::Success(output);
        let block = r.to_tool_result_block("tu3");
        assert_eq!(block.tool_use_id, "tu3");
        assert!(matches!(block.status, ToolResultStatus::Success));
    }

    #[test]
    fn test_permission_eval_result_ask() {
        let r = PermissionEvalResult::ask();
        assert!(matches!(r, PermissionEvalResult::Ask { .. }));
    }

    #[test]
    fn test_permission_eval_result_ask_with_options() {
        let opts = vec![TrustOption {
            label: "Allow".to_string(),
            display: "Allow paths".to_string(),
            setting_key: "x".into(),
            patterns: vec!["a".into()],
        }];
        let r = PermissionEvalResult::ask_with_options(opts.clone());
        match r {
            PermissionEvalResult::Ask { trust_options } => {
                assert_eq!(trust_options.len(), 1);
            },
            _ => panic!("expected Ask"),
        }
    }

    #[test]
    fn test_agent_error_display() {
        assert_eq!(AgentError::NotIdle.to_string(), "Agent is not idle");
        assert_eq!(AgentError::Channel.to_string(), "The agent channel has closed");
        assert_eq!(AgentError::Custom("oops".to_string()).to_string(), "oops");
    }

    #[test]
    fn test_agent_error_from_string() {
        let e: AgentError = "test error".to_string().into();
        assert!(matches!(e, AgentError::Custom(_)));
    }

    #[test]
    fn test_agent_error_serde() {
        let e = AgentError::NotIdle;
        let json = serde_json::to_string(&e).unwrap();
        let parsed: AgentError = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, AgentError::NotIdle));
    }

    #[test]
    fn test_content_chunk_from_string() {
        let chunk: ContentChunk = "hi".to_string().into();
        assert!(matches!(chunk, ContentChunk::Text(_)));
    }

    #[test]
    fn test_content_chunk_serde_text() {
        let c = ContentChunk::Text("hello".to_string());
        let json = serde_json::to_string(&c).unwrap();
        let parsed: ContentChunk = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ContentChunk::Text(_)));
    }

    #[test]
    fn test_content_chunk_resource_link() {
        let c = ContentChunk::ResourceLink("file://x".to_string());
        let json = serde_json::to_string(&c).unwrap();
        let parsed: ContentChunk = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ContentChunk::ResourceLink(_)));
    }

    #[test]
    fn test_send_prompt_args_text_with_image_only() {
        use crate::agent::agent_loop::types::{
            ImageBlock,
            ImageFormat,
            ImageSource,
        };
        let args = SendPromptArgs {
            content: vec![ContentChunk::Image(ImageBlock {
                format: ImageFormat::Png,
                source: ImageSource::Bytes(vec![]),
            })],
            should_continue_turn: None,
        };
        assert_eq!(args.text(), None);
    }

    #[test]
    fn test_send_prompt_args_text_with_resource_link() {
        let args = SendPromptArgs {
            content: vec![ContentChunk::ResourceLink("file://x".to_string())],
            should_continue_turn: None,
        };
        assert_eq!(args.text(), None);
    }

    #[test]
    fn test_send_prompt_args_text_mixed() {
        let args = SendPromptArgs {
            content: vec![
                ContentChunk::Text("hello".to_string()),
                ContentChunk::ResourceLink("file://x".to_string()),
                ContentChunk::Text(" world".to_string()),
            ],
            should_continue_turn: None,
        };
        assert_eq!(args.text(), Some("hello world".to_string()));
    }
}
