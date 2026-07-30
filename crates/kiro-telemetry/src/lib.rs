use std::sync::OnceLock;
use std::time::{
    Duration,
    Instant,
};

mod cardinality;
mod client;
mod config;
mod drop_store;
pub mod metric;
mod otel;
mod pricing;
mod record;
mod redaction;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use cardinality::{
    LimitError,
    validate_metric_record,
};
pub use client::{
    EmitOutcome,
    EventClass,
    InMemorySink,
    TelemetryClient,
    TelemetryError,
    TelemetrySink,
};
pub use config::{
    OtelMode,
    TelemetryConfig,
    resolve_otlp_endpoint,
};
pub use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
};
pub use metric::{
    FieldClass,
    MetricBuildError,
    MetricBuilder,
    PiiType,
    RedactionResult,
};
pub use otel::{
    OtelMetricsSink,
    OtelPipelineKind,
    OtelProviders,
    init_noop_otel,
    init_otel,
};
pub use pricing::TokenUsage;
pub use record::{
    Attribute,
    MetricRecord,
    MetricValue,
};
pub use redaction::{
    PiiRedactor,
    RedactionFinding,
    RedactionOutcome,
};

pub fn meter() -> opentelemetry::metrics::Meter {
    opentelemetry::global::meter("kiro-telemetry")
}

static PROCESS_STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn mark_process_started() {
    PROCESS_STARTED_AT.get_or_init(Instant::now);
}

pub fn process_start_elapsed() -> Duration {
    PROCESS_STARTED_AT.get_or_init(Instant::now).elapsed()
}
