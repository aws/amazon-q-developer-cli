//! Telemetry observer actor that processes [`AgentEvent`]s and emits telemetry events.
//!
//! [`TelemetryObserver::spawn()`] starts two background tasks and returns a
//! [`TelemetryObserverHandle`]:
//!
//! 1. **Actor task** — receives `AgentEvent`s, maintains per-request and per-tool state, and
//!    produces telemetry `Event`s.
//! 2. **Forwarding task** — sends `Event`s to [`TelemetryThread`] (skipped in test mode) and
//!    optionally captures them in a [`TelemetryEventStore`] for assertion.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent::agent_loop::protocol::{
    AgentLoopEventKind,
    LoopEndReason,
    LoopError,
    StreamMetadata,
    StreamResult,
    UserTurnMetadata,
};
use agent::agent_loop::types::{
    MetadataUsage,
    StreamError,
    StreamErrorKind,
    StreamEvent,
};
use agent::protocol::{
    AgentEvent,
    InitializeUpdateEvent,
    InternalEvent,
    PermissionEvalResult,
    ToolCallFailureReason,
    ToolCallResult,
    UpdateEvent,
};
use agent::task_executor::TaskExecutorEvent;
use agent::tools::{
    BuiltInTool,
    ToolKind,
};
use kiro_telemetry::metric;
use tokio::sync::mpsc;
use tracing::warn;

use super::ReasonCode;
use super::core::{
    ChatAddedMessageParams,
    ChatConversationType,
    Event,
    EventType,
    MessageMetaTag,
    RecordUserTurnCompletionArgs,
    estimated_cost_usd,
};
use crate::agent::ipc_server::TelemetryEventStore;
use crate::agent::rts::RtsState;
use crate::api_client::error::ConverseStreamError;
use crate::constants::KIRO_ACP_CLIENT_NAME;
use crate::telemetry::TelemetryResult;
use crate::util::consts::env_var::KIRO_TEST_MODE;

// ---------------------------------------------------------------------------
// Reason constants
// ---------------------------------------------------------------------------
//
// These reason codes are emitted in the `reason` field of `messageResponseError`
// and `recordUserTurnCompletion` telemetry events. The QCLI-SuccessRateDown alarm
// computes system failure rate as:
//
//   system_failures = total_errors - user_failures
//   success_rate    = 100 - (system_failures / total) * 100
//
// User failures are EXCLUDED from the alarm (they don't count against us).
// System failures trigger the alarm when the rate drops below 99%.
//
// ## User failure reasons (excluded from success rate alarm)
//
// Reference: https://code.amazon.com/packages/ToolkitTelemetryInfrastructure/blobs/4b3f4f4fb8e9a95807e434088df4c284be8f542b/--/src/monitoring/metrics/qcli-metrics.ts#L19
//
// | Reason                      | Source                          |
// |-----------------------------|---------------------------------|
// | `Interrupted`               | User cancelled (ctrl+c)         |
// | `ContextWindowOverflow`     | Context too large for model     |
// | `MonthlyLimitReached`       | Monthly usage quota exceeded    |
// | `QuotaBreachError`          | Request rate limit (throttling) |
// | `NonInteractiveToolApproval`| --no-interactive + tool needed  |
// | `dispatch failure`          | Network/env issue (no response) |
// | `AccessDeniedException`     | Auth/permission denied          |
// | `ThrottlingException`       | Service-level throttle          |
//
// ## System failure reasons (counted in success rate alarm)
//
// Everything else, including: `ServiceFailure`, `StreamTimeout`,
// `ValidationError`, `ModelOverloadedError`, `InvalidJson`,
// `InternalServerException`, `BedrockError`, etc.

/// Reason: user cancelled the request (ctrl+c).
pub const REASON_INTERRUPTED: &str = "Interrupted";
/// Reason: context window overflow — input too large for model.
pub const REASON_CONTEXT_WINDOW_OVERFLOW: &str = "ContextWindowOverflow";
/// Reason: request rate throttled (maps from `ConverseStreamErrorKind::Throttling`).
pub const REASON_QUOTA_BREACH: &str = "QuotaBreachError";
/// Reason: backend service failure.
pub const REASON_SERVICE_FAILURE: &str = "ServiceFailure";
/// Reason: stream timed out waiting for next event.
pub const REASON_STREAM_TIMEOUT: &str = "StreamTimeout";
/// Reason: request validation error.
pub const REASON_VALIDATION_ERROR: &str = "ValidationError";
/// Reason: backend rejected the request because the model id is not allowed in the current
/// inference path (maps from `StreamErrorKind::InvalidModelId`).
pub const REASON_INVALID_MODEL_ID: &str = "InvalidModelId";
/// Reason: model produced invalid JSON for tool use.
pub const REASON_INVALID_JSON: &str = "InvalidJson";
/// Reason: model returned a clean stream with no content.
pub const REASON_EMPTY_RESPONSE: &str = "EmptyResponse";

// ---------------------------------------------------------------------------
// AppType / AcpClientInfo
// ---------------------------------------------------------------------------

/// Application type for telemetry — distinguishes V1, V2 (built-in TUI), and ACP (external).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    V1,
    V2,
    Acp,
}

impl AppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1 => "V1",
            Self::V2 => "V2",
            Self::Acp => "ACP",
        }
    }
}

/// ACP client identity from `InitializeRequest.client_info`.
#[derive(Debug, Clone)]
pub struct AcpClientInfo {
    pub name: ClientName,
    pub version: ClientVersion,
}

/// Identifies the ACP client connecting to the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientName {
    /// The built-in Kiro TUI (`kiro-tui`).
    Kiro,
    /// An external ACP client.
    Other(String),
    /// No client info provided (e.g. pre-initialize).
    Unknown,
}

impl ClientName {
    pub fn parse(s: &str) -> Self {
        if s == KIRO_ACP_CLIENT_NAME {
            Self::Kiro
        } else {
            Self::Other(s.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Kiro => KIRO_ACP_CLIENT_NAME,
            Self::Other(s) => s,
            Self::Unknown => "Unknown",
        }
    }
}

/// ACP client version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientVersion {
    Known(String),
    Unknown,
}

impl ClientVersion {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(v) => v,
            Self::Unknown => "Unknown",
        }
    }
}

impl AcpClientInfo {
    pub fn new(name: String, version: String) -> Self {
        Self {
            name: ClientName::parse(&name),
            version: ClientVersion::Known(version),
        }
    }

    pub fn app_type(&self) -> AppType {
        match self.name {
            ClientName::Kiro => AppType::V2,
            ClientName::Other(_) | ClientName::Unknown => AppType::Acp,
        }
    }
}

/// Static context shared across all events in a session.
#[derive(Debug, Clone)]
pub struct TelemetryContext {
    /// Model is read dynamically from `RtsState` at emit time.
    pub rts_state: Arc<RtsState>,
    pub app_type: AppType,
    pub client_info: Option<AcpClientInfo>,
    pub is_subagent: bool,
}

