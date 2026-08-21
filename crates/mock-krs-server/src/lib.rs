//! A fake Kiro Runtime Service for deterministic KAS integration tests.
//!
//! # What it is
//!
//! KRS is the model-inference backend KAS calls. Point a real, published
//! `@kiro/agent` at this server (`KIRO_KAS_ENDPOINT` → KAS's `--endpoint`) and
//! the whole stack below the model stays real — real agent loop, real tools,
//! real sessions, real ACP — while the model's output becomes a value a test
//! wrote down. That is what makes a KAS version bump verifiable: the only
//! variable left is the KAS artifact itself.
//!
//! Scope is KAS (v3) only. The one operation is `GenerateAssistantResponse`.
//!
//! # The wire is generated, not written
//!
//! Routing, request deserialization, response serialization and event-stream
//! framing all come from [`amzn_kiro_runtime_service_server_sdk`], generated
//! from `KiroRuntimeServiceModel`. This crate never spells out a header name, a
//! status code or a JSON member: it decides *which* modeled value to return and
//! lets the SDK put it on the wire. A model change that renames or drops a
//! member is therefore a compile error here.
//!
//! KAS addresses KRS with `awsJson1_0`: `POST /` with the operation in an
//! `x-amz-target` header. Verified against the client bundled in
//! `@kiro/agent` (`protocol: AwsJson1_0Protocol`, `serviceTarget:
//! "KiroRuntimeService"`) and against its live traffic, not from the
//! standalone copy of the KRS TypeScript client, which is generated for
//! restJson1 and would send something KAS never sends.
//!
//! # Two surfaces
//!
//! * The **data plane** is the generated service.
//! * The **control plane** (`/__control/*`) is the test harness's: it injects scenarios, reads back
//!   captured requests, and resets between tests. It is namespaced under a path KRS itself would
//!   never serve, so a stray call from KAS can never be mistaken for harness traffic.
//!
//! # Failing loudly
//!
//! An unscripted request is an error, never a stubbed-out default. A default
//! would let an incorrectly scripted test pass while asserting nothing. That is
//! a failure mode a version-bump gate cannot afford.

pub mod scenario;
pub mod wire;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::time::Duration;

use amzn_kiro_runtime_service_server_sdk::error::{
    AccessDeniedException,
    GenerateAssistantResponseError,
    InternalServerException,
    ServiceQuotaExceededException,
    ServiceUnavailableException,
    ThrottlingException,
    ValidationException,
};
use amzn_kiro_runtime_service_server_sdk::input::GenerateAssistantResponseInput;
use amzn_kiro_runtime_service_server_sdk::model::{
    ChatResponseStream,
    StopReason,
};
use amzn_kiro_runtime_service_server_sdk::output::GenerateAssistantResponseOutput;
use amzn_kiro_runtime_service_server_sdk::{
    KiroRuntimeService,
    KiroRuntimeServiceConfig,
};
use anyhow::Context as _;
use aws_smithy_http_server::request::extension::Extension as SmithyExtension;
use axum::Router;
use axum::body::Body;
use axum::extract::{
    Request,
    State,
};
use axum::http::{
    HeaderMap,
    StatusCode,
};
use axum::response::{
    IntoResponse,
    Response,
};
use axum::routing::{
    get,
    post,
};
use bytes::Bytes;
use http_body_util::BodyExt as _;
use serde::Serialize;
use serde_json::{
    Value,
    json,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tower::{
    Service as _,
    ServiceExt as _,
};

use crate::scenario::{
    HttpError,
    HttpErrorKind,
    Matcher,
    Respond,
    Turn,
};

/// The `x-amz-target` value the generated service routes on. Exposed for tests
/// and diagnostics; the mock does not dispatch on it itself.
pub const GENERATE_ASSISTANT_RESPONSE_TARGET: &str = "KiroRuntimeService.GenerateAssistantResponse";

/// awsJson1_0 posts every operation to the root.
pub const GENERATE_ASSISTANT_RESPONSE_PATH: &str = "/";

/// Cap on a captured request body. KAS request bodies carry conversation
/// history and can be large, but an unbounded buffer in a test server is a way
/// to hang CI rather than fail it.
const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address to bind. Port 0 (the default) picks a free port, which is what
    /// lets tests run in parallel without a port registry.
    pub bind: SocketAddr,
    /// When set, requests must present exactly this bearer token. Left unset,
    /// any bearer token is accepted (but one is still required — KAS always
    /// sends one, so its absence is a wiring bug worth surfacing).
    pub api_key: Option<String>,
    /// Reject requests whose body does not carry a user message. Catches gross
    /// request-shape breakage at the first call instead of as a stalled turn.
    pub validate_requests: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_key: None,
            validate_requests: true,
        }
    }
}

