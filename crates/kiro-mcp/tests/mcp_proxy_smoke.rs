//! End-to-end smoke for the Phase 1d JSON-RPC proxy: stand up a tiny
//! hyper server on a loopback port, point [`call_read_tool_remote`]
//! and [`list_tools_remote`] at it, and assert that:
//!
//! 1. The wire body is a properly-shaped JSON-RPC request,
//! 2. The response decodes cleanly into the rmcp model types,
//! 3. JSON-RPC `error` envelopes surface as `ErrorData::internal_error`,
//! 4. Bounded retry on 5xx works for read-shaped calls (reach success on attempt 2, then stop).
//! 5. Write-shaped calls do not replay transient failures before the idempotency layer exists.
//!
//! Mirrors the structure of `sigv4_smoke.rs` from Phase 1b — same tiny
//! hyper-on-loopback setup, just exercising the proxy layer instead.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::{
    Duration,
    SystemTime,
};

use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use http_body_util::{
    BodyExt,
    Full,
};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use kiro_mcp::families::taskei::mcp_proxy;
use kiro_mcp::sigv4_client::SigV4HttpClient;
use kiro_mcp::sts_bridge::{
    StsBridge,
    StsBridgeConfig,
};
use serde_json::{
    Value,
    json,
};
use tokio::net::TcpListener;

/// Records each request body the mock server receives, in order.
#[derive(Default, Clone)]
struct CapturedBodies {
    inner: Arc<Mutex<Vec<Value>>>,
}

impl CapturedBodies {
    fn snapshot(&self) -> Vec<Value> {
        self.inner.lock().unwrap().clone()
    }
}

#[derive(Clone)]
enum Behavior {
    /// Always return this value (under JSON-RPC `result`).
    AlwaysOk(Value),
    /// Return this JSON-RPC `error` envelope.
    AlwaysError { code: i64, message: String },
    /// Return 5xx for the first N attempts, then succeed with the
    /// supplied result. Used to exercise retry logic.
    FailUntilAttempt { fail_count: usize, success: Value },
}

#[derive(Clone)]
struct MockState {
    behavior: Behavior,
    captured: CapturedBodies,
    attempt: Arc<Mutex<usize>>,
}

async fn start_mock(behavior: Behavior) -> (SocketAddr, CapturedBodies) {
    let captured = CapturedBodies::default();
    let state = MockState {
        behavior,
        captured: captured.clone(),
        attempt: Arc::new(Mutex::new(0)),
    };

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            let state = state.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let state = state.clone();
                    async move {
                        let bytes = req.into_body().collect().await.unwrap().to_bytes();
                        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                        state.captured.inner.lock().unwrap().push(body.clone());
                        let mut attempt = state.attempt.lock().unwrap();
                        *attempt += 1;
                        let n = *attempt;
                        drop(attempt);

                        let id = body.get("id").cloned().unwrap_or(json!(1));
                        let response: Value = match &state.behavior {
                            Behavior::AlwaysOk(result) => json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": result,
                            }),
                            Behavior::AlwaysError { code, message } => json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": *code, "message": message }
                            }),
                            Behavior::FailUntilAttempt { fail_count, success } => {
                                if n <= *fail_count {
                                    return Ok::<_, Infallible>(
                                        Response::builder()
                                            .status(503)
                                            .body(Full::new(Bytes::from_static(b"backend unavailable")))
                                            .unwrap(),
                                    );
                                }
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": success,
                                })
                            },
                        };
                        let body = serde_json::to_vec(&response).unwrap();
                        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(body))))
                    }
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });

    (addr, captured)
}

async fn dummy_bridge() -> StsBridge {
    let creds = Credentials::new(
        "AKIDTEST",
        "secret",
        Some("token".into()),
        Some(SystemTime::now() + Duration::from_secs(3600)),
        "test",
    );
    let base = aws_config::SdkConfig::builder()
        .credentials_provider(SharedCredentialsProvider::new(creds))
        .region(aws_config::Region::new("us-east-1"))
        .build();
    StsBridge::from_base(base, StsBridgeConfig::default()).await.unwrap()
}

