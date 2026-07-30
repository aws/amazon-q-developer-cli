use kiro_telemetry::{
    MetricRecord,
    metric,
};
use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
    legacy_mappings,
    registry,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyOtelTarget {
    pub event_type: LegacyEventType,
    pub metric_name: &'static str,
    pub metric_kind: MetricKind,
}

pub(crate) fn legacy_otel_target(event_type: LegacyEventType) -> Option<LegacyOtelTarget> {
    let metric_name = legacy_mappings().metric_for_event(event_type)?;
    let metric_kind = registry().metric(metric_name)?.kind;
    Some(LegacyOtelTarget {
        event_type,
        metric_name,
        metric_kind,
    })
}

pub fn legacy_metric_record(event_type: LegacyEventType) -> Option<MetricRecord> {
    let target = legacy_otel_target(event_type)?;
    match target.metric_kind {
        MetricKind::Counter => Some(metric::expect_valid(metric::counter(target.metric_name, 1))),
        MetricKind::ObservableGauge => Some(metric::expect_valid(metric::gauge(target.metric_name, 1.0))),
        MetricKind::Histogram | MetricKind::LogEvent | MetricKind::Derived => None,
    }
}

#[cfg(test)]
fn emits_legacy_user_turn_counter(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_user_turns")
}

#[cfg(test)]
fn emits_legacy_tool_call_total(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_tool_call_total")
}

#[cfg(test)]
fn matches_counter_target(event_type: LegacyEventType, metric_name: &str) -> bool {
    legacy_otel_target(event_type)
        .is_some_and(|target| target.metric_kind == MetricKind::Counter && target.metric_name == metric_name)
}

#[cfg(test)]
mod tests {
    use kiro_telemetry::MetricValue;

    use super::*;

    #[test]
    fn resolves_only_events_with_retained_schema_targets() {
        for event_type in LegacyEventType::ALL {
            if let Some(target) = legacy_otel_target(*event_type) {
                assert_eq!(target.event_type, *event_type);
                assert!(!target.metric_name.is_empty());
            }
        }

        assert!(legacy_otel_target(LegacyEventType::ChatAddedMessage).is_some());
        assert!(legacy_otel_target(LegacyEventType::ChatEnd).is_none());
    }

    #[test]
    fn exposes_metric_kind_for_dual_write_routing() {
        let counter = legacy_otel_target(LegacyEventType::ToolUseSuggested).expect("tool use target");
        assert_eq!(counter.metric_name, "kiro_cli_tool_call_total");
        assert_eq!(counter.metric_kind, MetricKind::Counter);

        assert!(legacy_otel_target(LegacyEventType::RecordUserTurnCompletion).is_none());
    }

    #[test]
    fn creates_metric_records_for_metric_shaped_targets() {
        let counter = legacy_metric_record(LegacyEventType::ToolUseSuggested).expect("counter target");
        assert_eq!(counter.name, "kiro_cli_tool_call_total");
        assert_eq!(counter.value, MetricValue::Counter(1));
        assert!(emits_legacy_tool_call_total(LegacyEventType::ToolUseSuggested));
        assert!(!emits_legacy_tool_call_total(LegacyEventType::ChatAddedMessage));
        assert!(emits_legacy_user_turn_counter(LegacyEventType::ChatAddedMessage));
        assert!(!emits_legacy_user_turn_counter(LegacyEventType::ToolUseSuggested));

        assert!(legacy_metric_record(LegacyEventType::ModeChanged).is_none());
    }
}