impl TelemetryContext {
    pub fn new(rts_state: Arc<RtsState>, client_info: Option<AcpClientInfo>, is_subagent: bool) -> Self {
        let app_type = client_info.as_ref().map_or(AppType::Acp, |ci| ci.app_type());
        Self {
            rts_state,
            app_type,
            client_info,
            is_subagent,
        }
    }

    fn model(&self) -> Option<String> {
        self.rts_state.model_id()
    }

    fn client_application(&self) -> metric::ClientApplication {
        match self.app_type {
            AppType::V1 => metric::ClientApplication::ChatCli,
            AppType::V2 => metric::ClientApplication::ChatCliV2,
            AppType::Acp => metric::ClientApplication::AcpExternal,
        }
    }

    fn apply_to(&self, event: &mut Event) {
        event.app_type = Some(self.app_type.as_str().to_string());
        if event.client_application.is_none() {
            event.set_client_application_kind(self.client_application());
        }
        event.is_subagent = self.is_subagent;
        if let Some(ci) = &self.client_info {
            event.acp_client_name = Some(ci.name.as_str().to_string());
            event.acp_client_version = Some(ci.version.as_str().to_string());
        }
    }
}

/// Per-request data accumulated for `recordUserTurnCompletion`.
#[derive(Debug, Default)]
struct TurnState {
    message_ids: Vec<String>,
    request_ids: Vec<Option<String>>,
    time_to_first_chunks_ms: Vec<Option<f64>>,
    assistant_response_length: i64,
    total_tokens: i64,
    uncached_input_tokens: i64,
    output_tokens: i64,
    cache_read_input_tokens: i64,
    cache_write_input_tokens: i64,
    estimated_cost_usd: f64,
    has_tool_use: bool,
    follow_up_count: i64,
    message_meta_tags: Vec<MessageMetaTag>,
    /// Stored from the last failed request for propagation to turn-level telemetry.
    last_error: Option<ErrorInfo>,
    /// Number of HTTP-level attempts for the last request in the turn. Pairs with
    /// `last_error` semantics so a turn that ends with a failed request has both
    /// the error reason and the attempt count for that request.
    last_request_attempts: Option<u32>,
}

impl TurnState {
    fn record_usage(&mut self, model: &Option<String>, usage: &MetadataUsage) {
        let input = usage.input_tokens.unwrap_or(0) as i64;
        let output = usage.output_tokens.unwrap_or(0) as i64;
        let cache_read = usage.cache_read_input_tokens.unwrap_or(0) as i64;
        let total = input + output + cache_read;

        self.total_tokens += total;
        self.uncached_input_tokens += input;
        self.output_tokens += output;
        self.cache_read_input_tokens += cache_read;
        self.cache_write_input_tokens += usage.cache_write_input_tokens.unwrap_or(0) as i64;
        self.estimated_cost_usd += estimated_cost_usd(model, kiro_telemetry::TokenUsage {
            uncached_input_tokens: usage.input_tokens.unwrap_or(0) as u64,
            cache_read_input_tokens: usage.cache_read_input_tokens.unwrap_or(0) as u64,
            cache_write_input_tokens: usage.cache_write_input_tokens.unwrap_or(0) as u64,
            output_tokens: usage.output_tokens.unwrap_or(0) as u64,
        })
        .unwrap_or_default();
    }

    fn add_message_meta_tag(&mut self, tag: MessageMetaTag) {
        if !self.message_meta_tags.contains(&tag) {
            self.message_meta_tags.push(tag);
        }
    }
}

#[derive(Debug, Clone)]
struct ErrorInfo {
    reason: String,
    reason_desc: String,
    status_code: Option<u16>,
}

/// Tracks state for a single tool use across multiple events.
#[derive(Debug)]
struct ToolUseTracker {
    tool_name: String,
    mcp_server_name: Option<String>,
    aws_service_name: Option<String>,
    aws_operation_name: Option<String>,
    is_custom_tool: bool,
    is_trusted: Option<bool>,
    is_accepted: Option<bool>,
    suggested_at: Instant,
    execution_start: Option<Instant>,
    utterance_id: Option<String>,
}

/// Message sent to the [`TelemetryObserver`] actor.
pub struct SessionEvent {
    pub session_id: String,
    pub agent_event: AgentEvent,
}

/// Per-session state tracked by the observer.
#[derive(Default)]
struct SessionState {
    /// Accumulated data for the current user turn.
    turn_state: TurnState,
    /// In-flight tool use trackers, keyed by tool_use_id.
    tool_trackers: HashMap<String, ToolUseTracker>,
}

/// Handle to a spawned [`TelemetryObserver`] actor.
///
/// Created by [`TelemetryObserver::spawn()`]. Sends [`SessionEvent`]s to the
/// background actor which processes them into telemetry and forwards to
/// `TelemetryThread`. In test mode, events are also recorded for assertion.
#[derive(Clone)]
pub struct TelemetryObserverHandle {
    tx: mpsc::UnboundedSender<SessionEvent>,
    event_tx: mpsc::UnboundedSender<Event>,
    context: TelemetryContext,
}

impl TelemetryObserverHandle {
    pub fn send_event(&self, session_id: String, agent_event: AgentEvent) {
        let _ = self.tx.send(SessionEvent {
            session_id,
            agent_event,
        });
    }

    pub fn send_telemetry_event(&self, mut event: Event) {
        self.context.apply_to(&mut event);
        let _ = self.event_tx.send(event);
    }
}

/// Processes [`AgentEvent`]s and emits telemetry events.
///
/// Call [`spawn()`](Self::spawn) to start the actor and get a [`TelemetryObserverHandle`].
pub struct TelemetryObserver {
    event_tx: mpsc::UnboundedSender<Event>,
    context: TelemetryContext,
    /// Per-session state, keyed by session ID. Supports concurrent sessions.
    sessions: HashMap<String, SessionState>,
}

impl TelemetryObserver {
    /// Create the observer and spawn the forwarding task to `TelemetryThread`.
    ///
    /// In test mode, pass a [`TelemetryEventStore`] to record events for assertion.
    pub fn spawn(
        context: TelemetryContext,
        telemetry_thread: crate::telemetry::TelemetryThread,
        database: crate::database::Database,
        event_store: Option<TelemetryEventStore>,
    ) -> TelemetryObserverHandle {
        // Forwarding task: Event -> TelemetryThread (+ optional capture)
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
        {
            tokio::spawn(async move {
                let is_test = std::env::var(KIRO_TEST_MODE).is_ok();
                while let Some(mut event) = event_rx.recv().await {
                    crate::telemetry::set_event_metadata(&database, &mut event).await;
                    if let Some(ref store) = event_store {
                        store.push(event.clone()).await;
                    }
                    if !is_test {
                        let _ = telemetry_thread.send_event(event);
                    }
                }
            });
        }

        // Actor task: SessionEvent -> process -> Event
        let (agent_tx, mut agent_rx) = mpsc::unbounded_channel::<SessionEvent>();
        let handle_event_tx = event_tx.clone();
        let handle_context = context.clone();
        let mut observer = Self {
            event_tx,
            context,
            sessions: HashMap::new(),
        };
        tokio::spawn(async move {
            while let Some(msg) = agent_rx.recv().await {
                observer.handle_event(&msg.session_id, &msg.agent_event);
            }
        });

        TelemetryObserverHandle {
            tx: agent_tx,
            event_tx: handle_event_tx,
            context: handle_context,
        }
    }