/// One captured `GenerateAssistantResponse` call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedRequest {
    /// Zero-based call index since the last reset.
    pub index: usize,
    pub path: String,
    /// The parsed request body. Exposed whole, so a test can assert on parts of
    /// the request this crate does not model — history shape, advertised tools,
    /// images — without a new field per case.
    pub body: Value,
    /// Interesting headers only. The full set is noise, and the bearer token
    /// must not be echoed.
    pub headers: Vec<(String, String)>,
    /// Which scripted turn answered this call, if any.
    pub matched_turn: Option<String>,
}

impl CapturedRequest {
    /// `conversationState.currentMessage.userInputMessage.content`, if present.
    ///
    /// Reads the captured JSON rather than the modeled input: this is the
    /// harness-facing view, and it stays useful even for a body the model
    /// could not fully deserialize.
    pub fn user_input(&self) -> Option<&str> {
        self.body
            .pointer("/conversationState/currentMessage/userInputMessage/content")
            .and_then(Value::as_str)
    }
}

/// Queue state, for `GET /__control/state`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    pub queued_turns: Vec<String>,
    pub calls: usize,
    pub unmatched_calls: usize,
    /// Control-plane operations this server answered, in order. KAS gates a
    /// prompt on these, so a harness that sees no model traffic can tell
    /// "never got past the registry" from "never called at all".
    pub control_plane_calls: Vec<String>,
    /// Operations that arrived and are not modelled here, in order. Recorded so
    /// a KAS version that starts calling something new names itself, instead of
    /// looking like silence.
    pub unknown_targets: Vec<String>,
}

struct QueuedTurn {
    turn: Turn,
    /// Remaining uses; `None` is unlimited.
    remaining: Option<u32>,
}

struct Inner {
    config: Config,
    queue: Mutex<Vec<QueuedTurn>>,
    requests: Mutex<Vec<CapturedRequest>>,
    calls: AtomicUsize,
    unmatched: AtomicUsize,
    control_plane_calls: Mutex<Vec<String>>,
    unknown_targets: Mutex<Vec<String>>,
}

/// Everything the operation handler needs that is not in the modeled input:
/// the raw bytes (for `bodyContains` matchers and harness assertions) and the
/// headers (for the bearer check).
///
/// Collected by a layer in front of the generated service, because the modeled
/// input deliberately does not carry them.
#[derive(Debug, Clone)]
struct RequestContext {
    raw_body: Bytes,
    headers: Vec<(String, String)>,
    bearer: Option<String>,
    path: String,
}

/// A running fake KRS.
pub struct MockKrsServer {
    addr: SocketAddr,
    inner: Arc<Inner>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl MockKrsServer {
    /// Binds and starts serving. Resolves once the listener is accepting, so a
    /// caller can hand the endpoint to KAS without racing the bind.
    pub async fn start(config: Config) -> anyhow::Result<Self> {
        let inner = Arc::new(Inner {
            config: config.clone(),
            queue: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
            unmatched: AtomicUsize::new(0),
            control_plane_calls: Mutex::new(Vec::new()),
            unknown_targets: Mutex::new(Vec::new()),
        });

        let listener = tokio::net::TcpListener::bind(config.bind)
            .await
            .with_context(|| format!("binding mock KRS to {}", config.bind))?;
        let addr = listener.local_addr().context("reading mock KRS local address")?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let app = router(Arc::clone(&inner))?;
        let handle = tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = serve.await {
                tracing::error!(%error, "mock KRS server stopped");
            }
        });

