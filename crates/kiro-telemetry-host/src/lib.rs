//! kiro-telemetry-host: shared host for kiro-cli telemetry.
//!
//! This crate owns the cross-harness telemetry pieces (Event types,
//! HostConfig, and `TelemetryThread`) so they can be reused from V2, V3,
//! kiro-bot, and other harnesses. Translation of [`Event`] into legacy
//! CloudWatch/Toolkit datums or OTel records lives in the consumer crates.

pub mod config;
pub mod event;
pub mod install_method;
pub mod reason;
pub mod thread;
pub mod tool_event;

pub use config::{
    EventEnricher,
    HostConfig,
    HostRole,
    LegacySink,
    OtelEventTranslator,
    US_GOV_PARTITION,
    govcloud_partition,
};
pub use event::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    ChatConversationType,
    EmptyResponseRetryOutcome,
    Event,
    EventType,
    MessageMetaTag,
    ModeChangeSource,
    QProfileSwitchIntent,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    TelemetryResult,
    UiModeSource,
};
pub use install_method::{
    InstallMethod,
    get_accurate_install_method,
    get_install_method,
};
pub use reason::{
    ReasonCode,
    get_error_reason,
};
pub use thread::{
    TelemetryError,
    TelemetrySender,
    TelemetryThread,
};
pub use tool_event::ToolUseEventBuilder;
