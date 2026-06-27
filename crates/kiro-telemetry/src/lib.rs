mod cardinality;
mod client;
mod config;
mod consent;
mod legacy;
pub mod log;
pub mod metric;
mod otel;
mod pricing;
mod record;
mod redaction;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use cardinality::{
    LimitError,
    validate_log_record,
    validate_metric_record,
};
pub use client::{
    EmitOutcome,
    InMemorySink,
    TelemetryClient,
    TelemetryError,
    TelemetrySink,
};
pub use config::{
    OtelMode,
    TelemetryConfig,
};
pub use consent::{
    consent_file_integrity_records,
    consent_record_integrity_record,
};
pub use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
};
pub use metric::{
    ConsentCheckKind,
    ConsentIntegrityResult,
    EventClass,
    FieldClass,
    MetricBuildError,
    MetricBuilder,
    PiiType,
    RedactionResult,
};
pub use otel::{
    OtelLogsSink,
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
    TelemetryLogRecord,
};
pub use redaction::{
    PiiRedactor,
    RedactionFinding,
    RedactionOutcome,
};

pub fn meter() -> opentelemetry::metrics::Meter {
    opentelemetry::global::meter("kiro-telemetry")
}