        Ok(Self {
            addr,
            inner,
            shutdown: Some(shutdown_tx),
            handle,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The value to hand KAS as its `--endpoint`.
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Enqueues scripted turns in process, the same path the control API takes.
    pub async fn enqueue(&self, turns: Vec<Turn>) {
        enqueue_turns(&self.inner, turns).await;
    }

    pub async fn requests(&self) -> Vec<CapturedRequest> {
        self.inner.requests.lock().await.clone()
    }

    pub async fn reset(&self) {
        reset_state(&self.inner).await;
    }

    pub async fn state(&self) -> ServerState {
        snapshot_state(&self.inner).await
    }

    /// Stops the server and waits for the accept loop to finish.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = (&mut self.handle).await;
    }
}

impl Drop for MockKrsServer {
    fn drop(&mut self) {
        // A test that forgets `shutdown()` (or panics) still releases the port.
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// The body the generated service reads requests from.
///
/// Identical to axum's, except the error is boxed: the SDK's request rejection
/// converts from a boxed error, not from `axum::Error`.
type SmithyBody = http_body_util::combinators::MapErr<Body, fn(axum::Error) -> BoxError>;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn box_body_error(error: axum::Error) -> BoxError {
    Box::new(error)
}

/// The generated service, type-erased so it can be held in axum state.
type KrsService = tower::util::BoxCloneService<
    axum::http::Request<SmithyBody>,
    axum::http::Response<aws_smithy_http_server::body::BoxBody>,
    std::convert::Infallible,
>;

/// A handle to the generated service that is `Sync`.
///
/// The SDK's router holds a `Box<dyn CloneService + Send>`, which is not `Sync`,
/// and axum state must be. The lock is taken only to clone the (cheap) service
/// handle and is never held across an await.
#[derive(Clone)]
struct KrsHandle(Arc<std::sync::Mutex<KrsService>>);

impl KrsHandle {
    fn service(&self) -> KrsService {
        self.0.lock().expect("mock KRS service lock poisoned").clone()
    }
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Inner>,
    krs: KrsHandle,
}

/// Control plane on axum, data plane on the generated service.
///
/// Every path the control plane does not claim goes to the generated service,
/// so an unmodeled route gets the SDK's own protocol-correct answer rather than
/// an axum 404 that KAS's client would report as an unknown failure.
fn router(inner: Arc<Inner>) -> anyhow::Result<Router> {
    let krs = KiroRuntimeService::builder::<SmithyBody, _, _, _>(KiroRuntimeServiceConfig::builder().build())
        .generate_assistant_response(generate_assistant_response)
        .build()
        .context("building the generated KiroRuntimeService (an operation is unimplemented)")?;

    let state = AppState {
        inner,
        krs: KrsHandle(Arc::new(std::sync::Mutex::new(tower::util::BoxCloneService::new(krs)))),
    };

    Ok(Router::new()
        .route("/__control/health", get(control_health))
        .route("/__control/state", get(control_state))
        .route("/__control/turns", post(control_enqueue))
        .route("/__control/requests", get(control_requests))
        .route("/__control/reset", post(control_reset))
        .fallback(serve_generated)
        .with_state(state))
}

// ── control plane ───────────────────────────────────────────────────────────

async fn control_health(State(state): State<AppState>) -> Response {
    let state = snapshot_state(&state.inner).await;
    axum::Json(json!({ "ok": true, "queuedTurns": state.queued_turns.len() })).into_response()
}

async fn control_state(State(state): State<AppState>) -> Response {
    axum::Json(snapshot_state(&state.inner).await).into_response()
}

/// Enqueues turns.
///
/// The body is KRS traffic — `match` a request, `respond` with events — so
/// nothing is inferred. A caller that has a TUI scenario rather than turns
/// resolves it to turns first; this server does not read scenarios.
///
/// Body: `{"turns": [...]}` or a bare `[...]`.
async fn control_enqueue(State(state): State<AppState>, body: Bytes) -> Response {
    // Parsed here rather than via `Json<...>` so a malformed scenario answers
    // with the serde message naming the offending field. A rejected extractor
    // would only say "unprocessable entity", which is a miserable thing to
    // debug from a test log.
    match scenario::parse_scenario_batch(&body) {
        Ok(turns) => {
            let queued = turns.len();
            enqueue_turns(&state.inner, turns).await;
            axum::Json(json!({ "queued": queued })).into_response()
        },
        Err(error) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": format!("invalid scenario: {error}") })),
        )
            .into_response(),
    }
}

async fn control_requests(State(state): State<AppState>) -> Response {
    let requests = state.inner.requests.lock().await.clone();
    axum::Json(json!({ "requests": requests })).into_response()
}

async fn control_reset(State(state): State<AppState>) -> Response {
    reset_state(&state.inner).await;
    axum::Json(json!({ "ok": true })).into_response()
}

async fn enqueue_turns(inner: &Arc<Inner>, turns: Vec<Turn>) {
    let mut queue = inner.queue.lock().await;
    for turn in turns {
        let remaining = turn.budget();
        queue.push(QueuedTurn { turn, remaining });
    }
}

async fn reset_state(inner: &Arc<Inner>) {
    inner.queue.lock().await.clear();
    inner.requests.lock().await.clear();
    inner.calls.store(0, Ordering::SeqCst);
    inner.unmatched.store(0, Ordering::SeqCst);
}

async fn snapshot_state(inner: &Arc<Inner>) -> ServerState {
    ServerState {
        queued_turns: inner.queue.lock().await.iter().map(|q| q.turn.label()).collect(),
        calls: inner.calls.load(Ordering::SeqCst),
        unmatched_calls: inner.unmatched.load(Ordering::SeqCst),
        control_plane_calls: inner.control_plane_calls.lock().await.clone(),
        unknown_targets: inner.unknown_targets.lock().await.clone(),
    }
}

// ── data plane ──────────────────────────────────────────────────────────────

/// Buffers the request so the handler can see the bytes KAS actually sent, then
/// hands it to the generated service.
///
/// The buffering exists because the modeled input deliberately does not carry
/// raw bytes or headers, and the harness wants both. State travels the rest of
/// the way in request extensions, which is how a smithy handler reads anything
/// outside its modeled input.
async fn serve_generated(State(state): State<AppState>, request: Request) -> Response {
    let (mut parts, body) = request.into_parts();

    // KAS reaches two services, not one. Model traffic is KRS, but a prompt is
    // gated on the *control plane*: KAS lists the available models before it
    // will generate anything. Both are pointed here by the harness, so answer
    // the control-plane operations we know and record the ones we don't.
    if let Some(operation) = target_operation(&parts.headers) {
        match operation.as_str() {
            LIST_AVAILABLE_MODELS => {
                let bearer = bearer_token(&parts.headers);
                state.inner.control_plane_calls.lock().await.push(operation);
                return control_plane_list_models(&state.inner.config, bearer.as_deref());
            },
            GENERATE_ASSISTANT_RESPONSE => {},
            // Not modelled here. Recorded before the generated service answers
            // it, so the captured exchange names the operation rather than
            // showing an empty request log next to a mystery failure.
            other => state.inner.unknown_targets.lock().await.push(other.to_string()),
        }
    }

    let raw_body = match axum::body::to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            // Transport-level, not something KRS models: say so plainly rather
            // than dressing it up as a service error.
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("mock KRS could not read the request body: {error}"),
            )
                .into_response();
        },
    };

    let context = RequestContext {
        raw_body: raw_body.clone(),
        headers: interesting_headers(&parts.headers),
        bearer: bearer_token(&parts.headers),
        path: parts.uri.path().to_string(),
    };
    parts.extensions.insert(Arc::clone(&state.inner));
    parts.extensions.insert(context);

    let request = Request::from_parts(parts, Body::from(raw_body))
        .map(|body| body.map_err(box_body_error as fn(axum::Error) -> BoxError));

    let mut service = state.krs.service();
    let response = match service.ready().await {
        Ok(service) => service.call(request).await,
        Err(infallible) => match infallible {},
    };
    match response {
        Ok(response) => response.map(Body::new).into_response(),
        Err(infallible) => match infallible {},
    }
}

