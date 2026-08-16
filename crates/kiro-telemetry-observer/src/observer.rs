//! Telemetry observer actor.
//!
//! [`TelemetryObserver::spawn`] starts two background tasks and returns a
//! [`TelemetryObserverHandle`]:
//!
//! 1. **Actor task** — receives [`AgentEvent`]s, maintains per-request and per-tool state, and
//!    produces telemetry [`Event`]s.
//! 2. **Forwarding task** — runs the optional [`EventEnricher`] closure on each event and sends it
//!    to [`TelemetryThread`] (skipped in test mode), and optionally captures it through a
//!    caller-provided [`EventStore`] for assertion.

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
    MetadataUsage,
    StreamError,
    StreamErrorKind,
    StreamTimeoutSource,
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
    ToolCallIdentity,
    ToolKind,
};
use futures::future::BoxFuture;
use kiro_telemetry::metric;
use kiro_telemetry_host::{
    ChatAddedMessageParams,
    ChatConversationType,
    Event,
    EventEnricher,
    EventType,
    MessageMetaTag,
    RecordUserTurnCompletionArgs,
    TelemetryResult,
    TelemetryThread,
};
use tokio::sync::mpsc;
use tracing::warn;

use crate::context::TelemetryContext;

/// Env var that, when set, suppresses real send paths so tests can capture
/// events without contacting the legacy/OTel sinks.
pub(crate) const KIRO_TEST_MODE: &str = "KIRO_TEST_MODE";

// ---------------------------------------------------------------------------
// Reason constants
// ---------------------------------------------------------------------------
//
// Emitted in the `reason` field of `messageResponseError` and
// `recordUserTurnCompletion` telemetry events. The QCLI-SuccessRateDown alarm
// computes system failure rate as:
//
//   system_failures = total_errors - user_failures
//   success_rate    = 100 - (system_failures / total) * 100
//
// User failures are EXCLUDED from the alarm (they don't count against us).
// System failures trigger the alarm when the rate drops below 99%.

/// Reason: user cancelled the request (ctrl+c).
pub const REASON_INTERRUPTED: &str = "Interrupted";
/// Reason: context window overflow — input too large for model.
pub const REASON_CONTEXT_WINDOW_OVERFLOW: &str = "ContextWindowOverflow";
/// Reason: request rate throttled.
pub const REASON_QUOTA_BREACH: &str = "QuotaBreachError";
/// Reason: the selected model is temporarily overloaded.
pub const REASON_MODEL_OVERLOADED: &str = "ModelOverloadedError";
/// Reason: the monthly usage limit was reached.
pub const REASON_MONTHLY_LIMIT_REACHED: &str = "MonthlyLimitReached";
/// Reason: backend service failure.
pub const REASON_SERVICE_FAILURE: &str = "ServiceFailure";
/// Reason: stream timed out waiting for next event.
pub const REASON_STREAM_TIMEOUT: &str = "StreamTimeout";
/// Reason: transient network failure while receiving the response stream.
pub const REASON_TRANSIENT_NETWORK_FAILURE: &str = "TransientNetworkFailure";
/// Reason: request validation error.
pub const REASON_VALIDATION_ERROR: &str = "ValidationError";
/// Reason: backend rejected the request because the model id is not allowed in the current
/// inference path (maps from `StreamErrorKind::InvalidModelId`).
pub const REASON_INVALID_MODEL_ID: &str = "InvalidModelId";
/// Reason: authentication was rejected (maps from `StreamErrorKind::AccessDenied`).
pub const REASON_ACCESS_DENIED: &str = "AccessDenied";
/// Reason: model produced invalid JSON for tool use.
pub const REASON_INVALID_JSON: &str = "InvalidJson";
/// Reason: model returned a clean stream with no content.
pub const REASON_EMPTY_RESPONSE: &str = "EmptyResponse";

