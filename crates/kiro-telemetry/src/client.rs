//! Thin client shell that fans out telemetry records to one or more sinks.
//!
//! The bounded-queue worker thread, WAL replay, and runtime cardinality budget
//! enforcement that previously lived here have been removed. Backpressure and
//! cardinality enforcement are handled by the OTel collector
//! (`memory_limiter`, `transform`/`filter` processors). On client side we only
//! validate the schema (closed-enum dimensions, attribute existence, kind)
//! before handing each record to its sinks.

use std::sync::{
    Arc,
    Mutex,
};

use crate::cardinality::{
    LimitError,
    validate_metric_record,
};
use crate::drop_store::ExportDropStore;
use crate::{
    MetricRecord,
    TelemetryConfig,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventClass {
    Metric,
    Audit,
    LegacyEvent,
}

impl EventClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Audit => "audit",
            Self::LegacyEvent => "legacy_event",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error(transparent)]
    Schema(#[from] LimitError),
    #[error("telemetry sink failed: {0}")]
    Sink(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmitOutcome {
    pub emitted: bool,
}

pub trait TelemetrySink: Send + Sync {
    fn exporter(&self) -> &'static str {
        "otel"
    }

    fn emit(&self, record: &MetricRecord) -> Result<(), TelemetryError>;
}

pub struct TelemetryClient {
    config: TelemetryConfig,
    sinks: Vec<Arc<dyn TelemetrySink>>,
}

impl std::fmt::Debug for TelemetryClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TelemetryClient")
            .field("config", &self.config)
            .field("sink_count", &self.sinks.len())
            .finish_non_exhaustive()
    }
}

impl TelemetryClient {
    pub fn new(config: TelemetryConfig) -> Self {
        Self {
            config,
            sinks: Vec::new(),
        }
    }

    pub fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    pub fn with_sink(mut self, sink: Arc<dyn TelemetrySink>) -> Self {
        self.sinks.push(sink);
        self
    }

    pub fn emit(&self, record: MetricRecord) -> Result<EmitOutcome, TelemetryError> {
        self.emit_with_class(record, EventClass::Metric)
    }

    pub fn emit_with_class(
        &self,
        mut record: MetricRecord,
        _event_class: EventClass,
    ) -> Result<EmitOutcome, TelemetryError> {
        if !self.config.exports_enabled() {
            return Ok(EmitOutcome { emitted: false });
        }

        if let Err(err) = validate_metric_record(&record) {
            ExportDropStore::new(self.config.state_dir.clone()).record(
                "metrics",
                crate::metric::ExportDropReason::InvalidRecord.as_str(),
                1,
            );
            return Err(err.into());
        }

        // Injected after schema validation: user_id is identity, not a metric
        // dimension — unbounded by nature, so it must never enter the
        // closed-enum registry or be settable by callers.
        if let Some(user_id) = &self.config.user_id {
            record = record.with_attribute("user_id", user_id.clone());
        }

        for sink in &self.sinks {
            if let Err(err) = sink.emit(&record) {
                ExportDropStore::new(self.config.state_dir.clone()).record(
                    "metrics",
                    crate::metric::ExportDropReason::EncodingFailure.as_str(),
                    1,
                );
                tracing::trace!(%err, "telemetry sink failed for metric");
            }
        }

        Ok(EmitOutcome { emitted: true })
    }
}

#[derive(Default)]
pub struct InMemorySink {
    records: Mutex<Vec<MetricRecord>>,
}

impl InMemorySink {
    pub fn records(&self) -> Vec<MetricRecord> {
        self.records.lock().expect("in-memory sink mutex poisoned").clone()
    }
}

impl TelemetrySink for InMemorySink {
    fn emit(&self, record: &MetricRecord) -> Result<(), TelemetryError> {
        self.records
            .lock()
            .expect("in-memory sink mutex poisoned")
            .push(record.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        OtelMode,
        metric,
    };

    #[test]
    fn opt_out_short_circuits_before_sink() {
        let sink = Arc::new(InMemorySink::default());
        let config = TelemetryConfig::new(false, OtelMode::DualWrite, None, std::env::temp_dir());
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let outcome = client
            .emit(metric::record_model_invocation(
                metric::Engine::V2,
                Some("claude-sonnet-4"),
            ))
            .expect("emit should not fail");

        assert!(!outcome.emitted);
        assert!(sink.records().is_empty());
    }

    #[test]
    fn enabled_client_emits_known_record() {
        let sink = Arc::new(InMemorySink::default());
        let config = TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir());
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let outcome = client
            .emit(metric::record_model_invocation(
                metric::Engine::V2,
                Some("claude-sonnet-4"),
            ))
            .expect("emit should not fail");

        assert!(outcome.emitted);
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn user_id_injected_on_every_metric_when_configured() {
        let sink = Arc::new(InMemorySink::default());
        let config = TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir())
            .with_user_id("test-user-id".to_string());
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        client
            .emit(metric::record_model_invocation(
                metric::Engine::V2,
                Some("claude-sonnet-4"),
            ))
            .expect("emit should not fail");

        let records = sink.records();
        assert_eq!(records.len(), 1);
        let user_id = records[0]
            .attributes
            .iter()
            .find(|attribute| attribute.key == "user_id")
            .map(|attribute| attribute.value.as_str());
        assert_eq!(user_id, Some("test-user-id"));
    }

    #[test]
    fn user_id_absent_when_not_configured() {
        let sink = Arc::new(InMemorySink::default());
        let config = TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir());
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        client
            .emit(metric::record_model_invocation(
                metric::Engine::V2,
                Some("claude-sonnet-4"),
            ))
            .expect("emit should not fail");

        let records = sink.records();
        assert_eq!(records.len(), 1);
        assert!(!records[0].attributes.iter().any(|attribute| attribute.key == "user_id"));
    }

    #[test]
    fn blank_user_id_is_ignored() {
        let config =
            TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir()).with_user_id("  ".to_string());
        assert_eq!(config.user_id, None);
    }

    #[test]
    fn unknown_metric_returns_schema_error() {
        let sink = Arc::new(InMemorySink::default());
        let state = tempfile::tempdir().unwrap();
        let config = TelemetryConfig::new(true, OtelMode::DualWrite, None, state.path().to_path_buf());
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let err = client
            .emit(MetricRecord::counter("not_registered_total", 1))
            .expect_err("unknown metric should fail");

        assert!(matches!(err, TelemetryError::Schema(LimitError::UnknownMetric(_))));
        assert!(sink.records().is_empty());
        let drops = ExportDropStore::new(state.path().to_path_buf()).snapshot();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].key.drop_reason, "invalid_record");
        assert_eq!(drops[0].count, 1);
    }
}