/// The one modeled operation.
///
/// Everything protocol-shaped — status codes, headers, event framing — belongs
/// to the SDK. This function only chooses which modeled value to return.
async fn generate_assistant_response(
    input: GenerateAssistantResponseInput,
    state: SmithyExtension<Arc<Inner>>,
    context: SmithyExtension<RequestContext>,
) -> Result<GenerateAssistantResponseOutput, GenerateAssistantResponseError> {
    let inner = state.0;
    let context = context.0;

    check_auth(&inner.config, &context)?;

    let index = inner.calls.fetch_add(1, Ordering::SeqCst);
    let mut captured = CapturedRequest {
        index,
        path: context.path.clone(),
        body: serde_json::from_slice(&context.raw_body).unwrap_or(Value::Null),
        headers: context.headers.clone(),
        matched_turn: None,
    };

    if inner.config.validate_requests && user_input(&input).is_none() {
        inner.requests.lock().await.push(captured);
        return Err(validation_error(
            "mock KRS: request carries no conversationState.currentMessage.userInputMessage.content",
        ));
    }

    let raw_body = String::from_utf8_lossy(&context.raw_body).into_owned();
    let respond = match take_matching_turn(&inner, &input, &raw_body, index).await {
        Some((label, respond)) => {
            captured.matched_turn = Some(label);
            inner.requests.lock().await.push(captured);
            respond
        },
        None => {
            inner.unmatched.fetch_add(1, Ordering::SeqCst);
            let input_text = user_input(&input).unwrap_or("<none>").to_string();
            let queued = snapshot_state(&inner).await.queued_turns;
            inner.requests.lock().await.push(captured);
            // Deliberately fatal. See the module docs: a stubbed default here
            // would turn an incorrectly scripted test into a passing one.
            return Err(internal_error(format!(
                "mock KRS has no scripted response for call {index} (user input: {input_text:?}); queued turns: {queued:?}"
            )));
        },
    };

    if let Some(http_error) = &respond.http_error {
        return Err(modeled_error(http_error));
    }

    // Marshalled up front so a scripting mistake becomes a modeled error rather
    // than a stream that dies halfway with no explanation.
    let steps = plan_stream(&respond).map_err(|error| {
        internal_error(format!(
            "mock KRS cannot express this scenario as modeled events: {error:#}"
        ))
    })?;

    let stream = async_stream::stream! {
        for step in steps {
            match step {
                Step::Event(event) => yield Ok(event),
                Step::Fail(error) => yield Err(error),
                Step::Pause(duration) => tokio::time::sleep(duration).await,
            }
        }
    };

    GenerateAssistantResponseOutput::builder()
        .generate_assistant_response_response(stream.into())
        .build()
        .map_err(|error| internal_error(format!("mock KRS could not build a response: {error}")))
}

enum Step {
    Event(ChatResponseStream),
    Fail(amzn_kiro_runtime_service_server_sdk::error::ChatResponseStreamError),
    Pause(Duration),
}

