//! kiro-telemetry-legacy: legacy CloudWatch/Toolkit telemetry translation
//! for kiro-cli (V2 only).
//!
//! This crate owns the V2-specific translation of [`kiro_telemetry_host::Event`]
//! into Toolkit `MetricDatum` values and OTel metric records, plus the
//! schema-driven legacy mappings used to dual-write events. It is consumed
//! only by V2 (chat-cli-v2) and a single V1 chat-cli test; kiro-bot does not
//! depend on it, so the entire CloudWatch/Toolkit graph stays out of lite
//! builds.

pub mod definitions;
pub mod event_translation;
pub mod legacy;

pub use event_translation::{
    event_to_metric_datum,
    event_to_otel_metric_record,
    event_to_otel_metric_records,
};
pub use legacy::legacy_metric_record;
