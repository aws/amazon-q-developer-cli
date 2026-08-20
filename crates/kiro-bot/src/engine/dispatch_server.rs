//! Tiny HTTP server that receives `forward`-ed Slack events from peer tasks.
//!
//! Each task starts one of these on `KIRO_BOT_DISPATCH_PORT` (8080 by
//! default, set by the Phase 4 runtime stack). When a peer holds the lease
//! for a conversation, it forwards the original Slack event JSON via
//! `POST /dispatch` and the receiving task processes it as if Slack had
//! delivered it directly. The dedup table prevents double-processing if
//! both tasks somehow saw the same event.
//!
//! `/dispatch` is the only path by which a non-Slack caller can inject an
//! event into the bot, so it requires a shared secret: a request whose
//! `x-kiro-bot-dispatch-token` doesn't match `KIRO_BOT_DISPATCH_TOKEN` is
//! rejected before the payload is parsed. When the token is unset the route
//! is not mounted at all — an unauthenticated forwarding endpoint is never
//! the safer default, even in dev.

use std::net::{
    IpAddr,
    Ipv4Addr,
    SocketAddr,
};
use std::sync::Arc;

use axum::Router;
use axum::extract::{
    Json,
    State,
};
use axum::http::{
    HeaderMap,
    StatusCode,
};
use axum::routing::{
    get,
    post,
};
use serde_json::Value;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tracing::{
    info,
    warn,
};

/// Env var carrying the shared secret peers present on `POST /dispatch`.
pub const ENV_DISPATCH_TOKEN: &str = "KIRO_BOT_DISPATCH_TOKEN";

/// Header peers use to present the shared secret.
pub const DISPATCH_TOKEN_HEADER: &str = "x-kiro-bot-dispatch-token";

/// Trait the dispatch server calls into. The bot's `engine::core` provides an
/// implementation that re-enters the normal Slack-event dispatch path.
#[async_trait::async_trait]
pub trait Dispatcher: Send + Sync + 'static {
    async fn process_as_if_from_slack(&self, event: Value) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct DispatchState {
    pub dispatcher: Arc<dyn Dispatcher>,
    pub token: Arc<String>,
}

/// Build the dispatch router. Exposed so tests can assemble an `axum::Server`
/// over an ephemeral port without owning the bind/serve loop.
pub fn router(state: DispatchState) -> Router {
    Router::new()
        .route("/dispatch", post(handle_dispatch))
        .route("/healthz", get(handle_health))
        .with_state(state)
}

/// `/healthz` stays unauthenticated for the ALB, so it must not be mounted on a
/// router that carries a dispatcher secret check bypass — it returns a bare 200
/// and reads no state.
async fn handle_health() -> StatusCode {
    StatusCode::OK
}

async fn handle_dispatch(
    State(state): State<DispatchState>,
    headers: HeaderMap,
    Json(event): Json<Value>,
) -> StatusCode {
    let presented = headers.get(DISPATCH_TOKEN_HEADER).and_then(|v| v.to_str().ok());
    let Some(presented) = presented else {
        warn!("rejected /dispatch: missing token header");
        return StatusCode::UNAUTHORIZED;
    };
    if !bool::from(presented.as_bytes().ct_eq(state.token.as_bytes())) {
        warn!("rejected /dispatch: token mismatch");
        return StatusCode::UNAUTHORIZED;
    }
    match state.dispatcher.process_as_if_from_slack(event).await {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            warn!(%error, "forwarded event dispatch failed");
            StatusCode::SERVICE_UNAVAILABLE
        },
    }
}

/// A dispatch server whose TCP listener is already open but which is not yet
/// serving.
///
/// Binding is split from serving so the caller can hold the socket open *before*
/// Slack ingress starts accepting events. Otherwise a peer that already holds the
/// lease for a conversation can `POST /dispatch` in the window between ingress
/// starting and the listener existing, and the forward is refused.
pub struct BoundDispatchServer {
    listener: TcpListener,
    app: Router,
}

impl BoundDispatchServer {
    pub async fn serve(self) -> anyhow::Result<()> {
        axum::serve(self.listener, self.app).await?;
        Ok(())
    }
}

/// Read the configured peer token, if any. Resolved by the caller so
/// `bind_dispatch_server` stays a function of its arguments.
pub fn dispatch_token() -> Option<String> {
    std::env::var(ENV_DISPATCH_TOKEN).ok().filter(|t| !t.is_empty())
}

