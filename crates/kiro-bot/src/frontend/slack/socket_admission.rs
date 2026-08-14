use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use slack_morphism::prelude::*;
use tokio::sync::{
    Semaphore,
    mpsc,
    oneshot,
};
use tracing::{
    error,
    warn,
};

use super::SlackState;

const EVENT_QUEUE_CAPACITY: usize = 256;
const EVENT_PROCESSING_CONCURRENCY: usize = 16;
const EVENT_ROUTING_TIMEOUT: Duration = Duration::from_secs(2);
const EVENT_QUEUE_FULL_ERROR: &str = "slack_event_queue_full";
const EVENT_QUEUE_CLOSED_ERROR: &str = "slack_event_queue_closed";
const EVENT_ROUTING_FAILED_ERROR: &str = "slack_event_routing_failed";

type RoutingResult = std::result::Result<(), String>;
type DispatchFuture = Pin<Box<dyn Future<Output = RoutingResult> + Send>>;
type EventDispatcher = Arc<dyn Fn(SlackPushEventCallback) -> DispatchFuture + Send + Sync>;

#[derive(Clone)]
pub struct SlackSocketState {
    event_tx: mpsc::Sender<QueuedSlackEvent>,
}

struct QueuedSlackEvent {
    event: SlackPushEventCallback,
    routing_tx: oneshot::Sender<RoutingResult>,
}

#[derive(Debug, PartialEq, Eq)]
enum EventEnqueueError {
    Full,
    Closed,
}

impl SlackSocketState {
    pub fn new(state: Arc<SlackState>) -> Self {
        Self::with_dispatcher(move |event| {
            let state = state.clone();
            async move {
                super::dispatch_event(event, &state, false)
                    .await
                    .map_err(|error| error.to_string())
            }
        })
    }

    fn with_dispatcher<F, Fut>(dispatcher: F) -> Self
    where
        F: Fn(SlackPushEventCallback) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RoutingResult> + Send + 'static,
    {
        let dispatcher: EventDispatcher = Arc::new(move |event| Box::pin(dispatcher(event)));
        let (event_tx, mut event_rx) = mpsc::channel::<QueuedSlackEvent>(EVENT_QUEUE_CAPACITY);
        let permits = Arc::new(Semaphore::new(EVENT_PROCESSING_CONCURRENCY));
        tokio::spawn(async move {
            while let Some(queued) = event_rx.recv().await {
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    break;
                };
                let dispatcher = dispatcher.clone();
                tokio::spawn(async move {
                    let QueuedSlackEvent { event, routing_tx } = queued;
                    let result = dispatcher(event).await;
                    if let Err(error) = &result {
                        warn!(%error, "Slack event processing failed");
                    }
                    let _ = routing_tx.send(result);
                    drop(permit);
                });
            }
        });
        Self { event_tx }
    }

    fn enqueue(&self, event: SlackPushEventCallback) -> Result<oneshot::Receiver<RoutingResult>, EventEnqueueError> {
        let (routing_tx, routing_rx) = oneshot::channel();
        self.event_tx
            .try_send(QueuedSlackEvent { event, routing_tx })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => EventEnqueueError::Full,
                mpsc::error::TrySendError::Closed(_) => EventEnqueueError::Closed,
            })?;
        Ok(routing_rx)
    }
}

pub async fn on_push(
    event: SlackPushEventCallback,
    _client: Arc<SlackHyperClient>,
    states: SlackClientEventsUserState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guard = states.read().await;
    let state = guard.get_user_state::<SlackSocketState>().ok_or("no state")?.clone();
    drop(guard);
    let event_id = event.event_id.0.clone();
    match state.enqueue(event) {
        Ok(routing_rx) => await_socket_routing(&event_id, routing_rx, EVENT_ROUTING_TIMEOUT).await,
        Err(EventEnqueueError::Full) => Err(Box::new(std::io::Error::other(format!(
            "{EVENT_QUEUE_FULL_ERROR}:{event_id}"
        )))),
        Err(EventEnqueueError::Closed) => Err(Box::new(std::io::Error::other(EVENT_QUEUE_CLOSED_ERROR))),
    }
}

