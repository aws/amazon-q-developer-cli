//! Internal helpers used by `metric.rs` to decide whether a legacy event
//! should also emit a counter (e.g. `tool_call_total`, `kiro_cli_user_turns`).
//!
//! The full public legacy translation API lives in the separate
//! `kiro-telemetry-legacy` crate; only the small slice needed by metric
//! builders inside this crate is kept here, behind `pub(crate)`.

use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
    legacy_mappings,
    registry,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyOtelTarget {
    pub(crate) metric_name: &'static str,
    pub(crate) metric_kind: MetricKind,
}

pub(crate) fn legacy_otel_target(event_type: LegacyEventType) -> Option<LegacyOtelTarget> {
    let metric_name = legacy_mappings().metric_for_event(event_type)?;
    let metric_kind = registry().metric(metric_name)?.kind;
    Some(LegacyOtelTarget {
        metric_name,
        metric_kind,
    })
}

pub(crate) fn emits_legacy_user_turn_counter(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_user_turns")
}

pub(crate) fn emits_legacy_chat_message_counter(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_chat_messages_total")
}

pub(crate) fn emits_legacy_tool_call_total(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_tool_call_total")
}

fn matches_counter_target(event_type: LegacyEventType, metric_name: &str) -> bool {
    legacy_otel_target(event_type)
        .is_some_and(|target| target.metric_kind == MetricKind::Counter && target.metric_name == metric_name)
}
