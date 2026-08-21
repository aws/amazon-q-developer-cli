//! V2 telemetry shim. Re-exports host + observer + legacy types and exposes
//! V2-specific helpers from [`host_config`]. Real implementation lives in
//! the submodules and the kiro-telemetry-{host,observer,legacy} crates.

mod acp_method;
pub mod cli_helpers;
pub mod cognito;
pub mod core;
pub mod endpoint;
pub mod host_config;
pub mod legacy_sink;
pub mod metadata_provider;

pub(crate) use acp_method::{
    AcpConnectionContext,
    AcpMethodTelemetry,
    RequestObservation,
};
#[cfg(feature = "voice")]
#[allow(unused_imports)]
pub use host_config::send_voice_input;
#[allow(unused_imports)]
pub use host_config::{
    BuildHostConfigError,
    V2OtelTranslator,
    build_v2_host_config,
    send_daily_heartbeat,
};
// Re-exported for parity with the V1 API.
#[allow(unused_imports)]
pub use kiro_telemetry_host::get_error_reason;
#[allow(unused_imports)]
pub use kiro_telemetry_host::{
    EventEnricher,
    HostConfig,
    HostRole,
    InstallMethod,
    ReasonCode,
    TelemetryError,
    TelemetryThread,
    ToolUseEventBuilder,
    get_accurate_install_method,
    get_install_method,
    govcloud_partition,
};
#[allow(unused_imports)]
pub use kiro_telemetry_observer::{
    AcpClientInfo,
    AppType,
    ClientName,
    ClientVersion,
    ReasonExtractor,
    SessionEvent,
    TelemetryContext,
    TelemetryObserver,
    TelemetryObserverHandle,
    extract_reason_from_kind,
};
#[allow(unused_imports)]
pub use metadata_provider::{
    build_metadata_enricher,
    build_reason_extractor,
    set_event_metadata,
};

#[allow(unused_imports)]
pub use crate::telemetry::core::{
    EmptyResponseRetryOutcome,
    EventType,
    QProfileSwitchIntent,
    TelemetryResult,
};
