//! Tiny HTTP server that receives `forward`-ed Slack events from peer tasks.
//!
//! Each task starts one of these on `KIRO_BOT_DISPATCH_PORT` (8080 by
//! default, set by the Phase 4 runtime stack). When a peer holds the lease
//! for a conversation, it forwards the original Slack event JSON via
//! `POST /dispatch` and the receiving task processes it as if Slack had
//! delivered it directly. The dedup table prevents double-processing if
//! both tasks somehow saw the same event.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{
    Json,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    get,
    post,
};
use serde_json::Value;
use tokio::net::TcpListener;
use tracing::info;

/// Trait the dispatch server calls into. The bot's `engine::core` provides an
/// implementation that re-enters the normal Slack-event dispatch path.
#[async_trait::async_trait]
pub trait Dispatcher: Send + Sync + 'static {
    async fn process_as_if_from_slack(&self, event: Value);
}

#[derive(Clone)]
pub struct DispatchState {
    pub dispatcher: Arc<dyn Dispatcher>,
}

/// Build the dispatch router. Exposed so tests can assemble an `axum::Server`
/// over an ephemeral port without owning the bind/serve loop.
pub fn router(state: DispatchState) -> Router {
    Router::new()
        .route("/dispatch", post(handle_dispatch))
        .route("/healthz", get(handle_health))
        .with_state(state)
}

async fn handle_dispatch(State(state): State<DispatchState>, Json(event): Json<Value>) -> StatusCode {
    state.dispatcher.process_as_if_from_slack(event).await;
    StatusCode::OK
}

async fn handle_health() -> StatusCode {
    StatusCode::OK
}

/// Bind to `port` on all interfaces and serve forever.
pub async fn run_dispatch_server(port: u16, dispatcher: Arc<dyn Dispatcher>) -> anyhow::Result<()> {
    let app = router(DispatchState { dispatcher });
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "dispatch server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default, Clone)]
    struct Recorder {
        seen: Arc<Mutex<Vec<Value>>>,
    }

    #[async_trait::async_trait]
    impl Dispatcher for Recorder {
        async fn process_as_if_from_slack(&self, event: Value) {
            self.seen.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn dispatch_endpoint_records_payloads() {
        let rec = Recorder::default();
        let state = DispatchState {
            dispatcher: Arc::new(rec.clone()),
        };
        let app = router(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // Give axum a moment to start.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/dispatch"))
            .json(&serde_json::json!({"event_id": "evt-1", "text": "hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let seen = rec.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["event_id"], "evt-1");
    }

    #[tokio::test]
    async fn healthz_returns_200() {
        let rec = Recorder::default();
        let state = DispatchState {
            dispatcher: Arc::new(rec),
        };
        let app = router(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let resp = reqwest::get(format!("http://{addr}/healthz")).await.unwrap();
        assert_eq!(resp.status(), 200);
    }
}