    #[cfg(test)]
    fn new_for_test(tx: mpsc::UnboundedSender<Event>, context: TelemetryContext) -> Self {
        Self {
            event_tx: tx,
            context,
            sessions: HashMap::new(),
        }
    }

    /// Process an [`AgentEvent`] and emit telemetry.
    fn handle_event(&mut self, session_id: &str, event: &AgentEvent) {
        if let AgentEvent::InitializeUpdate(InitializeUpdateEvent::Mcp(mcp_event)) | AgentEvent::Mcp(mcp_event) = event
        {
            self.handle_mcp_event(session_id, mcp_event);
            return;
        }

        let session = self.sessions.entry(session_id.to_string()).or_default();

        match event {
            AgentEvent::Internal(InternalEvent::AgentLoop(loop_event)) => match &loop_event.kind {
                AgentLoopEventKind::ResponseStreamEnd { result, metadata } => {
                    self.handle_response_stream_end(session_id, result, metadata);
                },
                AgentLoopEventKind::Stream(StreamResult::Ok(StreamEvent::RetryWarning(warning))) => {
                    self.handle_retry_warning(warning);
                },
                _ => {},
            },
            AgentEvent::Update(UpdateEvent::ToolCall(tool_call)) => {
                let (mcp_server_name, aws_service_name, aws_operation_name) = match &tool_call.tool.kind {
                    ToolKind::Mcp(mcp) => (Some(mcp.server_name.clone()), None, None),
                    ToolKind::BuiltIn(BuiltInTool::UseAws(use_aws)) => (
                        None,
                        Some(use_aws.service_name.clone()),
                        Some(use_aws.operation_name.clone()),
                    ),
                    ToolKind::BuiltIn(_) => (None, None, None),
                };
                session.tool_trackers.insert(tool_call.id.clone(), ToolUseTracker {
                    tool_name: tool_call.tool.kind.canonical_tool_name().tool_name().to_string(),
                    mcp_server_name,
                    aws_service_name,
                    aws_operation_name,
                    is_custom_tool: matches!(tool_call.tool.kind, ToolKind::Mcp(_)),
                    is_trusted: None,
                    is_accepted: None,
                    suggested_at: Instant::now(),
                    execution_start: None,
                    utterance_id: None,
                });
            },
            AgentEvent::Internal(InternalEvent::ToolPermissionEvalResult {
                tool_use_id,
                tool: _,
                result,
            }) => {
                if let Some(tracker) = session.tool_trackers.get_mut(tool_use_id) {
                    let trusted = matches!(result, PermissionEvalResult::Allow);
                    tracker.is_trusted = Some(trusted);
                    if trusted {
                        tracker.is_accepted = Some(true);
                    }
                    if matches!(result, PermissionEvalResult::Deny { .. }) {
                        tracker.is_accepted = Some(false);
                    }
                } else {
                    warn!(tool_use_id, "permission eval for unknown tool use");
                }
            },
            AgentEvent::Internal(InternalEvent::TaskExecutor(te)) => {
                if let TaskExecutorEvent::ToolExecutionStart(start) = te.as_ref()
                    && let Some(tracker) = session.tool_trackers.get_mut(start.id.tool_use_id())
                {
                    tracker.is_accepted = Some(true);
                    tracker.execution_start = Some(Instant::now());
                }
            },
            AgentEvent::Update(UpdateEvent::ToolCallFinished { tool_call, result }) => {
                if let Some(tracker) = session.tool_trackers.remove(&tool_call.id) {
                    self.emit_tool_use_suggested(session_id, &tool_call.id, tracker, Some(result), true);
                }
            },
            AgentEvent::Update(UpdateEvent::ToolCallFailed {
                tool_use_id,
                tool_name,
                reason,
                ..
            }) => {
                if let Some(tracker) = session.tool_trackers.remove(tool_use_id) {
                    self.emit_tool_use_suggested(session_id, tool_use_id, tracker, None, false);
                } else {
                    self.emit_failed_tool_use_suggested(session_id, tool_use_id, tool_name, reason);
                }
            },
            AgentEvent::EndTurn(metadata) => {
                self.handle_end_turn(session_id, metadata);
            },
            AgentEvent::Compaction(agent::protocol::CompactionEvent::Started) => {
                session.turn_state.add_message_meta_tag(MessageMetaTag::Compact);
            },
            _ => {},
        }
    }

    fn handle_mcp_event(&self, session_id: &str, event: &agent::mcp::McpServerEvent) {
        match event {
            agent::mcp::McpServerEvent::Initialized { server_name, .. } => {
                self.emit(EventType::McpServerInit {
                    conversation_id: session_id.to_string(),
                    server_name: server_name.clone(),
                    init_failure_reason: None,
                    number_of_tools: 0,
                    all_tool_names: None,
                    loaded_tool_names: None,
                    all_tools_count: 0,
                });
            },
            agent::mcp::McpServerEvent::InitializeError { server_name, error } => {
                self.emit(EventType::McpServerInit {
                    conversation_id: session_id.to_string(),
                    server_name: server_name.clone(),
                    init_failure_reason: Some(error.clone()),
                    number_of_tools: 0,
                    all_tool_names: None,
                    loaded_tool_names: None,
                    all_tools_count: 0,
                });
            },
            agent::mcp::McpServerEvent::Initializing { .. }
            | agent::mcp::McpServerEvent::OauthRequest { .. }
            | agent::mcp::McpServerEvent::ToolListChanged { .. } => {},
        }
    }

    fn handle_retry_warning(&self, warning: &agent::agent_loop::types::RetryWarningEvent) {
        self.emit(EventType::RetryAttempt {
            upstream: metric::Upstream::Rts,
            retry_reason: metric::RetryReason::Other,
            attempt: warning.attempt,
        });
    }