async fn await_socket_routing(
    event_id: &str,
    routing_rx: oneshot::Receiver<RoutingResult>,
    timeout: Duration,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match tokio::time::timeout(timeout, routing_rx).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(error))) => Err(Box::new(std::io::Error::other(format!(
            "{EVENT_ROUTING_FAILED_ERROR}:{event_id}:{error}"
        )))),
        Ok(Err(_)) => Err(Box::new(std::io::Error::other(format!(
            "{EVENT_QUEUE_CLOSED_ERROR}:{event_id}"
        )))),
        Err(_) => Ok(()),
    }
}

pub fn on_error(
    err: Box<dyn std::error::Error + Send + Sync>,
    _: Arc<SlackHyperClient>,
    _: SlackClientEventsUserState,
) -> http::StatusCode {
    let msg = err.to_string();
    if !should_ack_socket_error(&msg) {
        error!(
            capacity = EVENT_QUEUE_CAPACITY,
            slack_error = msg,
            "Slack event was not acknowledged so Slack can retry it"
        );
        return http::StatusCode::SERVICE_UNAVAILABLE;
    }
    if msg.contains("ConnectionReset") || msg.contains("ResetWithoutClosingHandshake") {
        warn!("Slack WebSocket reconnecting: {err}");
    } else {
        error!("Slack: {err}");
    }
    http::StatusCode::OK
}

fn should_ack_socket_error(message: &str) -> bool {
    !message.contains(EVENT_QUEUE_FULL_ERROR)
        && !message.contains(EVENT_QUEUE_CLOSED_ERROR)
        && !message.contains(EVENT_ROUTING_FAILED_ERROR)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use tokio::sync::Notify;

    use super::*;

    fn push_event(event_id: &str) -> SlackPushEventCallback {
        serde_json::from_value(serde_json::json!({
            "team_id": "T1",
            "api_app_id": "A1",
            "event_id": event_id,
            "event_time": 1,
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
                "text": "hello",
                "ts": "1.0"
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn callback_timeout_does_not_cancel_accepted_dispatch() {
        let release = Arc::new(Notify::new());
        let side_effects = Arc::new(AtomicUsize::new(0));
        let state = SlackSocketState::with_dispatcher({
            let release = release.clone();
            let side_effects = side_effects.clone();
            move |_| {
                let release = release.clone();
                let side_effects = side_effects.clone();
                async move {
                    release.notified().await;
                    side_effects.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }
        });
        let routing_rx = state.enqueue(push_event("EvSlow")).unwrap();

        await_socket_routing("EvSlow", routing_rx, Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);

        release.notify_one();
        tokio::time::timeout(Duration::from_secs(1), async {
            while side_effects.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(side_effects.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn socket_callback_queue_is_bounded_and_nonblocking() {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        let state = SlackSocketState { event_tx };

        let routing_rx = state.enqueue(push_event("Ev1")).unwrap();
        assert!(matches!(state.enqueue(push_event("Ev2")), Err(EventEnqueueError::Full)));
        let queued = event_rx.recv().await.unwrap();
        assert_eq!(queued.event.event_id.0, "Ev1");
        queued.routing_tx.send(Ok(())).unwrap();
        assert_eq!(routing_rx.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn socket_callback_surfaces_routing_failure() {
        let (failure_tx, failure_rx) = oneshot::channel();
        failure_tx.send(Err("coordinator unavailable".into())).unwrap();
        let failure = await_socket_routing("EvFailure", failure_rx, Duration::from_millis(10))
            .await
            .unwrap_err()
            .to_string();
        assert!(failure.contains(EVENT_ROUTING_FAILED_ERROR));
    }

    #[test]
    fn saturated_or_closed_event_queues_are_left_unacknowledged_for_retry() {
        assert!(!should_ack_socket_error(EVENT_QUEUE_FULL_ERROR));
        assert!(!should_ack_socket_error(EVENT_QUEUE_CLOSED_ERROR));
        assert!(!should_ack_socket_error(EVENT_ROUTING_FAILED_ERROR));
        assert!(should_ack_socket_error("ConnectionReset"));
    }
}
