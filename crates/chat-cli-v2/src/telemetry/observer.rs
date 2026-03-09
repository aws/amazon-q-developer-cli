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
    UserTurnMetadata,
};
use agent::agent_loop::types::{
    StreamError,
    StreamErrorKind,
};
use agent::protocol::{
    AgentEvent,
    InternalEvent,
    PermissionEvalResult,
    ToolCallResult,
    UpdateEvent,
};
use agent::task_executor::TaskExecutorEvent;
use agent::tools::ToolKind;
use tokio::sync::mpsc;
use tracing::warn;

use super::ReasonCode;
use super::core::{
    ChatAddedMessageParams,
    ChatConversationType,
    Event,
    EventType,
    RecordUserTurnCompletionArgs,
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
/// Reason: model produced invalid JSON for tool use.
pub const REASON_INVALID_JSON: &str = "InvalidJson";

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
}

impl TelemetryContext {
    pub fn new(rts_state: Arc<RtsState>, client_info: Option<AcpClientInfo>) -> Self {
        let app_type = client_info.as_ref().map_or(AppType::Acp, |ci| ci.app_type());
        Self {
            rts_state,
            app_type,
            client_info,
        }
    }

    fn model(&self) -> Option<String> {
        self.rts_state.model_id()
    }

    fn apply_to(&self, event: &mut Event) {
        event.app_type = Some(self.app_type.as_str().to_string());
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
    has_tool_use: bool,
    follow_up_count: i64,
    /// Stored from the last failed request for propagation to turn-level telemetry.
    last_error: Option<ErrorInfo>,
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
}

impl TelemetryObserverHandle {
    pub fn send_event(&self, session_id: String, agent_event: AgentEvent) {
        let _ = self.tx.send(SessionEvent {
            session_id,
            agent_event,
        });
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

        TelemetryObserverHandle { tx: agent_tx }
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
        let session = self.sessions.entry(session_id.to_string()).or_default();

        match event {
            AgentEvent::Internal(InternalEvent::AgentLoop(loop_event)) => {
                if let AgentLoopEventKind::ResponseStreamEnd { result, metadata } = &loop_event.kind {
                    self.handle_response_stream_end(session_id, result, metadata);
                }
            },
            AgentEvent::Update(UpdateEvent::ToolCall(tool_call)) => {
                session.tool_trackers.insert(tool_call.id.clone(), ToolUseTracker {
                    tool_name: tool_call.tool.kind.canonical_tool_name().tool_name().to_string(),
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
                    self.emit_tool_use_suggested(session_id, &tool_call.id, tracker, Some(result));
                }
            },
            AgentEvent::EndTurn(metadata) => {
                self.handle_end_turn(session_id, metadata);
            },
            _ => {},
        }
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
        let time_between_chunks_ms = metrics.and_then(|m| {
            m.time_between_chunks
                .as_ref()
                .map(|chunks| chunks.iter().map(|d| d.as_secs_f64() * 1000.0).collect::<Vec<_>>())
        });
        let response_len = metrics.map(|m| m.response_stream_len as i32);

        let tool_use_ids: Vec<String> = metadata.tool_uses.iter().map(|t| t.tool_use_id.clone()).collect();
        let tool_names: Vec<String> = metadata.tool_uses.iter().map(|t| t.name.clone()).collect();
        let has_tool_use = !metadata.tool_uses.is_empty();

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
            message_meta_tags: vec![], // TODO: populate meta tags (e.g. Compact) for V1 parity
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
            });
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
            self.emit_tool_use_suggested(session_id, &id, tracker, None);
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

        let user_turn_duration_seconds = metadata.turn_duration.map_or(0, |d| d.as_secs() as i64);

        let turn = std::mem::take(&mut session.turn_state);

