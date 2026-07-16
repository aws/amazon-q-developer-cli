//! Telemetry observer actor for kiro-cli harnesses.
//!
//! This crate owns the cross-harness pieces that turn live agent activity
//! ([`agent::protocol::AgentEvent`]s, MCP server events, end-of-turn metadata)
//! into [`kiro_telemetry_host::Event`]s and forwards them to the host
//! [`kiro_telemetry_host::TelemetryThread`]:
//!
//! * [`TelemetryObserver`] — the actor that fans `AgentEvent` -> `Event`.
//! * [`TelemetryContext`] — static per-session metadata (model provider closure, ACP client info,
//!   subagent flag) applied to every emitted event.
//!
//! The host crate's [`kiro_telemetry_host::TelemetryThread`] provides the
//! metadata-aware `send_*` helpers (e.g. `send_chat_added_message`) as
//! inherent methods; callers no longer need to import an extension trait.
//!
//! The crate intentionally has no dependency on V2's `Database`,
//! `RtsState`, CLI subcommand enum, or AWS SDK clients — those are abstracted
//! through:
//!
//! * `Arc<dyn Fn() -> Option<String> + Send + Sync>` for the dynamic model id.
//! * [`kiro_telemetry_host::EventEnricher`] for per-event metadata enrichment.
//! * The local [`EventStore`] trait for test-only event capture.

pub mod context;
pub mod observer;

pub use context::{
    AcpClientInfo,
    AppType,
    ClientName,
    ClientVersion,
    TelemetryContext,
};
pub use observer::{
    EventStore,
    REASON_CONTEXT_WINDOW_OVERFLOW,
    REASON_EMPTY_RESPONSE,
    REASON_INTERRUPTED,
    REASON_INVALID_JSON,
    REASON_INVALID_MODEL_ID,
    REASON_MODEL_OVERLOADED,
    REASON_MONTHLY_LIMIT_REACHED,
    REASON_QUOTA_BREACH,
    REASON_SERVICE_FAILURE,
    REASON_STREAM_TIMEOUT,
    REASON_VALIDATION_ERROR,
    ReasonExtractor,
    SessionEvent,
    TelemetryObserver,
    TelemetryObserverHandle,
    extract_reason_from_kind,
};
