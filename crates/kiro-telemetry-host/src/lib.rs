//! kiro-telemetry-host: shared host for kiro-cli telemetry.
//!
//! This crate owns the cross-harness telemetry pieces (Event types,
//! HostConfig, and eventually `TelemetryThread`) so they can be reused
//! from V2, V3, kiro-bot, and other harnesses. Translation of [`Event`]
//! into legacy CloudWatch/Toolkit datums or OTel records lives in the
//! consumer crates.

pub mod event;
pub mod install_method;
pub mod reason;

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
};
pub use install_method::{
    InstallMethod,
    get_install_method,
};
pub use reason::{
    ReasonCode,
    get_error_reason,
};
