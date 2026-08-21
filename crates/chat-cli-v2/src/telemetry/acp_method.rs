//! Telemetry for inbound ACP extension requests.

use std::sync::{
    Arc,
    OnceLock,
};
use std::time::{
    Duration,
    Instant,
};

use kiro_telemetry::metric::AcpMethodOutcome;
use kiro_telemetry::{
    MetricLogProperties,
    metric,
};
use kiro_telemetry_host::{
    Event,
    EventType,
};
use serde_json::Value;
use tracing::debug;

use crate::telemetry::{
    AcpClientInfo,
    TelemetryThread,
};

const CLIENT_ERROR_CODES: [i64; 4] = [-32_700, -32_600, -32_601, -32_602];

type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// Connection-scoped ACP client identity shared by session and method telemetry.
#[derive(Clone, Debug, Default)]
pub(crate) struct AcpConnectionContext {
    client_info: Arc<OnceLock<AcpClientInfo>>,
}

impl AcpConnectionContext {
    pub(crate) fn initialize(&self, name: String, version: String) {
        let _ = self.client_info.set(AcpClientInfo::new(name, version));
    }

    pub(crate) fn client_info(&self) -> Option<AcpClientInfo> {
        self.client_info.get().cloned()
    }
}

/// Starts one invocation/completion lifecycle for a recognized extension request.
#[derive(Clone)]
pub(crate) struct AcpMethodTelemetry {
    context: AcpConnectionContext,
    event_sink: EventSink,
}

impl AcpMethodTelemetry {
    pub(crate) fn new(telemetry: &TelemetryThread, context: AcpConnectionContext) -> Self {
        let telemetry = telemetry.clone();
        let event_sink: EventSink = Arc::new(move |event| {
            if let Err(err) = telemetry.send_event(event) {
                debug!(%err, "failed to enqueue ACP method telemetry");
            }
        });
        Self { context, event_sink }
    }

    pub(crate) fn observe(&self, method: &str, params: &Value) -> RequestObservation {
        let resolved = ResolvedRequest::new(method, params, self.context.client_info().as_ref());
        (self.event_sink)(resolved.invocation_event());
        RequestObservation {
            event_sink: Arc::clone(&self.event_sink),
            resolved,
            started: Instant::now(),
            finished: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_event_sink(context: AcpConnectionContext, event_sink: EventSink) -> Self {
        Self { context, event_sink }
    }
}

struct ResolvedRequest {
    method: String,
    client_name: Option<String>,
    session_interface: metric::SessionInterface,
    session_id: Option<String>,
}

impl ResolvedRequest {
    fn new(method: &str, params: &Value, client_info: Option<&AcpClientInfo>) -> Self {
        Self {
            method: method.to_owned(),
            client_name: client_info
                .and_then(|info| info.name.reported_name())
                .map(ToOwned::to_owned),
            session_interface: client_info
                .map_or(metric::SessionInterface::ExternalAcp, AcpClientInfo::session_interface),
            session_id: params.get("sessionId").and_then(Value::as_str).map(ToOwned::to_owned),
        }
    }

    fn base_event(&self, ty: EventType) -> Event {
        let mut event = Event::new(ty);
        event.acp_client_name = self.client_name.clone();
        event.set_engine(metric::Engine::V2);
        event.set_session_interface(self.session_interface);
        event.metric_context.log_properties = MetricLogProperties::default().with_session_id(self.session_id.clone());
        event
    }

    fn invocation_event(&self) -> Event {
        self.base_event(EventType::AcpMethodInvoked {
            method: self.method.clone(),
        })
    }

    fn completion_event(&self, outcome: AcpMethodOutcome, elapsed: Duration) -> Event {
        self.base_event(EventType::AcpMethodCompleted {
            method: self.method.clone(),
            outcome,
            duration: elapsed,
        })
    }
}

/// Emits `unknown` if a claimed extension request is abandoned without a response.
pub(crate) struct RequestObservation {
    event_sink: EventSink,
    resolved: ResolvedRequest,
    started: Instant,
    finished: bool,
}

impl RequestObservation {
    pub(crate) fn finish(mut self, result: &Result<Value, sacp::Error>) {
        let outcome = classify_outcome(result);
        self.finished = true;
        self.emit(outcome);
    }

    fn emit(&self, outcome: AcpMethodOutcome) {
        (self.event_sink)(self.resolved.completion_event(outcome, self.started.elapsed()));
    }
}

impl Drop for RequestObservation {
    fn drop(&mut self) {
        if !self.finished {
            self.emit(AcpMethodOutcome::Unknown);
        }
    }
}

fn classify_outcome(result: &Result<Value, sacp::Error>) -> AcpMethodOutcome {
    match result {
        Ok(_) => AcpMethodOutcome::Success,
        Err(error) => match error_code(error) {
            Some(code) if CLIENT_ERROR_CODES.contains(&code) => AcpMethodOutcome::Error,
            _ => AcpMethodOutcome::Fault,
        },
    }
}

fn error_code(error: &sacp::Error) -> Option<i64> {
    serde_json::to_value(error).ok()?.get("code")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sink() -> (EventSink, tokio::sync::mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
        let sink: EventSink = Arc::new(move |event| {
            tx.send(event).expect("test event receiver remains open");
        });
        (sink, rx)
    }

    #[test]
    fn shared_connection_context_preserves_first_client_identity() {
        let context = AcpConnectionContext::default();
        context.initialize("kiro-tui".to_string(), "1.0.0".to_string());
        context.initialize("other-client".to_string(), "2.0.0".to_string());

        let client = context.client_info().expect("client identity initialized");
        assert_eq!(client.name.reported_name(), Some("kiro-tui"));
        assert_eq!(client.version.as_str(), "1.0.0");
    }

    #[test]
    fn finish_classifies_success_client_errors_and_faults() {
        assert_eq!(classify_outcome(&Ok(Value::Null)), AcpMethodOutcome::Success);
        assert_eq!(
            classify_outcome(&Err(sacp::Error::invalid_params())),
            AcpMethodOutcome::Error
        );
        assert_eq!(
            classify_outcome(&Err(sacp::Error::internal_error())),
            AcpMethodOutcome::Fault
        );
    }

    #[test]
    fn drop_without_finish_emits_unknown() {
        let (event_sink, mut rx) = test_sink();
        let resolved = ResolvedRequest::new("_kiro.dev/settings/list", &serde_json::json!({}), None);
        drop(RequestObservation {
            event_sink,
            resolved,
            started: Instant::now(),
            finished: false,
        });

        let event = rx.try_recv().expect("drop emits a completion event");
        assert!(matches!(
            event.ty,
            EventType::AcpMethodCompleted { outcome, .. } if outcome == AcpMethodOutcome::Unknown
        ));
    }
}