/// Optional caller-provided extractor for typed errors carried by
/// [`StreamError::as_concrete_error`]. V2 supplies a closure that downcasts
/// to `ConverseStreamError` and returns its reason code; tests pass `None`
/// to fall back to the `StreamErrorKind`-only path.
///
/// Returns `Some((reason, description, status_code))` if this stream error
/// carries a richer concrete error, `None` to fall back to the kind-only
/// reason mapping.
pub type ReasonExtractor = Arc<dyn Fn(&StreamError) -> Option<(String, String, Option<u16>)> + Send + Sync>;

/// Boxed [`EventStore`] forwarded events get pushed through.
pub type DynEventStore = Arc<dyn EventStore>;

/// Test-only sink for events handed to the forwarding task. V2 implements
/// this on `TelemetryEventStore`; production passes `None`.
pub trait EventStore: Send + Sync + std::fmt::Debug + 'static {
    fn push(&self, event: Event) -> BoxFuture<'_, ()>;
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

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
    has_tool_use: bool,
    follow_up_count: i64,
    message_meta_tags: Vec<MessageMetaTag>,
    /// Stored from the last failed request for propagation to turn-level telemetry.
    last_error: Option<ErrorInfo>,
    /// Whether the final model request ended in a typed provider refusal.
    last_request_refused: bool,
    /// Number of HTTP-level attempts for the last request in the turn. Pairs with
    /// `last_error` semantics so a turn that ends with a failed request has both
    /// the error reason and the attempt count for that request.
    last_request_attempts: Option<u32>,
    /// Stream-stall observations during the turn: episodes opened by a soft
    /// warning plus hard cancels that fired with no prior warning. An episode
    /// that warns and then escalates to the hard cancel counts once.
    stream_stall_count: u32,
    /// A soft stall warning was emitted for the in-flight stream and no episode
    /// end has closed it yet. Lets the hard-cancel path recognize an escalation
    /// (one silent gap, already counted at the warning) instead of counting a
    /// second stall for the same gap.
    soft_stall_open: bool,
    /// Stall-continuation retries issued during the turn.
    stream_stall_retries: u32,
}

