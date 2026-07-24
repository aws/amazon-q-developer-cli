//! Legacy CloudWatch/Toolkit telemetry translation for kiro-cli.
//!
//! This crate owns the schema-driven legacy mappings and shared dual-write
//! adapter. Portable emitters that do not need Toolkit compatibility can avoid
//! pulling in the legacy client graph.

pub mod definitions;
pub mod event_translation;
pub mod legacy;

pub use event_translation::{
    event_to_metric_datum,
    event_to_otel_log_record,
    event_to_otel_metric_records,
};
pub use legacy::{
    legacy_log_record,
    legacy_metric_record,
};
