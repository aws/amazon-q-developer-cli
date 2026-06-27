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
use crate::{
    MetricRecord,
    TelemetryConfig,
    TelemetryLogRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventClass {
    Metric,
    Log,
    Audit,
    LegacyEvent,
}

impl EventClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Log => "log",
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

    fn emit_log(&self, _record: &TelemetryLogRecord) -> Result<(), TelemetryError> {
        Ok(())
    }
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
        record: MetricRecord,
        _event_class: EventClass,
    ) -> Result<EmitOutcome, TelemetryError> {
        if !self.config.exports_enabled() {
            return Ok(EmitOutcome { emitted: false });
        }

        validate_metric_record(&record)?;

        for sink in &self.sinks {
            if let Err(err) = sink.emit(&record) {
                tracing::trace!(%err, "telemetry sink failed for metric");
            }
        }

        Ok(EmitOutcome { emitted: true })
    }

    pub fn emit_log(&self, record: TelemetryLogRecord) -> Result<EmitOutcome, TelemetryError> {
        if !self.config.otlp_logs_enabled() {
            return Ok(EmitOutcome { emitted: false });
        }

        crate::cardinality::validate_log_record(&record)?;

        for sink in &self.sinks {
            if let Err(err) = sink.emit_log(&record) {
                tracing::trace!(%err, "telemetry sink failed for log");
            }
        }

        Ok(EmitOutcome { emitted: true })
    }
}

#[derive(Default)]
pub struct InMemorySink {
    records: Mutex<Vec<MetricRecord>>,
    log_records: Mutex<Vec<TelemetryLogRecord>>,
}

impl InMemorySink {
    pub fn records(&self) -> Vec<MetricRecord> {
        self.records.lock().expect("in-memory sink mutex poisoned").clone()
    }

    pub fn log_records(&self) -> Vec<TelemetryLogRecord> {
        self.log_records
            .lock()
            .expect("in-memory log sink mutex poisoned")
            .clone()
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

    fn emit_log(&self, record: &TelemetryLogRecord) -> Result<(), TelemetryError> {
        self.log_records
            .lock()
            .expect("in-memory log sink mutex poisoned")
            .push(record.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MetricRecord,
        OtelMode,
        log,
    };

    #[test]
    fn opt_out_short_circuits_before_sink() {
        let sink = Arc::new(InMemorySink::default());
        let config =
            TelemetryConfig::new(false, OtelMode::DualWrite, None, std::env::temp_dir()).with_otlp_logs_enabled(true);
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let outcome = client
            .emit(
                MetricRecord::counter("kiro_cli_model_invocations_total", 1).with_attribute("model", "claude-sonnet-4"),
            )
            .expect("emit should not fail");

        assert!(!outcome.emitted);
        assert!(sink.records().is_empty());
    }

    #[test]
    fn enabled_client_emits_known_record() {
        let sink = Arc::new(InMemorySink::default());
        let config =
            TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir()).with_otlp_logs_enabled(true);
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let outcome = client
            .emit(
                MetricRecord::counter("kiro_cli_model_invocations_total", 1).with_attribute("model", "claude-sonnet-4"),
            )
            .expect("emit should not fail");

        assert!(outcome.emitted);
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn unknown_metric_returns_schema_error() {
        let sink = Arc::new(InMemorySink::default());
        let config =
            TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir()).with_otlp_logs_enabled(true);
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let err = client
            .emit(MetricRecord::counter("not_registered_total", 1))
            .expect_err("unknown metric should fail");

        assert!(matches!(err, TelemetryError::Schema(LimitError::UnknownMetric(_))));
        assert!(sink.records().is_empty());
    }

    #[test]
    fn otlp_logs_disabled_short_circuits_before_sink() {
        let sink = Arc::new(InMemorySink::default());
        let config =
            TelemetryConfig::new(true, OtelMode::DualWrite, None, std::env::temp_dir()).with_otlp_logs_enabled(false);
        let client = TelemetryClient::new(config).with_sink(sink.clone());

        let outcome = client
            .emit_log(log::conversation_completed(
                "session-1",
                "conversation-1",
                log::CompletionReason::Stop,
            ))
            .expect("log emit should not fail");

        assert!(!outcome.emitted);
        assert!(sink.log_records().is_empty());
    }
}