/// Turns a scripted response into the ordered steps the stream will replay.
fn plan_stream(respond: &Respond) -> anyhow::Result<Vec<Step>> {
    let mut steps = Vec::new();
    if respond.delay_ms > 0 {
        steps.push(Step::Pause(Duration::from_millis(respond.delay_ms)));
    }

    let mut saw_stop_reason = false;
    for script in &respond.events {
        if let scenario::EventScript::Delay { ms } = script {
            steps.push(Step::Pause(Duration::from_millis(*ms)));
            continue;
        }
        let Some(event) = wire::to_modeled_event(script)? else {
            continue;
        };
        saw_stop_reason |= wire::carries_stop_reason(&event);
        steps.push(Step::Event(event));
        if respond.chunk_delay_ms > 0 {
            steps.push(Step::Pause(Duration::from_millis(respond.chunk_delay_ms)));
        }
    }

    if let Some(stream_error) = &respond.stream_error {
        steps.push(Step::Fail(wire::to_stream_error(stream_error)));
    } else if !respond.truncate && !saw_stop_reason {
        // Real KRS closes a complete response with a stop reason. Omitting one
        // trips KAS's stream-recovery retry, which doubles invocation counts and
        // makes a healthy test look flaky — so the healthy default supplies it
        // and `truncate` is the way to ask for the other behavior.
        steps.push(Step::Event(wire::terminal_metadata_event(StopReason::EndTurn)));
    }

    Ok(steps)
}

/// Pops the first queued turn whose matcher accepts this request.
///
/// First-match rather than strict FIFO: with matchers a test can bind responses
/// to requests and stay correct when tool calls make the call order hard to
/// predict; without them the queue behaves as a plain ordered script.
async fn take_matching_turn(
    inner: &Arc<Inner>,
    input: &GenerateAssistantResponseInput,
    raw_body: &str,
    index: usize,
) -> Option<(String, Respond)> {
    let mut queue = inner.queue.lock().await;
    let position = queue
        .iter()
        .position(|queued| matches_request(&queued.turn.matcher, input, raw_body, index))?;

    let label = queue[position].turn.label();
    let respond = queue[position].turn.respond.clone();
    match queue[position].remaining {
        None => {},
        Some(1) => {
            queue.remove(position);
        },
        Some(n) => queue[position].remaining = Some(n - 1),
    }
    Some((label, respond))
}