        self.emit(EventType::RecordUserTurnCompletion {
            conversation_id: session_id.to_string(),
            result,
            args: RecordUserTurnCompletionArgs {
                message_ids: turn.message_ids,
                request_ids: turn.request_ids,
                reason,
                reason_desc,
                status_code,
                time_to_first_chunks_ms: turn.time_to_first_chunks_ms,
                chat_conversation_type: Some(if turn.has_tool_use {
                    ChatConversationType::ToolUse
                } else {
                    ChatConversationType::NotToolUse
                }),
                user_prompt_length: 0, // TODO: track actual prompt length
                assistant_response_length: turn.assistant_response_length,
                user_turn_duration_seconds,
                follow_up_count: turn.follow_up_count,
                message_meta_tags: vec![], // TODO: populate meta tags for V1 parity
                is_subagent: false,        // TODO: derive from session context
                parent_tool_use_id: None,
            },
        });
    }

    fn emit_tool_use_suggested(
        &self,
        session_id: &str,
        tool_use_id: &str,
        tracker: ToolUseTracker,
        result: Option<&ToolCallResult>,
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
            is_accepted,
            is_trusted,
            is_success,
            reason_desc: None,
            is_valid: Some(true),
            is_custom_tool: tracker.is_custom_tool,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: self.context.model(),
            execution_duration,
            turn_duration,
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
        StreamErrorKind::Other { reason_code, message } => reason_code
            .as_deref()
            .unwrap_or_else(|| if message.len() > 256 { &message[..256] } else { message }),
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
        UserTurnMetadata,
    };
    use agent::agent_loop::types::{
        ContentBlock,
        Message,
        MetadataEvent,
        MetadataMetrics,
        MetadataService,
        Role,
        StreamError,
        StreamErrorKind,
    };
    use agent::types::AgentId;

    use super::*;

    fn test_loop_id() -> agent::agent_loop::AgentLoopId {
        agent::agent_loop::AgentLoopId::new(AgentId::default())
    }

    fn test_rts_state() -> Arc<RtsState> {
        Arc::new(RtsState::new("conv-123".into()))
    }

    fn make_observer() -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client_info = Some(AcpClientInfo::new(KIRO_ACP_CLIENT_NAME.into(), "1.0.0".into()));
        let ctx = TelemetryContext::new(test_rts_state(), client_info);
        (TelemetryObserver::new_for_test(tx, ctx), rx)
    }

    fn make_loop_event(kind: AgentLoopEventKind) -> AgentEvent {
        AgentEvent::Internal(InternalEvent::AgentLoop(Box::new(AgentLoopEvent {
            id: test_loop_id(),
            kind,
        })))
    }

    fn success_stream_end() -> AgentLoopEventKind {
        AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Role::Assistant,
                vec![ContentBlock::Text("hello".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: Some(MetadataEvent {
                    metrics: Some(MetadataMetrics {
                        request_start_time: chrono::Utc::now(),
                        request_end_time: chrono::Utc::now(),
                        time_to_first_chunk: Some(Duration::from_millis(100)),
                        time_between_chunks: Some(vec![Duration::from_millis(10)]),
                        response_stream_len: 42,
                    }),
                    usage: None,
                    service: Some(MetadataService {
                        request_id: Some("req-1".into()),
                        status_code: Some(200),
                    }),
                }),
            },
        }
    }

    fn error_stream_end(kind: StreamErrorKind) -> AgentLoopEventKind {
        AgentLoopEventKind::ResponseStreamEnd {
            result: Err(LoopError::Stream(StreamError::new(kind))),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: None,
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
                assert!(data.reason.is_none());
                assert_eq!(event.app_type.as_deref(), Some("V2"));
            },
            other => panic!("expected ChatAddedMessage, got {other:?}"),
        }
        // No error event
        assert!(rx.try_recv().is_err());
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
        };
        obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Succeeded);
                assert_eq!(args.request_ids.len(), 2);
                assert_eq!(args.user_turn_duration_seconds, 5);
                assert!(args.reason.is_none());
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
    fn test_acp_client_app_type() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client_info = Some(AcpClientInfo::new("external-client".into(), "2.0".into()));
        let ctx = TelemetryContext::new(test_rts_state(), client_info);
        let mut obs = TelemetryObserver::new_for_test(tx, ctx);
        obs.handle_event("test-session", &make_loop_event(success_stream_end()));

        let event = rx.try_recv().unwrap();
        assert_eq!(event.app_type.as_deref(), Some("ACP"));
        assert_eq!(event.acp_client_name.as_deref(), Some("external-client"));
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