    fn handle_response_stream_end(
        &mut self,
        session_id: &str,
        result: &Result<agent::agent_loop::types::Message, LoopError>,
        metadata: &StreamMetadata,
    ) {
        let service = metadata.stream.as_ref().and_then(|s| s.service.as_ref());
        let metrics = metadata.stream.as_ref().and_then(|s| s.metrics.as_ref());
        let request_id = service.and_then(|s| s.request_id.clone());
        let status_code = service.and_then(|s| s.status_code);
        let message_id = result.as_ref().ok().and_then(|m| m.id.clone());

        let time_to_first_chunk_ms = metrics
            .and_then(|m| m.time_to_first_chunk)
            .map(|d| d.as_secs_f64() * 1000.0);
        let request_duration_seconds = metrics
            .and_then(|m| (m.request_end_time - m.request_start_time).to_std().ok())
            .map(|d| d.as_secs_f64());
        let time_between_chunks_ms = metrics.and_then(|m| {
            m.time_between_chunks
                .as_ref()
                .map(|chunks| chunks.iter().map(|d| d.as_secs_f64() * 1000.0).collect::<Vec<_>>())
        });
        let response_len = metrics.map(|m| m.response_stream_len as i32);

        let usage = metadata.stream.as_ref().and_then(|s| s.usage.as_ref());

        if let Some(stream) = metadata.stream.as_ref() {
            let model = self.context.model();
            for metering_usage in &stream.metering_usage {
                self.emit(EventType::MeteringEvent {
                    request_id: request_id.clone(),
                    model: model.clone(),
                    usage: metering_usage.value,
                    unit: metering_usage.unit.clone(),
                    unit_plural: metering_usage.unit_plural.clone(),
                });
            }
        }

        let tool_use_ids: Vec<String> = metadata.tool_uses.iter().map(|t| t.tool_use_id.clone()).collect();
        let tool_names: Vec<String> = metadata.tool_uses.iter().map(|t| t.name.clone()).collect();
        let has_tool_use = !metadata.tool_uses.is_empty();
        let message_meta_tags = self
            .sessions
            .entry(session_id.to_string())
            .or_default()
            .turn_state
            .message_meta_tags
            .clone();

        let (telemetry_result, reason, reason_desc, err_status_code) = match result {
            Ok(_) => (TelemetryResult::Succeeded, None, None, None),
            Err(LoopError::Stream(stream_err)) => {
                let (r, rd) = extract_reason(stream_err);
                (
                    TelemetryResult::Failed,
                    Some(r),
                    Some(rd),
                    stream_err.original_status_code,
                )
            },
            Err(LoopError::InvalidJson { .. }) => (
                TelemetryResult::Failed,
                Some(REASON_INVALID_JSON.to_string()),
                Some("Model produced invalid JSON".to_string()),
                None,
            ),
            Err(LoopError::EmptyResponse) => (
                TelemetryResult::Failed,
                Some(REASON_EMPTY_RESPONSE.to_string()),
                Some("Model returned an empty response".to_string()),
                None,
            ),
        };

        let final_status_code = err_status_code.or(status_code);

        // Emit addChatMessage
        let data = ChatAddedMessageParams {
            message_id: message_id.clone(),
            request_id: request_id.clone(),
            context_file_length: None,
            reason: reason.clone(),
            reason_desc: reason_desc.clone(),
            status_code: final_status_code,
            model: self.context.model(),
            time_to_first_chunk_ms,
            request_duration_seconds,
            time_between_chunks_ms,
            chat_conversation_type: Some(if has_tool_use {
                ChatConversationType::ToolUse
            } else {
                ChatConversationType::NotToolUse
            }),
            tool_name: if tool_names.is_empty() {
                None
            } else {
                Some(tool_names.join(","))
            },
            tool_use_id: if tool_use_ids.is_empty() {
                None
            } else {
                Some(tool_use_ids.join(","))
            },
            assistant_response_length: response_len,
            message_meta_tags,
            total_tokens: usage.and_then(|u| {
                // total = uncached_input + cache_read + output
                // cache_write is a subset of uncached_input, not additive
                let input = u.input_tokens.unwrap_or(0);
                let output = u.output_tokens.unwrap_or(0);
                let cache_read = u.cache_read_input_tokens.unwrap_or(0);
                let total = input as i32 + output as i32 + cache_read as i32;
                if total > 0 { Some(total) } else { None }
            }),
            uncached_input_tokens: usage.and_then(|u| u.input_tokens.map(|v| v as i32)),
            output_tokens: usage.and_then(|u| u.output_tokens.map(|v| v as i32)),
            cache_read_input_tokens: usage.and_then(|u| u.cache_read_input_tokens.map(|v| v as i32)),
            cache_write_input_tokens: usage.and_then(|u| u.cache_write_input_tokens.map(|v| v as i32)),
        };
        self.emit(EventType::ChatAddedMessage {
            conversation_id: session_id.to_string(),
            result: telemetry_result,
            data,
        });

        // Emit messageResponseError on failure
        if telemetry_result == TelemetryResult::Failed {
            self.emit(EventType::MessageResponseError {
                conversation_id: session_id.to_string(),
                context_file_length: None,
                result: TelemetryResult::Failed,
                reason: reason.clone(),
                reason_desc: reason_desc.clone(),
                status_code: final_status_code,
                request_id: request_id.clone(),
                message_id: message_id.clone(),
                model: self.context.model(),
            });
            if metadata.request_attempts.is_some_and(|attempts| attempts > 1) {
                self.emit(EventType::RetryExhausted {
                    upstream: metric::Upstream::Rts,
                    final_error_kind: metric::ErrorKind::from_reason(reason.as_deref(), final_status_code),
                });
            }
            let session = self.sessions.entry(session_id.to_string()).or_default();
            session.turn_state.last_error = Some(ErrorInfo {
                reason: reason.unwrap_or_default(),
                reason_desc: reason_desc.unwrap_or_default(),
                status_code: final_status_code,
            });
        }

        // Accumulate into turn state
        let session = self.sessions.entry(session_id.to_string()).or_default();
        session.turn_state.message_ids.extend(message_id);
        session.turn_state.request_ids.push(request_id);
        session.turn_state.time_to_first_chunks_ms.push(time_to_first_chunk_ms);
        session.turn_state.assistant_response_length += response_len.unwrap_or(0) as i64;
        if let Some(usage) = usage {
            session.turn_state.record_usage(&self.context.model(), usage);
        }
        // Track attempt count for the latest request in the turn — pairs with `last_error`
        // so a turn that ends with a failed request has both the error reason and attempts.
        // `None` if the transport layer didn't report attempts (e.g. mock clients, validation
        // errors that short-circuit before dispatch).
        if let Some(attempts) = metadata.request_attempts {
            session.turn_state.last_request_attempts = Some(attempts);
        }
        if has_tool_use {
            session.turn_state.has_tool_use = true;
            session.turn_state.follow_up_count += 1;
        }
    }

