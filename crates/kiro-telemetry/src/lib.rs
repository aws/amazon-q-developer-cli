mod cardinality;
mod client;
mod config;
mod consent;
mod legacy;
mod otel;
mod pricing;
mod record;
mod redaction;

pub use cardinality::{
    LimitError,
    validate_log_record,
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
};
pub use consent::{
    ConsentCheckKind,
    ConsentIntegrityResult,
    consent_file_integrity_records,
    consent_record_integrity_record,
};
pub use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
};
pub use legacy::{
    LegacyOtelTarget,
    legacy_log_record,
    legacy_metric_record,
    legacy_otel_target,
};
pub use otel::{
    OtelLogsSink,
    OtelMetricsSink,
    OtelPipelineKind,
    OtelProviders,
    init_noop_otel,
    init_otel,
};
pub use pricing::{
    PRICING_TABLE_VERSION,
    TokenUsage,
    estimate_cost_usd,
};
pub use record::{
    Attribute,
    MetricRecord,
    MetricValue,
    TelemetryLogRecord,
};
pub use redaction::{
    FieldClass,
    PiiRedactor,
    PiiType,
    RedactionFinding,
    RedactionOutcome,
    RedactionResult,
};

pub fn meter() -> opentelemetry::metrics::Meter {
    opentelemetry::global::meter("kiro-telemetry")
}