/// Matches against the *modeled* request, so a matcher cannot drift from the
/// shape KRS actually accepts. `bodyContains` is the one escape hatch, for
/// asserting on parts of the request the matcher does not model.
fn matches_request(matcher: &Matcher, input: &GenerateAssistantResponseInput, raw_body: &str, index: usize) -> bool {
    if matcher.is_empty() {
        return true;
    }
    let content = user_input(input).unwrap_or_default();

    if let Some(needle) = &matcher.user_input_contains {
        if !content.contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(pattern) = &matcher.user_input_regex {
        // An unparseable pattern must not silently match everything.
        match regex::Regex::new(pattern) {
            Ok(regex) if regex.is_match(content) => {},
            _ => return false,
        }
    }
    if let Some(needle) = &matcher.body_contains {
        if !raw_body.contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(needle) = &matcher.body_not_contains {
        if raw_body.contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(expected) = matcher.has_tool_results {
        if has_tool_results(input) != expected {
            return false;
        }
    }
    if let Some(expected) = &matcher.model_id {
        if model_id(input) != Some(expected.as_str()) {
            return false;
        }
    }
    if let Some(expected) = &matcher.agent_mode {
        if input.agent_mode.as_deref() != Some(expected.as_str()) {
            return false;
        }
    }
    if let Some(expected) = matcher.call_index {
        if index != expected {
            return false;
        }
    }
    true
}

/// The current user message, if this request carries one.
fn current_user_message(
    input: &GenerateAssistantResponseInput,
) -> Option<&amzn_kiro_runtime_service_server_sdk::model::UserInputMessage> {
    input
        .conversation_state
        .as_ref()?
        .current_message
        .as_ref()?
        .as_user_input_message()
        .ok()
}

fn user_input(input: &GenerateAssistantResponseInput) -> Option<&str> {
    current_user_message(input)?.content.as_deref()
}

fn model_id(input: &GenerateAssistantResponseInput) -> Option<&str> {
    current_user_message(input)?.model_id.as_deref()
}

fn has_tool_results(input: &GenerateAssistantResponseInput) -> bool {
    current_user_message(input)
        .and_then(|message| message.user_input_message_context.as_ref())
        .and_then(|context| context.tool_results.as_ref())
        .is_some_and(|results| !results.is_empty())
}

/// KAS always sends a bearer token, so a missing one is a wiring bug worth
/// failing on even when no specific key is configured.
///
/// Shared by both services this fixture answers, so KRS and the control plane
/// cannot disagree about what counts as authenticated.
fn check_bearer(config: &Config, bearer: Option<&str>) -> Result<(), String> {
    let token = match bearer {
        Some(token) if !token.trim().is_empty() => token,
        _ => return Err("mock KRS: request carried no Authorization bearer token".to_string()),
    };

    match &config.api_key {
        Some(expected) if token != expected => {
            Err("mock KRS: bearer token does not match the configured api key".to_string())
        },
        _ => Ok(()),
    }
}

fn check_auth(config: &Config, context: &RequestContext) -> Result<(), GenerateAssistantResponseError> {
    check_bearer(config, context.bearer.as_deref()).map_err(|message| access_denied(&message))
}

/// Maps a scripted failure onto the modeled error it names. The SDK decides the
/// status code and body from the model.
fn modeled_error(error: &HttpError) -> GenerateAssistantResponseError {
    let message = error.message();
    match error.kind {
        HttpErrorKind::AccessDenied => access_denied(&message),
        HttpErrorKind::InternalServer => internal_error(message),
        HttpErrorKind::ServiceQuotaExceeded => GenerateAssistantResponseError::ServiceQuotaExceededException(
            ServiceQuotaExceededException::builder().message(Some(message)).build(),
        ),
        HttpErrorKind::ServiceUnavailable => GenerateAssistantResponseError::ServiceUnavailableException(
            ServiceUnavailableException::builder().message(Some(message)).build(),
        ),
        HttpErrorKind::Throttling => GenerateAssistantResponseError::ThrottlingException(
            ThrottlingException::builder().message(Some(message)).build(),
        ),
        HttpErrorKind::Validation => validation_error(&message),
    }
}

fn access_denied(message: &str) -> GenerateAssistantResponseError {
    GenerateAssistantResponseError::AccessDeniedException(
        AccessDeniedException::builder()
            .message(Some(message.to_string()))
            .build(),
    )
}

fn internal_error(message: String) -> GenerateAssistantResponseError {
    GenerateAssistantResponseError::InternalServerException(
        InternalServerException::builder().message(Some(message)).build(),
    )
}

/// Uses the framework validation exception rather than the model's own, which
/// this crate's codegen renames to `ValidationError` to avoid a name collision.
/// The framework shape is the one that still serializes as `ValidationException`,
/// which is the code KAS's client recognizes.
fn validation_error(message: &str) -> GenerateAssistantResponseError {
    GenerateAssistantResponseError::ValidationException(
        ValidationException::builder()
            .message(message.to_string())
            .build()
            .expect("message is set"),
    )
}

/// Headers a test may reasonably assert on. `authorization` is recorded as
/// present/absent only — never its value.
fn interesting_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    const KEEP: &[&str] = &[
        "x-amz-target",
        "content-type",
        "user-agent",
        "x-amz-user-agent",
        "x-amzn-kiro-agent-mode",
        "amz-sdk-invocation-id",
        "amz-sdk-request",
    ];
    let mut kept: Vec<(String, String)> = KEEP
        .iter()
        .filter_map(|name| header_str(headers, name).map(|value| ((*name).to_string(), value)))
        .collect();
    if headers.contains_key("authorization") {
        kept.push(("authorization".to_string(), "<redacted>".to_string()));
    }
    kept
}

/// The control-plane operation KAS calls before it will serve a prompt.
const LIST_AVAILABLE_MODELS: &str = "ListAvailableModels";
/// The one KRS operation the generated service implements.
const GENERATE_ASSISTANT_RESPONSE: &str = "GenerateAssistantResponse";
/// The model this fixture advertises. KAS only needs an id it can carry.
const MOCK_MODEL_ID: &str = "mock-krs-model";

/// The operation from an `x-amz-target` header (`Service.Operation`).
///
/// Matched on the suffix: the service prefix differs between KRS and the control
/// plane, and pinning either would make this fixture fail for a reason that has
/// nothing to do with the behaviour under test.
fn target_operation(headers: &HeaderMap) -> Option<String> {
    let target = headers.get("x-amz-target")?.to_str().ok()?;
    let operation = target.rsplit('.').next()?.trim();
    (!operation.is_empty()).then(|| operation.to_string())
}

/// The body of a `ListAvailableModels` answer.
///
/// Held to the two members KAS reads — a model carrying an id, and a default.
/// Everything else it treats as optional, and every field added here is a field
/// that can drift from the real service without anything noticing.
fn list_available_models_body() -> Value {
    json!({
        "models": [{
            "modelId": MOCK_MODEL_ID,
            "modelName": "Mock KRS Model",
            "description": "Served by mock-krs-server so a prompt can reach the fake KRS.",
            "status": "Active",
            "modelProvider": "DEFAULT",
        }],
        "defaultModel": { "modelId": MOCK_MODEL_ID },
    })
}

/// `ListAvailableModels`, hand-written rather than generated.
///
/// The KRS side of this fixture is generated from the model, so a model change is
/// a compile error here. The control plane has no server SDK in this workspace —
/// only a TypeScript client — so this one is shaped by hand and a control-plane
/// model change will surface as a failing lane instead. That asymmetry is the
/// reason to keep the body minimal.
fn control_plane_list_models(config: &Config, bearer: Option<&str>) -> Response {
    if let Err(message) = check_bearer(config, bearer) {
        return aws_json_error(StatusCode::BAD_REQUEST, "AccessDeniedException", &message);
    }
    aws_json_ok(&list_available_models_body())
}

/// An AWS JSON 1.0 success body, the framing the control-plane client expects.
fn aws_json_ok(body: &Value) -> Response {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, AWS_JSON_CONTENT_TYPE)],
        body.to_string(),
    )
        .into_response()
}