    fn handle_end_turn(&mut self, session_id: &str, metadata: &UserTurnMetadata) {
        let session = self.sessions.entry(session_id.to_string()).or_default();

        // Flush any orphaned tool trackers (denied tools that never got ToolCallFinished)
        let orphaned: Vec<(String, ToolUseTracker)> = session.tool_trackers.drain().collect();
        for (id, tracker) in orphaned {
            self.emit_tool_use_suggested(session_id, &id, tracker, None, false);
        }

        let session = self.sessions.entry(session_id.to_string()).or_default();

        let result = match metadata.end_reason {
            LoopEndReason::UserTurnEnd | LoopEndReason::ToolUseRejected => TelemetryResult::Succeeded,
            LoopEndReason::Cancelled => TelemetryResult::Cancelled,
            LoopEndReason::Error | LoopEndReason::DidNotRun => TelemetryResult::Failed,
        };

        let (reason, reason_desc, status_code) = if result == TelemetryResult::Failed {
            session.turn_state.last_error.as_ref().map_or((None, None, None), |e| {
                (Some(e.reason.clone()), Some(e.reason_desc.clone()), e.status_code)
            })
        } else {
            (None, None, None)
        };

        let user_turn_duration_seconds = whole_turn_duration_seconds(metadata.turn_duration);

        let turn = std::mem::take(&mut session.turn_state);

        self.emit(EventType::RecordUserTurnCompletion {
            conversation_id: session_id.to_string(),
            result,
            args: RecordUserTurnCompletionArgs {
                message_ids: turn.message_ids,
                request_ids: turn.request_ids,
                model: self.context.model(),
                reason,
                reason_desc,
                status_code,
                time_to_first_chunks_ms: turn.time_to_first_chunks_ms,
                chat_conversation_type: Some(if turn.has_tool_use {
                    ChatConversationType::ToolUse
                } else {
                    ChatConversationType::NotToolUse
                }),
                user_prompt_length: metadata.user_prompt_length.min(i64::MAX as usize) as i64,
                assistant_response_length: turn.assistant_response_length,
                total_tokens: positive_i64(turn.total_tokens),
                uncached_input_tokens: positive_i64(turn.uncached_input_tokens),
                output_tokens: positive_i64(turn.output_tokens),
                cache_read_input_tokens: positive_i64(turn.cache_read_input_tokens),
                cache_write_input_tokens: positive_i64(turn.cache_write_input_tokens),
                estimated_cost_usd: positive_f64(turn.estimated_cost_usd),
                user_turn_duration_seconds,
                follow_up_count: turn.follow_up_count,
                message_meta_tags: turn.message_meta_tags,
                is_subagent: self.context.is_subagent,
                emit_user_turn_counter: false,
                parent_tool_use_id: None,
                request_attempts: turn.last_request_attempts,
            },
        });

        if let Some(percentage) = metadata.context_usage_percentage
            && percentage.is_finite()
            && percentage >= 0.0
        {
            self.emit(EventType::ContextUsagePercentage {
                model: self.context.model(),
                percentage: percentage as f64,
            });
        }
    }

    fn emit_tool_use_suggested(
        &self,
        session_id: &str,
        tool_use_id: &str,
        tracker: ToolUseTracker,
        result: Option<&ToolCallResult>,
        is_valid: bool,
    ) {
        let now = Instant::now();
        let is_accepted = tracker.is_accepted.unwrap_or(false);
        let is_trusted = tracker.is_trusted.unwrap_or(false);

        let (is_success, execution_duration, turn_duration) = match result {
            Some(ToolCallResult::Success(_)) => (
                Some(true),
                tracker.execution_start.map(|s| now.duration_since(s)),
                Some(now.duration_since(tracker.suggested_at)),
            ),
            Some(ToolCallResult::Error(_)) => (
                Some(false),
                tracker.execution_start.map(|s| now.duration_since(s)),
                Some(now.duration_since(tracker.suggested_at)),
            ),
            Some(ToolCallResult::Cancelled) | None => (None, None, None),
        };

        self.emit(EventType::ToolUseSuggested {
            conversation_id: session_id.to_string(),
            utterance_id: tracker.utterance_id,
            user_input_id: None,
            tool_use_id: Some(tool_use_id.to_string()),
            tool_name: Some(tracker.tool_name),
            mcp_server_name: tracker.mcp_server_name,
            is_accepted,
            is_trusted,
            is_success,
            reason_desc: None,
            is_valid: Some(is_valid),
            is_custom_tool: tracker.is_custom_tool,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: self.context.model(),
            execution_duration,
            turn_duration,
            aws_service_name: tracker.aws_service_name,
            aws_operation_name: tracker.aws_operation_name,
        });
    }

    fn emit_failed_tool_use_suggested(
        &self,
        session_id: &str,
        tool_use_id: &str,
        tool_name: &str,
        reason: &ToolCallFailureReason,
    ) {
        let is_parse_error = matches!(reason, ToolCallFailureReason::ParseError);
        self.emit(EventType::ToolUseSuggested {
            conversation_id: session_id.to_string(),
            utterance_id: None,
            user_input_id: None,
            tool_use_id: Some(tool_use_id.to_string()),
            tool_name: Some(tool_name.to_string()),
            mcp_server_name: mcp_server_name_from_tool_name(tool_name),
            is_accepted: is_parse_error,
            is_trusted: false,
            is_success: is_parse_error.then_some(false),
            reason_desc: None,
            is_valid: Some(!is_parse_error),
            is_custom_tool: tool_name.starts_with('@'),
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: self.context.model(),
            execution_duration: None,
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        });
    }

    fn emit(&self, ty: EventType) {
        let mut event = Event::new(ty);
        self.context.apply_to(&mut event);
        let _ = self.event_tx.send(event);
    }
}

