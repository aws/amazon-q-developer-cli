use fig_proto::fig::AggregateSessionMetricActionRequest;
use fig_proto::fig::aggregate_session_metric_action_request::{
    Action,
    Increment,
};
use fig_remote_ipc::figterm::FigtermState;

use super::{
    RequestResult,
    RequestResultImpl,
};

pub fn handle_aggregate_session_metric_action_request(
    request: AggregateSessionMetricActionRequest,
    state: &FigtermState,
) -> RequestResult {
    if let Some(result) = state.with_most_recent(|session| {
        if let Some(ref mut metrics) = session.current_session_metrics {
            if let Some(action) = request.action {
                match action {
                    Action::Increment(Increment { field, amount }) => match field.as_str() {
                        "num_popups" => metrics.num_popups += amount.unwrap_or(1),
                        "num_insertions" => metrics.num_insertions += amount.unwrap_or(1),
                        _ => return Err(format!("Unknown field: {field}")),
                    },
                };
            }
        }
        Ok(())
    }) {
        result?;
    }

    RequestResult::success()
}
