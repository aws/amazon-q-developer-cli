use kiro_telemetry::{
    MetricRecord,
    metric,
};
use kiro_telemetry_host::{
    Event,
    EventType,
};
use kiro_telemetry_legacy::event_to_otel_metric_records;

use super::prepare_v1_event;

pub(super) fn records(event: &Event) -> Vec<MetricRecord> {
    let mut event = event.clone();
    match event.ty {
        EventType::ChatStart { .. } => {
            event.metric_context.mode.get_or_insert(metric::Mode::Interactive);
            event
                .metric_context
                .session_start_kind
                .get_or_insert(metric::SessionStartKind::New);
        },
        EventType::RecordUserTurnCompletion { .. } => {
            event.metric_context.mode.get_or_insert(metric::Mode::Interactive);
        },
        _ => {},
    }
    prepare_v1_event(&mut event);
    event_to_otel_metric_records(&event)
}

mod tests;
