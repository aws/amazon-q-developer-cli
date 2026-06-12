use kiro_telemetry_schema::{
    LegacyEventType,
    MetricKind,
    legacy_mappings,
    registry,
};

use crate::{
    MetricRecord,
    TelemetryLogRecord,
    metric,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyOtelTarget {
    pub event_type: LegacyEventType,
    pub metric_name: &'static str,
    pub metric_kind: MetricKind,
}

pub fn legacy_otel_target(event_type: LegacyEventType) -> Option<LegacyOtelTarget> {
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

pub fn emits_legacy_user_turn_counter(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "kiro_cli_user_turns")
}

pub fn emits_legacy_tool_call_total(event_type: LegacyEventType) -> bool {
    matches_counter_target(event_type, "tool_call_total")
}

fn matches_counter_target(event_type: LegacyEventType, metric_name: &str) -> bool {
    legacy_otel_target(event_type)
        .is_some_and(|target| target.metric_kind == MetricKind::Counter && target.metric_name == metric_name)
}

pub fn legacy_log_record(event_type: LegacyEventType) -> Option<TelemetryLogRecord> {
    let target = legacy_otel_target(event_type)?;
    (target.metric_kind == MetricKind::LogEvent).then(|| TelemetryLogRecord::new(target.metric_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricValue;

    #[test]
    fn resolves_all_legacy_events_to_schema_targets() {
        for event_type in LegacyEventType::ALL {
            let target = legacy_otel_target(*event_type).expect("event should have schema target");

            assert_eq!(target.event_type, *event_type);
            assert!(!target.metric_name.is_empty());
        }
    }

    #[test]
    fn exposes_metric_kind_for_dual_write_routing() {
        let counter = legacy_otel_target(LegacyEventType::ToolUseSuggested).expect("tool use target");
        assert_eq!(counter.metric_name, "tool_call_total");
        assert_eq!(counter.metric_kind, MetricKind::Counter);

        let log = legacy_otel_target(LegacyEventType::RecordUserTurnCompletion).expect("turn completion target");
        assert_eq!(log.metric_name, "kiro_cli_user_turn_completed");
        assert_eq!(log.metric_kind, MetricKind::LogEvent);
    }

    #[test]
    fn creates_metric_records_for_metric_shaped_targets() {
        let counter = legacy_metric_record(LegacyEventType::ToolUseSuggested).expect("counter target");
        assert_eq!(counter.name, "tool_call_total");
        assert_eq!(counter.value, MetricValue::Counter(1));
        assert!(emits_legacy_tool_call_total(LegacyEventType::ToolUseSuggested));
        assert!(!emits_legacy_tool_call_total(LegacyEventType::ChatAddedMessage));
        assert!(emits_legacy_user_turn_counter(LegacyEventType::ChatAddedMessage));
        assert!(!emits_legacy_user_turn_counter(LegacyEventType::ToolUseSuggested));

        let gauge = legacy_metric_record(LegacyEventType::ModeChanged).expect("gauge target");
        assert_eq!(gauge.name, "mode_active_users_weekly");
        assert_eq!(gauge.value, MetricValue::Gauge(1.0));
    }

    #[test]
    fn creates_log_records_for_log_event_targets() {
        assert!(legacy_metric_record(LegacyEventType::RecordUserTurnCompletion).is_none());

        let log = legacy_log_record(LegacyEventType::RecordUserTurnCompletion).expect("log target");
        assert_eq!(log.name, "kiro_cli_user_turn_completed");

        let conversation = legacy_log_record(LegacyEventType::ChatEnd).expect("log target");
        assert_eq!(conversation.name, "kiro_cli_conversation_completed");
    }
}