fn positive_i64(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

fn positive_f64(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

fn mcp_server_name_from_tool_name(tool_name: &str) -> Option<String> {
    let (server_name, _) = tool_name.strip_prefix('@')?.split_once('/')?;
    (!server_name.is_empty()).then(|| server_name.to_string())
}

fn whole_turn_duration_seconds(duration: Option<std::time::Duration>) -> i64 {
    duration.map_or(0, |duration| {
        let seconds = duration.as_secs();
        if seconds == 0 && !duration.is_zero() {
            1
        } else {
            seconds.min(i64::MAX as u64) as i64
        }
    })
}

/// Extract reason code from a [`StreamError`], trying downcast first, then fallback.
fn extract_reason(stream_err: &StreamError) -> (String, String) {
    // For RTS, failing to send the initial request results in [`ConverseStreamError`].
    if let Some(cse) = stream_err.as_concrete_error::<ConverseStreamError>() {
        return (cse.reason_code(), stream_err.to_string());
    }

    // These match mid-stream errors
    let reason = match &stream_err.kind {
        StreamErrorKind::Throttling => REASON_QUOTA_BREACH,
        StreamErrorKind::ContextWindowOverflow => REASON_CONTEXT_WINDOW_OVERFLOW,
        StreamErrorKind::Interrupted => REASON_INTERRUPTED,
        StreamErrorKind::ServiceFailure => REASON_SERVICE_FAILURE,
        StreamErrorKind::StreamTimeout { .. } => REASON_STREAM_TIMEOUT,
        StreamErrorKind::Validation { .. } => REASON_VALIDATION_ERROR,
        StreamErrorKind::InvalidModelId { .. } => REASON_INVALID_MODEL_ID,
        StreamErrorKind::Other { reason_code, message } => reason_code.as_deref().unwrap_or_else(|| {
            if message.len() > 256 {
                agent::util::truncate_safe(message, 256)
            } else {
                message
            }
        }),
    };
    (reason.to_string(), stream_err.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use agent::agent_loop::protocol::{
        AgentLoopEvent,
        AgentLoopEventKind,
        LoopEndReason,
        LoopError,
        StreamMetadata,
        StreamResult,
        UserTurnMetadata,
    };
    use agent::agent_loop::types::{
        ContentBlock,
        Message,
        MetadataEvent,
        MetadataMetrics,
        MetadataService,
        MetadataUsage,
        MeteringUsageInfo,
        RetryWarningEvent,
        Role,
        StreamError,
        StreamErrorKind,
        StreamEvent,
    };
    use agent::types::AgentId;
    use uuid::Uuid;

    use super::*;
    use crate::telemetry::core::EventLegacyExt;

    fn test_loop_id() -> agent::agent_loop::AgentLoopId {
        agent::agent_loop::AgentLoopId::new(AgentId::default())
    }

    fn test_rts_state() -> Arc<RtsState> {
        let state = Arc::new(RtsState::new("conv-123".into()));
        state.set_model_info(Some(crate::cli::chat::legacy::model::ModelInfo {
            model_name: None,
            description: None,
            model_id: "claude-4-sonnet".to_string(),
            context_window_tokens: 200_000,
            rate_multiplier: None,
            rate_unit: None,
            additional_fields: None,
        }));
        state
    }

    fn make_observer_with_subagent(is_subagent: bool) -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client_info = Some(AcpClientInfo::new(KIRO_ACP_CLIENT_NAME.into(), "1.0.0".into()));
        let ctx = TelemetryContext::new(test_rts_state(), client_info, is_subagent);
        (TelemetryObserver::new_for_test(tx, ctx), rx)
    }

    fn make_observer() -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
        make_observer_with_subagent(false)
    }

    fn make_loop_event(kind: AgentLoopEventKind) -> AgentEvent {
        AgentEvent::Internal(InternalEvent::AgentLoop(Box::new(AgentLoopEvent {
            id: test_loop_id(),
            kind,
        })))
    }

    fn success_stream_end() -> AgentLoopEventKind {
        let request_start_time = chrono::Utc::now();
        AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text("hello".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: Some(MetadataEvent {
                    metrics: Some(MetadataMetrics {
                        request_start_time,
                        request_end_time: request_start_time + chrono::Duration::milliseconds(250),
                        time_to_first_chunk: Some(Duration::from_millis(100)),
                        time_between_chunks: Some(vec![Duration::from_millis(10)]),
                        response_stream_len: 42,
                    }),
                    usage: None,
                    service: Some(MetadataService {
                        request_id: Some("req-1".into()),
                        status_code: Some(200),
                    }),
                    metering_usage: Vec::new(),
                }),
                request_attempts: None,
            },
        }
    }

    fn error_stream_end(kind: StreamErrorKind) -> AgentLoopEventKind {
        AgentLoopEventKind::ResponseStreamEnd {
            result: Err(LoopError::Stream(StreamError::new(kind))),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: None,
                request_attempts: None,
            },
        }
    }

    /// Like `error_stream_end`, but with a specific transport-level attempt count.
    fn error_stream_end_with_attempts(kind: StreamErrorKind, attempts: u32) -> AgentLoopEventKind {
        AgentLoopEventKind::ResponseStreamEnd {
            result: Err(LoopError::Stream(StreamError::new(kind))),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: None,
                request_attempts: Some(attempts),
            },
        }
    }

    #[test]
    fn test_successful_request_emits_add_chat_message() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event("test-session", &make_loop_event(success_stream_end()));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ChatAddedMessage { result, data, .. } => {
                assert_eq!(*result, TelemetryResult::Succeeded);
                assert_eq!(data.request_id.as_deref(), Some("req-1"));
                assert_eq!(data.request_duration_seconds, Some(0.25));
                assert!(data.reason.is_none());
                assert_eq!(event.app_type.as_deref(), Some("V2"));
                assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
            },
            other => panic!("expected ChatAddedMessage, got {other:?}"),
        }
        // No error event
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn mcp_initialize_update_emits_mcp_server_init_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &AgentEvent::InitializeUpdate(InitializeUpdateEvent::Mcp(agent::mcp::McpServerEvent::Initialized {
                server_name: "code".to_string(),
                serve_duration: Duration::from_millis(25),
                list_tools_duration: Some(Duration::from_millis(10)),
                list_prompts_duration: None,
            })),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::McpServerInit {
                conversation_id,
                server_name,
                init_failure_reason,
                number_of_tools,
                all_tool_names,
                loaded_tool_names,
                all_tools_count,
            } => {
                assert_eq!(conversation_id, "test-session");
                assert_eq!(server_name, "code");
                assert!(init_failure_reason.is_none());
                assert_eq!(*number_of_tools, 0);
                assert!(all_tool_names.is_none());
                assert!(loaded_tool_names.is_none());
                assert_eq!(*all_tools_count, 0);
            },
            other => panic!("expected McpServerInit, got {other:?}"),
        }
    }

    #[test]
    fn mcp_runtime_error_emits_failed_mcp_server_init_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &AgentEvent::Mcp(agent::mcp::McpServerEvent::InitializeError {
                server_name: "local-server".to_string(),
                error: "request timed out while listing tools".to_string(),
            }),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::McpServerInit {
                conversation_id,
                server_name,
                init_failure_reason,
                ..
            } => {
                assert_eq!(conversation_id, "test-session");
                assert_eq!(server_name, "local-server");
                assert_eq!(
                    init_failure_reason.as_deref(),
                    Some("request timed out while listing tools")
                );
            },
            other => panic!("expected McpServerInit, got {other:?}"),
        }
    }

    #[test]
    fn retry_warning_emits_retry_attempt_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(AgentLoopEventKind::Stream(StreamResult::Ok(StreamEvent::RetryWarning(
                RetryWarningEvent {
                    attempt: 3,
                    max_attempts: 4,
                    delay_secs: 1.0,
                    message: "Retrying in 1s (attempt 3/4)".to_string(),
                },
            )))),
        );

        let event = rx.try_recv().unwrap();
        assert_eq!(event.app_type.as_deref(), Some("V2"));
        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
        match &event.ty {
            EventType::RetryAttempt {
                upstream,
                retry_reason,
                attempt,
            } => {
                assert_eq!(*upstream, metric::Upstream::Rts);
                assert_eq!(*retry_reason, metric::RetryReason::Other);
                assert_eq!(*attempt, 3);
            },
            other => panic!("expected RetryAttempt, got {other:?}"),
        }
    }

    #[test]
    fn test_compaction_turn_carries_compact_message_meta_tag() {
        let (mut obs, mut rx) = make_observer();

        obs.handle_event(
            "test-session",
            &AgentEvent::Compaction(agent::protocol::CompactionEvent::Started),
        );
        obs.handle_event("test-session", &make_loop_event(success_stream_end()));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ChatAddedMessage { data, .. } => {
                assert_eq!(data.message_meta_tags, vec![MessageMetaTag::Compact]);
            },
            other => panic!("expected ChatAddedMessage, got {other:?}"),
        }

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_secs(2)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { args, .. } => {
                assert_eq!(args.message_meta_tags, vec![MessageMetaTag::Compact]);
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_metering_stream_emits_metering_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
                result: Ok(Message::new(
                    Uuid::new_v4().to_string(),
                    Role::Assistant,
                    vec![ContentBlock::Text("hello".into())],
                    None,
                )),
                metadata: StreamMetadata {
                    tool_uses: vec![],
                    stream: Some(MetadataEvent {
                        metrics: None,
                        usage: None,
                        service: Some(MetadataService {
                            request_id: Some("req-1".into()),
                            status_code: Some(200),
                        }),
                        metering_usage: vec![MeteringUsageInfo {
                            value: 2.0,
                            unit: "credit".into(),
                            unit_plural: "credits".into(),
                        }],
                    }),
                    request_attempts: None,
                },
            }),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::MeteringEvent {
                request_id,
                usage,
                unit,
                unit_plural,
                ..
            } => {
                assert_eq!(request_id.as_deref(), Some("req-1"));
                assert_eq!(*usage, 2.0);
                assert_eq!(unit, "credit");
                assert_eq!(unit_plural, "credits");
            },
            other => panic!("expected MeteringEvent, got {other:?}"),
        }

        let event = rx.try_recv().unwrap();
        assert!(matches!(event.ty, EventType::ChatAddedMessage { .. }));
    }

    #[test]
    fn test_failed_request_emits_error_events() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end(StreamErrorKind::Throttling)),
        );

        // addChatMessage with Failed
        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ChatAddedMessage { result, data, .. } => {
                assert_eq!(*result, TelemetryResult::Failed);
                assert_eq!(data.reason.as_deref(), Some(REASON_QUOTA_BREACH));
            },
            other => panic!("expected ChatAddedMessage, got {other:?}"),
        }

        // messageResponseError
        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::MessageResponseError { reason, .. } => {
                assert_eq!(reason.as_deref(), Some(REASON_QUOTA_BREACH));
            },
            other => panic!("expected MessageResponseError, got {other:?}"),
        }
    }

    #[test]
    fn failed_retried_request_emits_retry_exhausted_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::Throttling, 3)),
        );

        assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
        assert!(matches!(
            rx.try_recv().unwrap().ty,
            EventType::MessageResponseError { .. }
        ));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RetryExhausted {
                upstream,
                final_error_kind,
            } => {
                assert_eq!(*upstream, metric::Upstream::Rts);
                assert_eq!(*final_error_kind, metric::ErrorKind::Throttling);
            },
            other => panic!("expected RetryExhausted, got {other:?}"),
        }
    }

    #[test]
    fn first_attempt_failure_does_not_emit_retry_exhausted_event() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::ServiceFailure, 1)),
        );

        assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
        assert!(matches!(
            rx.try_recv().unwrap().ty,
            EventType::MessageResponseError { .. }
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_context_overflow_reason_mapping() {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end(StreamErrorKind::ContextWindowOverflow)),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ChatAddedMessage { data, .. } => {
                assert_eq!(data.reason.as_deref(), Some(REASON_CONTEXT_WINDOW_OVERFLOW));
            },
            other => panic!("expected ChatAddedMessage, got {other:?}"),
        }
    }

    #[test]
    fn test_end_turn_emits_record_user_turn_completion() {
        let (mut obs, mut rx) = make_observer();

        // Simulate 2 requests then end turn
        obs.handle_event("test-session", &make_loop_event(success_stream_end()));
        let _ = rx.try_recv(); // consume addChatMessage

        obs.handle_event("test-session", &make_loop_event(success_stream_end()));
        let _ = rx.try_recv(); // consume addChatMessage

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 2,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_secs(5)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 11,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Succeeded);
                assert_eq!(args.request_ids.len(), 2);
                assert_eq!(args.user_turn_duration_seconds, 5);
                assert_eq!(args.user_prompt_length, 11);
                assert!(args.reason.is_none());
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_turn_completion_keeps_subsecond_duration() {
        let (mut obs, mut rx) = make_observer();

        obs.handle_event("test-session", &make_loop_event(success_stream_end()));
        let _ = rx.try_recv();

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_millis(250)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { args, .. } => {
                assert_eq!(args.user_turn_duration_seconds, 1);
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_end_turn_emits_context_usage_percentage() {
        let (mut obs, mut rx) = make_observer_with_subagent(true);

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_secs(1)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: Some(42.5),
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        assert!(matches!(event.ty, EventType::RecordUserTurnCompletion { .. }));

        let event = rx.try_recv().unwrap();
        assert!(event.is_subagent);
        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
        match &event.ty {
            EventType::ContextUsagePercentage { model, percentage } => {
                assert_eq!(model.as_deref(), Some("claude-4-sonnet"));
                assert_eq!(*percentage, 42.5);
            },
            other => panic!("expected ContextUsagePercentage, got {other:?}"),
        }
    }

    #[test]
    fn test_subagent_context_marks_chat_and_turn_telemetry() {
        let (mut obs, mut rx) = make_observer_with_subagent(true);

        obs.handle_event("subagent-session", &make_loop_event(success_stream_end()));

        let event = rx.try_recv().unwrap();
        assert!(event.is_subagent);
        assert!(matches!(event.ty, EventType::ChatAddedMessage { .. }));

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_secs(1)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("subagent-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        assert!(event.is_subagent);
        match &event.ty {
            EventType::RecordUserTurnCompletion { args, .. } => {
                assert!(args.is_subagent);
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_turn_completion_accumulates_token_counts() {
        let (mut obs, mut rx) = make_observer();

        obs.handle_event(
            "test-session",
            &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
                result: Ok(Message::new(
                    Uuid::new_v4().to_string(),
                    Role::Assistant,
                    vec![ContentBlock::Text("hello".into())],
                    None,
                )),
                metadata: StreamMetadata {
                    tool_uses: vec![],
                    stream: Some(MetadataEvent {
                        metrics: None,
                        usage: Some(MetadataUsage {
                            input_tokens: Some(10),
                            output_tokens: Some(5),
                            cache_read_input_tokens: Some(2),
                            cache_write_input_tokens: Some(3),
                            context_usage_percentage: None,
                        }),
                        service: None,
                        metering_usage: Vec::new(),
                    }),
                    request_attempts: None,
                },
            }),
        );
        let _ = rx.try_recv(); // addChatMessage

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: None,
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { args, .. } => {
                assert_eq!(args.total_tokens, Some(17));
                assert_eq!(args.uncached_input_tokens, Some(10));
                assert_eq!(args.output_tokens, Some(5));
                assert_eq!(args.cache_read_input_tokens, Some(2));
                assert_eq!(args.cache_write_input_tokens, Some(3));
                assert!(
                    args.estimated_cost_usd
                        .is_some_and(|cost| (cost - 0.00010785).abs() < 0.000000001)
                );
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_error_turn_propagates_reason() {
        let (mut obs, mut rx) = make_observer();

        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end(StreamErrorKind::Throttling)),
        );
        let _ = rx.try_recv(); // addChatMessage
        let _ = rx.try_recv(); // messageResponseError

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: None,
            end_reason: LoopEndReason::Error,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Failed);
                assert_eq!(args.reason.as_deref(), Some(REASON_QUOTA_BREACH));
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_turn_completion_carries_last_request_attempts() {
        let (mut obs, mut rx) = make_observer();

        // First request succeeded on attempt 1, second request failed after 3 attempts.
        // We expect the turn to report the last request's attempts (3), not the max
        // or an aggregation.
        obs.handle_event(
            "test-session",
            &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
                result: Ok(Message::new(
                    Uuid::new_v4().to_string(),
                    Role::Assistant,
                    vec![ContentBlock::Text("ok".into())],
                    None,
                )),
                metadata: StreamMetadata {
                    tool_uses: vec![],
                    stream: None,
                    request_attempts: Some(1),
                },
            }),
        );
        let _ = rx.try_recv();

        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::Throttling, 3)),
        );
        let _ = rx.try_recv(); // addChatMessage
        let _ = rx.try_recv(); // messageResponseError
        assert!(matches!(rx.try_recv().unwrap().ty, EventType::RetryExhausted {
            final_error_kind: metric::ErrorKind::Throttling,
            ..
        }));

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 2,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: None,
            end_reason: LoopEndReason::Error,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Failed);
                assert_eq!(args.request_attempts, Some(3));
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_turn_completion_attempts_is_none_when_transport_does_not_report() {
        // Mock/IPC clients don't run the interceptor, so request_attempts is None.
        // The turn completion should reflect that (None) rather than defaulting to some value.
        let (mut obs, mut rx) = make_observer();

        obs.handle_event("test-session", &make_loop_event(success_stream_end()));
        let _ = rx.try_recv();

        let metadata = UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: None,
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { args, .. } => {
                assert!(args.request_attempts.is_none());
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
    }

    #[test]
    fn test_acp_client_app_type() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client_info = Some(AcpClientInfo::new("external-client".into(), "2.0".into()));
        let ctx = TelemetryContext::new(test_rts_state(), client_info, false);
        let mut obs = TelemetryObserver::new_for_test(tx, ctx);
        obs.handle_event("test-session", &make_loop_event(success_stream_end()));

        let event = rx.try_recv().unwrap();
        assert_eq!(event.app_type.as_deref(), Some("ACP"));
        assert_eq!(event.acp_client_name.as_deref(), Some("external-client"));
        assert_eq!(event.client_application.as_deref(), Some("acp_external"));
    }

    #[test]
    fn test_use_aws_tool_call_emits_aws_origin_metadata() {
        let (mut obs, mut rx) = make_observer();
        let tool_call = agent::protocol::ToolCall {
            id: "tool-aws".to_string(),
            tool: agent::tools::Tool {
                tool_use_purpose: None,
                kind: ToolKind::BuiltIn(BuiltInTool::UseAws(agent::tools::use_aws::UseAws {
                    service_name: "s3".to_string(),
                    operation_name: "ListBuckets".to_string(),
                    positional_args: None,
                    parameters: None,
                    region: "us-east-1".to_string(),
                    profile_name: None,
                    label: None,
                })),
            },
            tool_use_block: agent::agent_loop::types::ToolUseBlock {
                tool_use_id: "tool-aws".to_string(),
                name: "use_aws".to_string(),
                input: serde_json::json!({}),
            },
        };

        obs.handle_event(
            "test-session",
            &AgentEvent::Update(UpdateEvent::ToolCall(tool_call.clone())),
        );
        obs.handle_event(
            "test-session",
            &AgentEvent::Update(UpdateEvent::ToolCallFinished {
                tool_call,
                result: ToolCallResult::Cancelled,
            }),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ToolUseSuggested {
                aws_service_name,
                aws_operation_name,
                ..
            } => {
                assert_eq!(aws_service_name.as_deref(), Some("s3"));
                assert_eq!(aws_operation_name.as_deref(), Some("ListBuckets"));
            },
            other => panic!("expected ToolUseSuggested, got {other:?}"),
        }

        let records = event.otel_metric_records();
        let tool_call = records
            .iter()
            .find(|record| record.name == "tool_call_total")
            .expect("tool_call_total");
        assert!(
            tool_call
                .attributes
                .iter()
                .any(|attr| { attr.key == "tool_origin" && attr.value == "aws_api" })
        );
    }

    #[test]
    fn test_failed_tool_call_without_tracker_emits_denied_mcp_telemetry() {
        let (mut obs, mut rx) = make_observer();

        obs.handle_event(
            "test-session",
            &AgentEvent::Update(UpdateEvent::ToolCallFailed {
                tool_use_id: "tool-mcp".to_string(),
                tool_name: "@local-server/custom_tool".to_string(),
                raw_input: serde_json::json!({}),
                reason: ToolCallFailureReason::PermissionDenied,
                error: "denied".to_string(),
            }),
        );

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::ToolUseSuggested {
                mcp_server_name,
                is_accepted,
                is_valid,
                is_custom_tool,
                ..
            } => {
                assert_eq!(mcp_server_name.as_deref(), Some("local-server"));
                assert!(!is_accepted);
                assert_eq!(*is_valid, Some(true));
                assert!(*is_custom_tool);
            },
            other => panic!("expected ToolUseSuggested, got {other:?}"),
        }

        let records = event.otel_metric_records();
        let invocations = records
            .iter()
            .find(|record| record.name == "kiro_cli_tool_invocations")
            .expect("kiro_cli_tool_invocations");
        assert!(
            invocations
                .attributes
                .iter()
                .any(|attr| { attr.key == "tool_origin" && attr.value == "mcp" })
        );
        assert!(
            invocations
                .attributes
                .iter()
                .any(|attr| { attr.key == "outcome" && attr.value == "denied" })
        );
    }

    // -----------------------------------------------------------------------
    // Reason constant & classification tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_reason_from_stream_error_kind() {
        let err = StreamError::new(StreamErrorKind::Throttling);
        assert_eq!(extract_reason(&err).0, REASON_QUOTA_BREACH);

        let err = StreamError::new(StreamErrorKind::ContextWindowOverflow);
        assert_eq!(extract_reason(&err).0, REASON_CONTEXT_WINDOW_OVERFLOW);

        let err = StreamError::new(StreamErrorKind::Interrupted);
        assert_eq!(extract_reason(&err).0, REASON_INTERRUPTED);
    }

    // -----------------------------------------------------------------------
    // AppType / AcpClientInfo tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_kiro_client_is_v2() {
        let info = AcpClientInfo::new(KIRO_ACP_CLIENT_NAME.into(), "1.0.0".into());
        assert_eq!(info.app_type(), AppType::V2);
        assert_eq!(info.name, ClientName::Kiro);
    }

    #[test]
    fn test_external_client_is_acp() {
        let info = AcpClientInfo::new("external-editor".into(), "2.0".into());
        assert_eq!(info.app_type(), AppType::Acp);
        assert_eq!(info.name, ClientName::Other("external-editor".into()));
    }
}