/// An AWS JSON 1.0 error body. `__type` is what the client maps onto a modeled
/// error, so a rejection reads as that error rather than as a transport failure.
fn aws_json_error(status: StatusCode, kind: &str, message: &str) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, AWS_JSON_CONTENT_TYPE)],
        json!({ "__type": kind, "message": message }).to_string(),
    )
        .into_response()
}

const AWS_JSON_CONTENT_TYPE: &str = "application/x-amz-json-1.0";

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    header_str(headers, "authorization").map(|value| value.strip_prefix("Bearer ").map(str::to_string).unwrap_or(value))
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use amzn_kiro_runtime_service_server_sdk::model::{
        ChatMessage,
        ConversationState,
        ToolResult,
        UserInputMessage,
        UserInputMessageContext,
    };

    use super::*;

    fn request(message: UserInputMessage) -> GenerateAssistantResponseInput {
        GenerateAssistantResponseInput {
            conversation_state: Some(
                ConversationState::builder()
                    .current_message(Some(ChatMessage::UserInputMessage(message)))
                    .build(),
            ),
            profile_arn: None,
            agent_mode: None,
            additional_model_request_fields: None,
            system_prompt: None,
        }
    }

    fn user_message(content: &str) -> GenerateAssistantResponseInput {
        request(UserInputMessage::builder().content(Some(content.to_string())).build())
    }

    fn context(bearer: Option<&str>) -> RequestContext {
        RequestContext {
            raw_body: Bytes::new(),
            headers: Vec::new(),
            bearer: bearer.map(str::to_string),
            path: GENERATE_ASSISTANT_RESPONSE_PATH.to_string(),
        }
    }

    #[test]
    fn empty_matcher_matches_anything() {
        assert!(matches_request(&Matcher::default(), &user_message("anything"), "", 0));
    }

    #[test]
    fn user_input_matchers_read_the_modeled_member() {
        let input = user_message("please read the file");
        let matcher = Matcher {
            user_input_contains: Some("read the file".into()),
            ..Default::default()
        };
        assert!(matches_request(&matcher, &input, "", 0));

        let miss = Matcher {
            user_input_contains: Some("write".into()),
            ..Default::default()
        };
        assert!(!matches_request(&miss, &input, "", 0));
    }

    #[test]
    fn an_invalid_regex_matches_nothing() {
        // Failing closed keeps a typo'd pattern from answering every request.
        let matcher = Matcher {
            user_input_regex: Some("([unclosed".into()),
            ..Default::default()
        };
        assert!(!matches_request(&matcher, &user_message("hello"), "", 0));
    }

    #[test]
    fn tool_result_matcher_distinguishes_continuations() {
        let with_results = request(
            UserInputMessage::builder()
                .content(Some(String::new()))
                .user_input_message_context(Some(
                    UserInputMessageContext::builder()
                        .tool_results(Some(vec![
                            ToolResult::builder().tool_use_id(Some("tu-1".into())).build(),
                        ]))
                        .build(),
                ))
                .build(),
        );
        let without = user_message("first turn");

        let wants_results = Matcher {
            has_tool_results: Some(true),
            ..Default::default()
        };
        assert!(matches_request(&wants_results, &with_results, "", 0));
        assert!(!matches_request(&wants_results, &without, "", 0));
    }

    #[test]
    fn model_id_matcher_reads_the_modeled_member() {
        let input = request(
            UserInputMessage::builder()
                .content(Some("hi".into()))
                .model_id(Some("claude-sonnet".into()))
                .build(),
        );
        let matcher = Matcher {
            model_id: Some("claude-sonnet".into()),
            ..Default::default()
        };
        assert!(matches_request(&matcher, &input, "", 0));
    }

    #[test]
    fn body_contains_reaches_unmodeled_parts_of_the_request() {
        let input = user_message("hi");
        let raw = r#"{"tools":[{"toolSpecification":{"name":"fs_read"}}]}"#;
        let matcher = Matcher {
            body_contains: Some("fs_read".into()),
            ..Default::default()
        };
        assert!(matches_request(&matcher, &input, raw, 0));
        assert!(!matches_request(&matcher, &input, "{}", 0));
    }

    #[test]
    fn body_not_contains_requires_the_needle_to_be_absent() {
        let input = user_message("hi");
        let matcher = Matcher {
            body_not_contains: Some("ZEPHYR-9".into()),
            ..Default::default()
        };
        assert!(matches_request(&matcher, &input, r#"{"history":[]}"#, 0));
        assert!(!matches_request(&matcher, &input, r#"{"history":["ZEPHYR-9"]}"#, 0));
    }

    #[test]
    fn body_contains_and_not_contains_are_both_required() {
        let input = user_message("hi");
        let matcher = Matcher {
            body_contains: Some("summary".into()),
            body_not_contains: Some("raw-turn".into()),
            ..Default::default()
        };
        assert!(matches_request(&matcher, &input, r#"{"a":"summary"}"#, 0));
        // Present-and-forbidden fails even though the positive needle matched.
        assert!(!matches_request(
            &matcher,
            &input,
            r#"{"a":"summary","b":"raw-turn"}"#,
            0
        ));
        // Absent-and-required fails even though nothing forbidden is present.
        assert!(!matches_request(&matcher, &input, r#"{"a":"none"}"#, 0));
    }

    #[test]
    fn missing_bearer_token_is_rejected() {
        let rejection = check_auth(&Config::default(), &context(None));
        assert!(matches!(
            rejection,
            Err(GenerateAssistantResponseError::AccessDeniedException(_))
        ));
    }

    /// The prefix is the part that differs between the two services KAS reaches,
    /// so the operation must be read without it.
    #[test]
    fn target_operation_ignores_the_service_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-target",
            "KiroControlPlaneBearerService.ListAvailableModels".parse().unwrap(),
        );
        assert_eq!(target_operation(&headers).as_deref(), Some(LIST_AVAILABLE_MODELS));

        headers.insert(
            "x-amz-target",
            "AmazonKiroRuntimeService.GenerateAssistantResponse".parse().unwrap(),
        );
        assert_eq!(target_operation(&headers).as_deref(), Some(GENERATE_ASSISTANT_RESPONSE));
    }

    #[test]
    fn absent_or_empty_target_is_not_an_operation() {
        assert_eq!(target_operation(&HeaderMap::new()), None);

        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "Service.".parse().unwrap());
        assert_eq!(target_operation(&headers), None);
    }

    /// KAS drops a model without an id and reads `defaultModel.modelId`, so an
    /// answer missing either leaves it with an empty registry and no way to say
    /// why.
    #[test]
    fn listed_models_carry_an_id_and_a_default() {
        let body = list_available_models_body();
        let models = body["models"].as_array().expect("models is an array");
        assert!(!models.is_empty(), "at least one model, or the registry is empty");
        for model in models {
            assert!(
                model["modelId"].as_str().is_some_and(|id| !id.is_empty()),
                "every model needs an id: {model}"
            );
        }
        assert_eq!(body["defaultModel"]["modelId"], models[0]["modelId"]);
    }

    /// One rule, both services: a token good enough for KRS must be good enough
    /// for the control plane, or the fixture fails for a reason of its own making.
    #[test]
    fn both_services_share_the_bearer_rules() {
        let config = Config {
            api_key: Some("expected".into()),
            ..Config::default()
        };

        assert!(check_bearer(&config, Some("expected")).is_ok());
        assert!(check_bearer(&config, Some("wrong")).is_err());
        assert!(check_bearer(&config, None).is_err());
        assert!(check_bearer(&config, Some("   ")).is_err());

        // The KRS path is the same rules, mapped onto its modeled error.
        assert!(check_auth(&config, &context(Some("expected"))).is_ok());
        assert!(check_auth(&config, &context(Some("wrong"))).is_err());
    }

    #[test]
    fn configured_api_key_must_match() {
        let config = Config {
            api_key: Some("expected".into()),
            ..Default::default()
        };
        assert!(check_auth(&config, &context(Some("wrong"))).is_err());
        assert!(check_auth(&config, &context(Some("expected"))).is_ok());
    }

    #[test]
    fn bearer_prefix_is_stripped() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token-value".parse().unwrap());
        assert_eq!(bearer_token(&headers).as_deref(), Some("token-value"));
    }

    #[test]
    fn captured_headers_never_include_the_token_value() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer super-secret".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        let kept = interesting_headers(&headers);
        assert!(
            kept.iter()
                .any(|(name, value)| name == "authorization" && value == "<redacted>")
        );
        assert!(!kept.iter().any(|(_, value)| value.contains("super-secret")));
    }

    #[test]
    fn a_healthy_stream_is_closed_with_a_stop_reason() {
        let respond = Respond {
            events: vec![scenario::EventScript::Text {
                content: "hi".into(),
                model_id: None,
                model_tag: None,
            }],
            ..Default::default()
        };
        let steps = plan_stream(&respond).expect("plans");
        let last = steps.last().expect("at least one step");
        assert!(matches!(last, Step::Event(event) if wire::carries_stop_reason(event)));
    }

    #[test]
    fn truncate_leaves_the_turn_unterminated() {
        let respond = Respond {
            events: vec![scenario::EventScript::Text {
                content: "hi".into(),
                model_id: None,
                model_tag: None,
            }],
            truncate: true,
            ..Default::default()
        };
        let steps = plan_stream(&respond).expect("plans");
        assert!(
            !steps
                .iter()
                .any(|step| matches!(step, Step::Event(event) if wire::carries_stop_reason(event)))
        );
    }

    #[test]
    fn a_scripted_stream_error_replaces_the_stop_reason() {
        let respond = Respond {
            stream_error: Some(scenario::StreamError {
                kind: scenario::StreamErrorKind::Throttling,
                message: None,
            }),
            ..Default::default()
        };
        let steps = plan_stream(&respond).expect("plans");
        assert!(matches!(steps.last(), Some(Step::Fail(_))));
    }
}