/// Bind the dispatch endpoint. Binds `0.0.0.0` only when a peer token is
/// configured — without one there is nothing to authenticate forwarded events
/// against, so we bind loopback and refuse to expose `/dispatch` to the subnet.
pub async fn bind_dispatch_server(
    port: u16,
    dispatcher: Arc<dyn Dispatcher>,
    token: Option<String>,
) -> anyhow::Result<BoundDispatchServer> {
    let (bind_ip, app) = match token.filter(|t| !t.is_empty()) {
        Some(token) => (
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            router(DispatchState {
                dispatcher,
                token: Arc::new(token),
            }),
        ),
        None => {
            warn!(
                "{ENV_DISPATCH_TOKEN} unset — binding loopback and serving /healthz only; \
                 cross-task event forwarding is disabled"
            );
            (
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                Router::new().route("/healthz", get(handle_health)),
            )
        },
    };
    let addr = SocketAddr::new(bind_ip, port);
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "dispatch server listening");
    Ok(BoundDispatchServer { listener, app })
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
        async fn process_as_if_from_slack(&self, event: Value) -> anyhow::Result<()> {
            self.seen.lock().unwrap().push(event);
            Ok(())
        }
    }

    struct RejectingDispatcher;

    #[async_trait::async_trait]
    impl Dispatcher for RejectingDispatcher {
        async fn process_as_if_from_slack(&self, _event: Value) -> anyhow::Result<()> {
            anyhow::bail!("receiver rejected forwarded event")
        }
    }

    const TEST_TOKEN: &str = "test-shared-secret";

    async fn serve() -> (Recorder, SocketAddr) {
        let rec = Recorder::default();
        let app = router(DispatchState {
            dispatcher: Arc::new(rec.clone()),
            token: Arc::new(TEST_TOKEN.to_string()),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // Give axum a moment to start.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        (rec, addr)
    }

    #[tokio::test]
    async fn dispatch_endpoint_records_payloads_with_valid_token() {
        let (rec, addr) = serve().await;
        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/dispatch"))
            .header(DISPATCH_TOKEN_HEADER, TEST_TOKEN)
            .json(&serde_json::json!({"event_id": "evt-1", "text": "hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let seen = rec.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["event_id"], "evt-1");
    }

    /// A forged event must not reach the dispatcher: anyone who can route to the
    /// task's port could otherwise impersonate Slack and drive the agent.
    #[tokio::test]
    async fn dispatch_rejects_missing_and_wrong_token() {
        let (rec, addr) = serve().await;
        let client = reqwest::Client::new();
        let body = serde_json::json!({"event_id": "forged", "text": "hi"});

        let no_header = client
            .post(format!("http://{addr}/dispatch"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(no_header.status(), 401);

        let wrong = client
            .post(format!("http://{addr}/dispatch"))
            .header(DISPATCH_TOKEN_HEADER, "not-the-secret")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(wrong.status(), 401);

        assert!(
            rec.seen.lock().unwrap().is_empty(),
            "unauthenticated events must never reach the dispatcher"
        );
    }

    #[tokio::test]
    async fn healthz_returns_200_without_token() {
        let (_rec, addr) = serve().await;
        let resp = reqwest::get(format!("http://{addr}/healthz")).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn dispatch_returns_non_success_when_receiver_rejects_event() {
        let app = router(DispatchState {
            dispatcher: Arc::new(RejectingDispatcher),
            token: Arc::new(TEST_TOKEN.to_string()),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/dispatch"))
            .header(DISPATCH_TOKEN_HEADER, TEST_TOKEN)
            .json(&serde_json::json!({"type": "event_callback"}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    async fn bind_with_token(token: Option<&str>) -> BoundDispatchServer {
        bind_dispatch_server(0, Arc::new(Recorder::default()), token.map(str::to_string))
            .await
            .unwrap()
    }

    /// The point of splitting bind from serve: the socket must already accept
    /// connections before the caller starts Slack ingress, so a peer forward that
    /// races startup is queued by the kernel rather than refused.
    #[tokio::test]
    async fn binding_opens_the_socket_before_serving() {
        let bound = bind_with_token(Some(TEST_TOKEN)).await;
        let addr = bound.listener.local_addr().unwrap();

        // Connect before `serve()` is ever called: a refused connection here is
        // exactly the startup race this split closes.
        let early = tokio::net::TcpStream::connect(addr).await;
        assert!(early.is_ok(), "listener must accept connections before serve()");

        tokio::spawn(bound.serve());
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/dispatch"))
            .header(DISPATCH_TOKEN_HEADER, TEST_TOKEN)
            .json(&serde_json::json!({"event_id": "evt-after-serve"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Without a peer token there is nothing to authenticate forwards against, so
    /// `/dispatch` must not exist and the bind must stay on loopback.
    #[tokio::test]
    async fn unauthenticated_bind_serves_only_healthz_on_loopback() {
        let bound = bind_with_token(None).await;
        let addr = bound.listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback(), "must not expose an unauthenticated port");
        tokio::spawn(bound.serve());

        assert_eq!(
            reqwest::get(format!("http://{addr}/healthz")).await.unwrap().status(),
            StatusCode::OK
        );
        let dispatch = reqwest::Client::new()
            .post(format!("http://{addr}/dispatch"))
            .json(&serde_json::json!({"event_id": "forged"}))
            .send()
            .await
            .unwrap();
        assert_eq!(dispatch.status(), StatusCode::NOT_FOUND);
    }
}