#[tokio::test]
async fn list_tools_remote_decodes_gateway_response() {
    // Gateway returns one tool with a minimal schema. Plan §296-306
    // pins the gateway as itself an MCP server, so the result.tools[]
    // entries are rmcp Tool struct shape.
    let (addr, captured) = start_mock(Behavior::AlwaysOk(json!({
        "tools": [
            {
                "name": "Taskei___list_tasks",
                "description": "List tasks in a Taskei room.",
                "inputSchema": { "type": "object", "properties": { "roomId": { "type": "string" } }, "required": ["roomId"] }
            }
        ]
    })))
    .await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let tools = mcp_proxy::list_tools_remote(&view, &endpoint, "us-east-1")
        .await
        .expect("tools/list");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name.as_ref(), "Taskei___list_tasks");

    let req = &captured.snapshot()[0];
    assert_eq!(req["jsonrpc"], "2.0");
    assert_eq!(req["method"], "tools/list");
}

#[tokio::test]
async fn initialize_remote_decodes_protocol_version() {
    let (addr, captured) = start_mock(Behavior::AlwaysOk(json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "serverInfo": { "name": "TaskeiMCPService-Prod-us-east-1", "version": "test" }
    })))
    .await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let protocol = mcp_proxy::initialize_remote(&view, &endpoint, "us-west-2")
        .await
        .expect("initialize");
    assert_eq!(protocol, "2025-06-18");

    let req = &captured.snapshot()[0];
    assert_eq!(req["method"], "initialize");
    assert_eq!(req["params"]["protocolVersion"], mcp_proxy::MCP_PROTOCOL_VERSION);
}

#[tokio::test]
async fn call_read_tool_remote_forwards_args_and_decodes_result() {
    // Gateway echoes back an MCP-shaped CallToolResult with structured
    // content. Plan: the gateway IS an MCP server; we deserialize its
    // result into rmcp::model::CallToolResult directly.
    let result_payload = json!({
        "content": [{ "type": "text", "text": "ok" }],
        "structuredContent": { "tasks": [] },
        "isError": false
    });
    let (addr, captured) = start_mock(Behavior::AlwaysOk(result_payload.clone())).await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let args = json!({ "roomId": "room-A", "limit": 10 });
    let result = mcp_proxy::call_read_tool_remote(&view, &endpoint, "us-east-1", "Taskei___list_tasks", &args)
        .await
        .expect("tools/call");
    assert_eq!(result.is_error, Some(false));
    assert!(
        result
            .content
            .iter()
            .any(|c| matches!(c, rmcp::model::ContentBlock::Text(t) if t.text == "ok"))
    );

    let req = &captured.snapshot()[0];
    assert_eq!(req["method"], "tools/call");
    assert_eq!(req["params"]["name"], "Taskei___list_tasks");
    assert_eq!(req["params"]["arguments"]["roomId"], "room-A");
    assert_eq!(req["params"]["arguments"]["limit"], 10);
}

#[tokio::test]
async fn jsonrpc_error_surfaces_as_error_data() {
    let (addr, _captured) = start_mock(Behavior::AlwaysError {
        code: -32602,
        message: "Invalid params: roomId required".into(),
    })
    .await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let err = mcp_proxy::call_read_tool_remote(&view, &endpoint, "us-east-1", "Taskei___list_tasks", &json!({}))
        .await
        .expect_err("expected jsonrpc error to surface");
    assert!(err.message.contains("Invalid params"), "got: {err:?}");
    assert!(err.message.contains("-32602"), "got: {err:?}");
}

