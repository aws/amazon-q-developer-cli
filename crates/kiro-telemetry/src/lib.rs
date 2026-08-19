use std::sync::OnceLock;
use std::time::{
    Duration,
    Instant,
};

mod cardinality;
mod client;
mod config;
mod drop_store;
pub mod identity_epochs;
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
pub use identity_epochs::{
    IdentifyError,
    IdentifyOutcome,
    IdentityEpochs,
    is_valid_raw_user_id,
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
    OneShotMetricExportError,
    OtelMetricsSink,
    OtelPipelineKind,
    OtelProviders,
    export_metric_records_once,
    init_noop_otel,
    init_otel,
};
pub use pricing::TokenUsage;
pub use record::{
    Attribute,
    MetricLogProperties,
    MetricRecord,
    MetricValue,
};
pub use redaction::{
    PiiRedactor,
    RedactionFinding,
    RedactionOutcome,
};

const TELEMETRY_USER_ID_DOMAIN: &[u8] = b"kiro-tui-telemetry-user-id:v1\0";

/// Return the stable, non-reversible identity used by every telemetry emitter.
/// Raw service user IDs remain internal and are never placed on telemetry wire formats.
pub fn pseudonymous_user_id(user_id: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;

    let mut digest = sha2::Sha256::new();
    digest.update(TELEMETRY_USER_ID_DOMAIN);
    digest.update(user_id.as_bytes());
    format!(
        "v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.finalize())
    )
}

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

#[cfg(test)]
mod tests {
    use super::pseudonymous_user_id;

    #[test]
    fn telemetry_user_id_value_space_is_versioned_and_excludes_raw_ids() {
        let raw_user_id = "service-user-123";
        let pseudonym = pseudonymous_user_id(raw_user_id);

        assert!(pseudonym.starts_with("v1:"));
        assert_ne!(pseudonym, raw_user_id);
        assert!(!pseudonym.contains(raw_user_id));
        assert_eq!(pseudonym, pseudonymous_user_id(raw_user_id));
        assert_ne!(pseudonym, pseudonymous_user_id("service-user-456"));
    }
}