impl TurnState {
    fn record_usage(&mut self, usage: &MetadataUsage) {
        let input = usage.input_tokens.unwrap_or(0) as i64;
        let output = usage.output_tokens.unwrap_or(0) as i64;
        let cache_read = usage.cache_read_input_tokens.unwrap_or(0) as i64;
        let total = input + output + cache_read;

        self.total_tokens += total;
        self.uncached_input_tokens += input;
        self.output_tokens += output;
        self.cache_read_input_tokens += cache_read;
        self.cache_write_input_tokens += usage.cache_write_input_tokens.unwrap_or(0) as i64;
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
    context_recovery: Option<ContextRecoveryState>,
}

#[derive(Default)]
struct ContextRecoveryState {
    attempts: u32,
    final_attempt: bool,
}

// ---------------------------------------------------------------------------
// Public handles + spawn
// ---------------------------------------------------------------------------

/// Handle to a spawned [`TelemetryObserver`] actor.
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
/// Call [`spawn`](Self::spawn) to start the actor and get a [`TelemetryObserverHandle`].
pub struct TelemetryObserver {
    event_tx: mpsc::UnboundedSender<Event>,
    context: TelemetryContext,
    /// Per-session state, keyed by session ID. Supports concurrent sessions.
    sessions: HashMap<String, SessionState>,
    /// Optional caller-supplied extractor for typed stream errors.
    reason_extractor: Option<ReasonExtractor>,
}

impl TelemetryObserver {
    /// Create the observer and spawn the forwarding task.
    ///
    /// * `metadata_enricher` — runs against every emitted event before it's handed to the host
    ///   thread. Pass `None` to skip enrichment (tests / lite harnesses).
    /// * `event_store` — optional sink for tests to capture events for assertion.
    /// * `reason_extractor` — optional extractor for V2's `ConverseStreamError`. Pass `None` to
    ///   fall back to the `StreamErrorKind`-only reason mapping.
    pub fn spawn(
        context: TelemetryContext,
        telemetry_thread: TelemetryThread,
        metadata_enricher: Option<EventEnricher>,
        event_store: Option<DynEventStore>,
        reason_extractor: Option<ReasonExtractor>,
    ) -> TelemetryObserverHandle {
        // Forwarding task: Event -> TelemetryThread (+ optional capture)
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
        {
            tokio::spawn(async move {
                let is_test = std::env::var(KIRO_TEST_MODE).is_ok();
                while let Some(mut event) = event_rx.recv().await {
                    if let Some(enricher) = metadata_enricher.as_ref() {
                        enricher(&mut event).await;
                    }
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
            reason_extractor,
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
    pub(crate) fn new_for_test(tx: mpsc::UnboundedSender<Event>, context: TelemetryContext) -> Self {
        Self {
            event_tx: tx,
            context,
            sessions: HashMap::new(),
            reason_extractor: None,
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
                AgentLoopEventKind::StreamStallWarning { .. } => {
                    session.turn_state.stream_stall_count = session.turn_state.stream_stall_count.saturating_add(1);
                    session.turn_state.soft_stall_open = true;
                    self.emit(EventType::StreamStall {
                        model: self.context.model(),
                        tier: metric::StallTier::Soft,
                    });
                },
                AgentLoopEventKind::StreamStallResumed { idle } => {
                    session.turn_state.soft_stall_open = false;
                    self.emit(EventType::StreamStallEpisodeEnd {
                        model: self.context.model(),
                        idle_seconds: idle.as_secs_f64(),
                        episode_end: metric::StallEpisodeEnd::Resumed,
                    });
                },
                AgentLoopEventKind::StreamStallFailed { idle } => {
                    session.turn_state.soft_stall_open = false;
                    self.emit(EventType::StreamStallEpisodeEnd {
                        model: self.context.model(),
                        idle_seconds: idle.as_secs_f64(),
                        episode_end: metric::StallEpisodeEnd::Failed,
                    });
                },
                AgentLoopEventKind::StreamStallCancelled { idle } => {
                    session.turn_state.soft_stall_open = false;
                    self.emit(EventType::StreamStallEpisodeEnd {
                        model: self.context.model(),
                        idle_seconds: idle.as_secs_f64(),
                        episode_end: metric::StallEpisodeEnd::Cancelled,
                    });
                },
                _ => {},
            },
            // The whole stall metric family (stall/episode-end/retry/recovery) is
            // gated on the watchdog producer: SDK recv timeouts ride the same
            // continuation machinery, and letting them into some series but not
            // others would skew any cross-series join (e.g. recovered / stalls).
            AgentEvent::Internal(InternalEvent::StreamStallRetry {
                outcome,
                attempt_number,
                partial_output,
                source,
            }) => {
                if *source != StreamTimeoutSource::IdleWatchdog {
                    return;
                }
                session.turn_state.stream_stall_retries =
                    session.turn_state.stream_stall_retries.saturating_add(*attempt_number);
                self.emit(EventType::StreamStallRetry {
                    model: self.context.model(),
                    outcome: match outcome {
                        agent::protocol::StallRetryOutcome::Recovered => metric::RetryOutcome::Recovered,
                        agent::protocol::StallRetryOutcome::Exhausted => metric::RetryOutcome::Exhausted,
                        agent::protocol::StallRetryOutcome::Cancelled => metric::RetryOutcome::Cancelled,
                    },
                    attempt_number: *attempt_number,
                    partial_output: Some(*partial_output),
                });
            },
            AgentEvent::Internal(InternalEvent::StreamStallRecovery { recovery, source }) => {
                if *source != StreamTimeoutSource::IdleWatchdog {
                    return;
                }
                self.emit(EventType::StreamStallRecovery {
                    model: self.context.model(),
                    recovery_seconds: recovery.as_secs_f64(),
                });
            },
            // Always watchdog-produced (compaction streams have no SDK recv-timeout
            // continuation path), so no producer gate is needed here.
            AgentEvent::Internal(InternalEvent::CompactionStreamStalled { idle }) => {
                // Same escalation rule as the main-loop hard cancel: the compaction
                // stream's forwarded soft warning already counted this gap.
                let escalated = std::mem::replace(&mut session.turn_state.soft_stall_open, false);
                if !escalated {
                    session.turn_state.stream_stall_count = session.turn_state.stream_stall_count.saturating_add(1);
                    self.emit(EventType::StreamStall {
                        model: self.context.model(),
                        tier: metric::StallTier::Hard,
                    });
                }
                self.emit(EventType::StreamStallEpisodeEnd {
                    model: self.context.model(),
                    idle_seconds: idle.as_secs_f64(),
                    episode_end: metric::StallEpisodeEnd::HardCancelled,
                });
            },
            AgentEvent::Internal(InternalEvent::SubagentDeadlineExpired { deadline }) => {
                self.emit(EventType::SubagentDeadlineExpired {
                    deadline_seconds: deadline.as_secs_f64(),
                });
            },
            // Retry volume counts EXECUTED retries: the schedule-time
            // `TransientRetry` event drives the client banner, and a cancel during
            // the backoff suppresses the re-send, so counting at schedule time
            // would overstate requests to the model service.
            AgentEvent::Internal(InternalEvent::TransientRetryExecuted {
                class,
                attempt_number,
                partial_output,
            }) => {
                // Exhaustive map from the agent's typed class into the schema's
                // closed dimension, so a new agent variant is a compile error here
                // rather than an invalid label at emit time.
                let class = match class {
                    agent::error_recovery::TransientErrorClass::Throttle => metric::TransientErrorClass::Throttle,
                    agent::error_recovery::TransientErrorClass::ServerError => metric::TransientErrorClass::ServerError,
                    agent::error_recovery::TransientErrorClass::Network => metric::TransientErrorClass::Network,
                };
                self.emit(EventType::TransientRetry {
                    model: self.context.model(),
                    class,
                    attempt_number: *attempt_number,
                    partial_output: Some(*partial_output),
                });
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
                tool_identity,
                reason,
                ..
            }) => {
                if let Some(tracker) = session.tool_trackers.remove(tool_use_id) {
                    self.emit_tool_use_suggested(session_id, tool_use_id, tracker, None, false);
                } else {
                    self.emit_failed_tool_use_suggested(
                        session_id,
                        tool_use_id,
                        tool_name,
                        tool_identity.as_ref(),
                        reason,
                    );
                }
            },
            AgentEvent::EndTurn(metadata) => {
                self.handle_end_turn(session_id, metadata);
            },
            AgentEvent::Compaction(agent::protocol::CompactionEvent::Started) => {
                session.turn_state.add_message_meta_tag(MessageMetaTag::Compact);
            },
            AgentEvent::Compaction(agent::protocol::CompactionEvent::ContextRecoveryAttempt { final_attempt }) => {
                let recovery = session.context_recovery.get_or_insert_default();
                recovery.attempts = recovery.attempts.saturating_add(1);
                recovery.final_attempt = *final_attempt;
            },
            _ => {},
        }
    }

    fn handle_mcp_event(&self, session_id: &str, event: &agent::mcp::McpServerEvent) {
        match event {
            agent::mcp::McpServerEvent::Initialized {
                server_name,
                source,
                tool_token_count_estimate,
                ..
            } => {
                self.emit(EventType::McpServerInit {
                    conversation_id: session_id.to_string(),
                    server_name: server_name.clone(),
                    mcp_server_source: mcp_server_source(*source),
                    init_failure_reason: None,
                    number_of_tools: 0,
                    all_tool_names: None,
                    loaded_tool_names: None,
                    all_tools_count: 0,
                    mcp_tools_token_count_estimate: Some(*tool_token_count_estimate),
                });
            },
            agent::mcp::McpServerEvent::InitializeError {
                server_name,
                source,
                error,
            } => {
                self.emit(EventType::McpServerInit {
                    conversation_id: session_id.to_string(),
                    server_name: server_name.clone(),
                    mcp_server_source: mcp_server_source(*source),
                    init_failure_reason: Some(error.clone()),
                    number_of_tools: 0,
                    all_tool_names: None,
                    loaded_tool_names: None,
                    all_tools_count: 0,
                    mcp_tools_token_count_estimate: None,
                });
            },
            agent::mcp::McpServerEvent::Initializing { .. }
            | agent::mcp::McpServerEvent::OauthRequest { .. }
            | agent::mcp::McpServerEvent::StatusRefresh { .. }
            | agent::mcp::McpServerEvent::ToolListChanged { .. } => {},
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

        let refusal = metadata.stream.as_ref().is_some_and(|stream| {
            stream.refusal.is_some() || stream.stop_reason.as_deref() == Some("CONTENT_FILTERED")
        });
        let (telemetry_result, reason, reason_desc, err_status_code) = if refusal {
            (
                TelemetryResult::Failed,
                Some("ModelRefusal".to_string()),
                Some("Model refused the request".to_string()),
                None,
            )
        } else {
            match result {
                Ok(_) => (TelemetryResult::Succeeded, None, None, None),
                Err(LoopError::Stream(stream_err)) => {
                    let (r, rd, sc) = self.extract_reason(stream_err);
                    (TelemetryResult::Failed, Some(r), Some(rd), sc)
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
            }
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

        // Hard stall: the watchdog abandoned the stream after the timeout's worth of
        // silence, so the timeout duration IS the observed idle gap. Gated on the
        // producer: the SDK transport's own ~59s receive timeout arrives as the same
        // error kind and would otherwise drown the watchdog series it shares.
        if let Err(LoopError::Stream(stream_err)) = result
            && let StreamErrorKind::StreamTimeout {
                duration,
                source: StreamTimeoutSource::IdleWatchdog,
            } = &stream_err.kind
        {
            // With both tiers armed, every hard cancel is an ESCALATION of the
            // soft warning that preceded it — the same silent gap, already
            // counted when it warned. Counting it again would leave the stall
            // series permanently ahead of the episode ends (one gap, two
            // stalls, one end). Only a hard cancel with no open soft episode
            // (soft tier disabled or misconfigured) opens — and counts — the
            // episode itself.
            let session = self.sessions.entry(session_id.to_string()).or_default();
            let escalated = std::mem::replace(&mut session.turn_state.soft_stall_open, false);
            if !escalated {
                session.turn_state.stream_stall_count = session.turn_state.stream_stall_count.saturating_add(1);
                self.emit(EventType::StreamStall {
                    model: self.context.model(),
                    tier: metric::StallTier::Hard,
                });
            }
            self.emit(EventType::StreamStallEpisodeEnd {
                model: self.context.model(),
                idle_seconds: duration.as_secs_f64(),
                episode_end: metric::StallEpisodeEnd::HardCancelled,
            });
        }

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
            let session = self.sessions.entry(session_id.to_string()).or_default();
            session.turn_state.last_error = Some(ErrorInfo {
                reason: reason.clone().unwrap_or_default(),
                reason_desc: reason_desc.clone().unwrap_or_default(),
                status_code: final_status_code,
            });
        }

        if let Some(total_attempts) = metadata.request_attempts.filter(|attempts| *attempts > 1) {
            self.emit(EventType::AutomaticRetryCompleted {
                retry_reason: transport_retry_reason(result, final_status_code),
                additional_attempts: total_attempts - 1,
                outcome: transport_retry_outcome(result),
            });
        }
        self.handle_context_recovery_response(session_id, result);

        // Accumulate into turn state
        let session = self.sessions.entry(session_id.to_string()).or_default();
        session.turn_state.last_request_refused = refusal;
        session.turn_state.message_ids.extend(message_id);
        session.turn_state.request_ids.push(request_id);
        session.turn_state.time_to_first_chunks_ms.push(time_to_first_chunk_ms);
        session.turn_state.assistant_response_length += response_len.unwrap_or(0) as i64;
        if let Some(usage) = usage {
            session.turn_state.record_usage(usage);
        }
        if let Some(attempts) = metadata.request_attempts {
            session.turn_state.last_request_attempts = Some(attempts);
        }
        if has_tool_use {
            session.turn_state.has_tool_use = true;
            session.turn_state.follow_up_count += 1;
        }
    }

    fn handle_context_recovery_response(
        &mut self,
        session_id: &str,
        result: &Result<agent::agent_loop::types::Message, LoopError>,
    ) {
        let session = self.sessions.entry(session_id.to_string()).or_default();
        let Some(recovery) = session.context_recovery.as_ref() else {
            return;
        };
        let Some(outcome) = context_recovery_outcome(result, recovery.final_attempt) else {
            return;
        };
        let attempts = session.context_recovery.take().unwrap().attempts;
        self.emit(EventType::AutomaticRetryCompleted {
            retry_reason: metric::RetryReason::ContextRecovery,
            additional_attempts: attempts,
            outcome,
        });
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
            LoopEndReason::UserTurnEnd if session.turn_state.last_request_refused => TelemetryResult::Failed,
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
                model_invocation_count: 0,
                user_turn_duration_seconds,
                follow_up_count: turn.follow_up_count,
                message_meta_tags: turn.message_meta_tags,
                is_subagent: self.context.is_subagent,
                emit_user_turn_counter: false,
                emit_turn_numeric_metrics: None,
                parent_tool_use_id: None,
                request_attempts: turn.last_request_attempts,
                stream_stall_count: (turn.stream_stall_count > 0).then_some(turn.stream_stall_count),
                stream_stall_retries: (turn.stream_stall_retries > 0).then_some(turn.stream_stall_retries),
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
        tool_identity: Option<&ToolCallIdentity>,
        reason: &ToolCallFailureReason,
    ) {
        // Exhaustive so a new variant cannot silently classify as a denial.
        // An unavailable-tool (dummy placeholder) call is a model error like a
        // parse failure: the model emitted a call nothing could execute.
        let is_model_error = match reason {
            ToolCallFailureReason::ParseError | ToolCallFailureReason::ToolUnavailable => true,
            ToolCallFailureReason::PermissionDenied | ToolCallFailureReason::HookRejected => false,
        };
        let (metric_tool_name, mcp_server_name, is_custom_tool) = match tool_identity {
            Some(identity) => (
                identity.tool_name.clone(),
                identity.mcp_server_name.clone(),
                identity.mcp_server_name.is_some(),
            ),
            None => (
                tool_name.to_string(),
                mcp_server_name_from_tool_name(tool_name),
                tool_name.starts_with('@'),
            ),
        };
        self.emit(EventType::ToolUseSuggested {
            conversation_id: session_id.to_string(),
            utterance_id: None,
            user_input_id: None,
            tool_use_id: Some(tool_use_id.to_string()),
            tool_name: Some(metric_tool_name),
            mcp_server_name,
            is_accepted: is_model_error,
            is_trusted: false,
            is_success: is_model_error.then_some(false),
            reason_desc: None,
            is_valid: Some(!is_model_error),
            is_custom_tool,
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

    /// Resolve a reason code, description, and status code for a [`StreamError`].
    ///
    /// Tries the caller-supplied [`ReasonExtractor`] (V2's `ConverseStreamError`
    /// path) first and falls back to the [`StreamErrorKind`]-only mapping.
    fn extract_reason(&self, stream_err: &StreamError) -> (String, String, Option<u16>) {
        if let Some(extractor) = self.reason_extractor.as_ref()
            && let Some((r, rd, sc)) = extractor(stream_err)
        {
            return (r, rd, sc.or(stream_err.original_status_code));
        }
        let (reason, desc) = extract_reason_from_kind(stream_err);
        (reason, desc, stream_err.original_status_code)
    }
}

fn mcp_server_source(source: agent::agent_config::McpServerConfigSource) -> metric::McpServerSource {
    match source {
        agent::agent_config::McpServerConfigSource::Registry => metric::McpServerSource::Registry,
        agent::agent_config::McpServerConfigSource::GlobalMcpJson => metric::McpServerSource::Global,
        agent::agent_config::McpServerConfigSource::WorkspaceMcpJson => metric::McpServerSource::Workspace,
        agent::agent_config::McpServerConfigSource::AgentConfig => metric::McpServerSource::Agent,
        agent::agent_config::McpServerConfigSource::AcpInjected => metric::McpServerSource::AcpInjected,
        agent::agent_config::McpServerConfigSource::Unknown => metric::McpServerSource::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

fn positive_i64(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

fn transport_retry_reason(
    result: &Result<agent::agent_loop::types::Message, LoopError>,
    status_code: Option<u16>,
) -> metric::RetryReason {
    match result {
        Err(LoopError::Stream(stream_err)) => match &stream_err.kind {
            StreamErrorKind::Throttling => metric::RetryReason::Throttled,
            StreamErrorKind::StreamTimeout { .. } => metric::RetryReason::Timeout,
            StreamErrorKind::TransientNetworkFailure { .. } => metric::RetryReason::Connection,
            StreamErrorKind::ServiceFailure => metric::RetryReason::ServerError,
            _ if status_code.is_some_and(|status| status >= 500) => metric::RetryReason::ServerError,
            _ => metric::RetryReason::Other,
        },
        _ => metric::RetryReason::Other,
    }
}

fn transport_retry_outcome(result: &Result<agent::agent_loop::types::Message, LoopError>) -> metric::RetryOutcome {
    match result {
        Ok(_) => metric::RetryOutcome::Recovered,
        Err(LoopError::Stream(stream_err)) if matches!(&stream_err.kind, StreamErrorKind::Interrupted) => {
            metric::RetryOutcome::Cancelled
        },
        Err(_) => metric::RetryOutcome::Exhausted,
    }
}

fn context_recovery_outcome(
    result: &Result<agent::agent_loop::types::Message, LoopError>,
    final_attempt: bool,
) -> Option<metric::RetryOutcome> {
    match result {
        Ok(_) => Some(metric::RetryOutcome::Recovered),
        Err(LoopError::Stream(stream_err)) if matches!(&stream_err.kind, StreamErrorKind::Interrupted) => {
            Some(metric::RetryOutcome::Cancelled)
        },
        Err(LoopError::Stream(stream_err))
            if matches!(&stream_err.kind, StreamErrorKind::ContextWindowOverflow) && !final_attempt =>
        {
            None
        },
        Err(_) => Some(metric::RetryOutcome::Exhausted),
    }
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

/// Map a [`StreamError`] to a (reason, description) pair using only the
/// [`StreamErrorKind`] variant. Public so callers (V2) can compose with their
/// own typed-error path.
pub fn extract_reason_from_kind(stream_err: &StreamError) -> (String, String) {
    let reason = match &stream_err.kind {
        StreamErrorKind::Throttling => REASON_QUOTA_BREACH,
        StreamErrorKind::ModelOverloaded { .. } => REASON_MODEL_OVERLOADED,
        StreamErrorKind::MonthlyLimitReached { .. } => REASON_MONTHLY_LIMIT_REACHED,
        StreamErrorKind::ContextWindowOverflow => REASON_CONTEXT_WINDOW_OVERFLOW,
        StreamErrorKind::Interrupted => REASON_INTERRUPTED,
        StreamErrorKind::ServiceFailure => REASON_SERVICE_FAILURE,
        StreamErrorKind::StreamTimeout { .. } => REASON_STREAM_TIMEOUT,
        StreamErrorKind::TransientNetworkFailure { .. } => REASON_TRANSIENT_NETWORK_FAILURE,
        StreamErrorKind::Validation { .. } => REASON_VALIDATION_ERROR,
        StreamErrorKind::AccessDenied => REASON_ACCESS_DENIED,
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
#[path = "tests.rs"]
mod tests;