#[tokio::test]
async fn retries_on_transient_5xx_then_succeeds() {
    // Two 503s, then success on attempt 3. Plan §319: max 3 attempts.
    let success_payload = json!({
        "content": [{ "type": "text", "text": "after retry" }],
        "isError": false
    });
    let (addr, captured) = start_mock(Behavior::FailUntilAttempt {
        fail_count: 2,
        success: success_payload,
    })
    .await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let result = mcp_proxy::call_read_tool_remote(&view, &endpoint, "us-east-1", "Taskei___list_tasks", &json!({}))
        .await
        .expect("retry should reach success");
    assert_eq!(result.is_error, Some(false));
    // 3 total attempts hit the mock (two 503s + one success).
    assert_eq!(captured.snapshot().len(), 3);
}

#[tokio::test]
async fn retries_exhaust_then_surface_5xx() {
    // 5 failures > MAX_ATTEMPTS, so we should give up after 3 attempts.
    let dummy_success = json!({});
    let (addr, captured) = start_mock(Behavior::FailUntilAttempt {
        fail_count: 5,
        success: dummy_success,
    })
    .await;

    let bridge = dummy_bridge().await;
    let view = bridge.read_only();
    let endpoint = format!("http://{addr}/mcp");

    let err = mcp_proxy::call_read_tool_remote(&view, &endpoint, "us-east-1", "Taskei___list_tasks", &json!({}))
        .await
        .expect_err("retries should exhaust");
    assert!(err.message.contains("503"), "got: {err:?}");
    assert_eq!(captured.snapshot().len(), 3, "should give up after 3 attempts");
}

#[tokio::test]
async fn write_tool_remote_does_not_retry_transient_5xx() {
    // Non-idempotent writes must not be replayed before Phase 4b's
    // idempotency key exists. A successful-but-503 create/comment
    // could otherwise duplicate user-visible Taskei state.
    let success_payload = json!({
        "content": [{ "type": "text", "text": "would only appear on retry" }],
        "isError": false
    });
    let (addr, captured) = start_mock(Behavior::FailUntilAttempt {
        fail_count: 1,
        success: success_payload,
    })
    .await;

    let bridge = dummy_bridge().await;
    let endpoint = format!("http://{addr}/mcp");

    let err = mcp_proxy::call_write_tool_remote(
        &bridge,
        &endpoint,
        "us-east-1",
        "Taskei___create_task",
        &json!({ "task": { "roomId": "room-A", "title": "new task" } }),
    )
    .await
    .expect_err("write calls should surface the first transient response");
    assert!(err.message.contains("503"), "got: {err:?}");

    let requests = captured.snapshot();
    assert_eq!(requests.len(), 1, "write call must not be retried");
    assert_eq!(requests[0]["method"], "tools/call");
    assert_eq!(requests[0]["params"]["name"], "Taskei___create_task");
}

/// Phase 1d's plan §324: `#[ignore]` integration test against the real
/// IAD endpoint. Local developer can run with `--include-ignored` and
/// AWS_PROFILE=kiro-bot.
#[tokio::test]
#[ignore = "hits the live Taskei prod gateway; run with AWS_PROFILE=kiro-bot --include-ignored"]
async fn live_tools_list_against_iad_prod() {
    let bridge = StsBridge::from_default_chain("us-east-1".into(), StsBridgeConfig::default())
        .await
        .expect("default chain");
    let view = bridge.read_only();
    let tools = mcp_proxy::list_tools_remote(&view, "https://iad.prod.service.mcp.taskei.amazon.dev/mcp", "us-east-1")
        .await
        .expect("live tools/list");
    assert_eq!(tools.len(), 7, "phase 0 confirmed 7 tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "Taskei___create_task",
        "Taskei___update_task",
        "Taskei___get_task",
        "Taskei___list_tasks",
        "Taskei___get_room",
        "Taskei___list_room_resource",
        "x_amz_bedrock_agentcore_search",
    ] {
        assert!(
            names.contains(&expected),
            "expected tool {expected} not present; got {names:?}"
        );
    }
}

// SigV4HttpClient lives behind `with_provider` constructor — this
// import keeps the test surface honest about what's reachable.
#[allow(dead_code)]
fn _import_check(_creds: SharedCredentialsProvider) -> SigV4HttpClient {
    SigV4HttpClient::with_provider(_creds, "us-east-1".into()).unwrap()
}
