//! JSON-RPC proxy from the kiro-mcp stdio server to the SigV4-signed
//! Taskei MCP gateway.
//!
//! The remote gateway at
//! `https://iad.prod.service.mcp.taskei.amazon.dev/mcp` is itself an MCP
//! server. So everything in this module is *just* a JSON-RPC envelope
//! ferrying `tools/list` and `tools/call` requests over a SigV4-signed
//! HTTPS POST and translating the response back into the rmcp model
//! types our local [`crate::families::taskei::TaskeiServer`] returns to
//! upstream clients.
//!
//! Read/write boundary: the public entry points split deliberately.
//!
//! - [`list_tools_remote`] takes a [`crate::sts_bridge::ReadOnlyView`] so any caller in
//!   `families/<x>/read/` can use it without acquiring write capability.
//! - [`call_read_tool_remote`] also takes a `ReadOnlyView`. It signs with whatever credentials the
//!   read provider hands out (cached read-role or base task-role creds in the no-op case).
//! - [`call_write_tool_remote`] takes the full [`crate::sts_bridge::StsBridge`] so it can call
//!   [`crate::sts_bridge::StsBridge::assume_write_once`] per invocation, drop the per-call
//!   provider, and sign exactly one call with the resulting credentials. Phase 1c's compile-fail
//!   trybuild fixture (`tests/compile_fail/read_view_calls_assume_write.rs`) makes "read code calls
//!   write helper" a build error.
//!
//! Retry shape (plan §319): read-shaped calls use exponential backoff,
//! max 3 attempts on 5xx / 429. Write-shaped calls do not retry until
//! the Phase 4b idempotency layer exists; a replayed create/comment can
//! duplicate user-visible Taskei state. 4xx other than 429 surfaces
//! immediately — these are operator-fixable and silent retry burns the
//! gateway's budget against a problem the user can't see.

use std::time::Duration;

use rmcp::ErrorData;
use rmcp::model::{
    CallToolResult,
    Tool,
};
use serde::Deserialize;
use serde_json::{
    Value,
    json,
};
use tracing::{
    debug,
    warn,
};

use crate::sigv4_client::{
    SigV4Error,
    SigV4HttpClient,
};
use crate::sts_bridge::{
    ReadOnlyView,
    StsBridge,
    one_shot_provider,
};

/// MCP protocol version negotiated with the gateway. Phase 0's smoke
/// confirmed the gateway responds with `2025-06-18`; pinning this in
/// outbound requests prevents the gateway from downgrading us to an
/// older shape that might omit annotations or drop a field the
/// schema-pin asserts on.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Maximum attempts on transient failures (5xx / 429). Plan §319.
const MAX_ATTEMPTS: u32 = 3;

/// Initial backoff between attempts. Doubles each retry. We don't add
/// jitter at this scale — the bot makes O(1) tools/call per Slack
/// message, not enough concurrent traffic to need it.
const INITIAL_BACKOFF: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryPolicy {
    /// Retry transient 429 / 5xx responses up to [`MAX_ATTEMPTS`].
    RetryTransient,
    /// Send exactly one signed request. Used for non-idempotent writes
    /// so a transient response cannot create duplicate Taskei state.
    NoRetry,
}

/// JSON-RPC request id sequence. We don't actually correlate
/// request/response per call (the SigV4 client awaits the response
/// inline), so a counter purely for log readability is fine.
fn next_id() -> u64 {
    use std::sync::atomic::{
        AtomicU64,
        Ordering,
    };
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[serde(default)]
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

/// Send `initialize` to the gateway and return the protocol version it
/// negotiated. Used by the schema-pin assertion so its
/// `protocolVersion` row reflects the live gateway, not the bundled
/// fixture.
pub async fn initialize_remote(view: &ReadOnlyView, endpoint: &str, region: &str) -> Result<String, ErrorData> {
    let creds = view
        .read_credentials_provider()
        .map_err(|e| ErrorData::internal_error(format!("read provider: {e}"), None))?;
    let client = SigV4HttpClient::with_provider(creds, region.to_string())
        .map_err(|e| ErrorData::internal_error(format!("sigv4 client build: {e}"), None))?;

    let body = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "kiro-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }
    });

    let response = post_jsonrpc_with_policy(&client, endpoint, body, RetryPolicy::RetryTransient).await?;
    let result = unwrap_jsonrpc(response)?;
    result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            ErrorData::internal_error(format!("initialize response missing protocolVersion: {result}"), None)
        })
}

/// Send `tools/list` to the gateway and return the parsed
/// `result.tools` array. Used by both the runtime
/// [`crate::families::taskei::TaskeiServer::list_tools`] handler (when
/// it falls back to a remote query — though Phase 1d serves a curated
/// catalog instead) and the Phase 1d schema-pin assertion.
///
/// Returns a `Vec<Tool>` because the gateway's tool catalog is
/// MCP-shaped end-to-end. If the gateway returns a tool whose schema
/// doesn't deserialize cleanly into [`Tool`], we surface that as an
/// `ErrorData::internal_error` rather than dropping it silently — a
/// silent drop would let the schema-pin assertion pass against an
/// incomplete view of the catalog.
pub async fn list_tools_remote(view: &ReadOnlyView, endpoint: &str, region: &str) -> Result<Vec<Tool>, ErrorData> {
    let creds = view
        .read_credentials_provider()
        .map_err(|e| ErrorData::internal_error(format!("read provider: {e}"), None))?;
    let client = SigV4HttpClient::with_provider(creds, region.to_string())
        .map_err(|e| ErrorData::internal_error(format!("sigv4 client build: {e}"), None))?;

    let body = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "tools/list",
        "params": {},
    });

    let response = post_jsonrpc_with_policy(&client, endpoint, body, RetryPolicy::RetryTransient).await?;
    let result = unwrap_jsonrpc(response)?;

    // The gateway returns `{ "tools": [Tool, Tool, ...] }`. Either deserialize
    // the whole object or pluck the array out. Plucking is safer because future
    // gateway additions (a `nextCursor` for pagination, etc.) won't break us.
    let tools_value = result
        .get("tools")
        .ok_or_else(|| ErrorData::internal_error("gateway tools/list response missing `tools`".to_string(), None))?
        .clone();
    let tools: Vec<Tool> = serde_json::from_value(tools_value)
        .map_err(|e| ErrorData::internal_error(format!("decode tools/list: {e}"), None))?;
    Ok(tools)
}

/// Forward a `tools/call` to the gateway using read-role credentials.
/// The kiro-mcp shim's [`crate::families::taskei::TaskeiServer`]
/// dispatches read tools (`Taskei___list_tasks`, `_get_task`,
/// `_get_room`, `_list_room_resource`, plus the
/// `x_amz_bedrock_agentcore_search` helper) through this entry point.
pub async fn call_read_tool_remote(
    view: &ReadOnlyView,
    endpoint: &str,
    region: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<CallToolResult, ErrorData> {
    let creds = view
        .read_credentials_provider()
        .map_err(|e| ErrorData::internal_error(format!("read provider: {e}"), None))?;
    let client = SigV4HttpClient::with_provider(creds, region.to_string())
        .map_err(|e| ErrorData::internal_error(format!("sigv4 client build: {e}"), None))?;
    call_tool_signed(
        &client,
        endpoint,
        tool_name,
        arguments,
        "read",
        RetryPolicy::RetryTransient,
    )
    .await
}

/// Forward a `tools/call` to the gateway using *one-shot* write-role
/// credentials. Builds a fresh STS AssumeRole snapshot, signs exactly
/// one call with it, and drops the snapshot before returning.
///
/// Phase-0 reality (both ARNs unset): `assume_write_once` falls through
/// to base-creds resolution and the call signs with the kiro-bot ECS
/// task role, same as the read path. The audit log records `mode=base`
/// in that case so an operator can see the bridge wasn't actually
/// hopping STS for this call.
pub async fn call_write_tool_remote(
    bridge: &StsBridge,
    endpoint: &str,
    region: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<CallToolResult, ErrorData> {
    let wc = bridge
        .assume_write_once()
        .await
        .map_err(|e| ErrorData::internal_error(format!("assume write: {e}"), None))?;
    let client = SigV4HttpClient::with_provider(one_shot_provider(wc.creds), region.to_string())
        .map_err(|e| ErrorData::internal_error(format!("sigv4 client build: {e}"), None))?;
    call_tool_signed(
        &client,
        endpoint,
        tool_name,
        arguments,
        wc.mode.as_str(),
        RetryPolicy::NoRetry,
    )
    .await
}

async fn call_tool_signed(
    client: &SigV4HttpClient,
    endpoint: &str,
    tool_name: &str,
    arguments: &Value,
    audit_mode: &str,
    retry_policy: RetryPolicy,
) -> Result<CallToolResult, ErrorData> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments,
        },
    });

    debug!(
        target: "kiro_mcp",
        tool_name,
        audit_mode,
        retry_policy = ?retry_policy,
        "forwarding tools/call to taskei gateway"
    );
    let response = post_jsonrpc_with_policy(client, endpoint, body, retry_policy).await?;
    let result = unwrap_jsonrpc(response)?;

    // The gateway is itself an MCP server, so the `result` payload IS
    // already a CallToolResult. Deserialize directly. If the gateway
    // ever gains a field rmcp doesn't know about, serde will silently
    // ignore it (#[serde(deny_unknown_fields)] is NOT set on
    // CallToolResult in rmcp 0.17), and we surface what rmcp does
    // understand.
    let call_result: CallToolResult = serde_json::from_value(result.clone())
        .map_err(|e| ErrorData::internal_error(format!("decode tools/call result: {e} body={result}"), None))?;
    Ok(call_result)
}

/// Send a signed POST with bounded retry on transient failures when
/// the caller's policy allows replay.
///
/// Retry policy (plan §319):
/// - read/list/initialize 5xx / 429: retry up to MAX_ATTEMPTS with exponential backoff.
/// - write 5xx / 429: surface immediately. Until Phase 4b's idempotency key exists, replay can
///   duplicate tasks or comments.
/// - other 4xx: surface immediately. The user / operator must fix it.
/// - signing / credential / network errors: surface immediately. Phase 1b's `SigV4HttpClient`
///   already audit-logs the cred-failure path; double-retrying would just generate noise.
async fn post_jsonrpc_with_policy(
    client: &SigV4HttpClient,
    endpoint: &str,
    body: Value,
    retry_policy: RetryPolicy,
) -> Result<Value, ErrorData> {
    let mut attempt = 0u32;
    let mut backoff = INITIAL_BACKOFF;
    loop {
        attempt += 1;
        let response = match client.post_json(endpoint, body.clone()).await {
            Ok(resp) => resp,
            Err(SigV4Error::CredentialFailure(msg)) => {
                // Cred failures surface as ErrorData::internal_error; the
                // process-level fail-closed exit is owned by the caller's
                // loop, not by a per-call retry. Audit line was already
                // emitted by SigV4HttpClient.
                return Err(ErrorData::internal_error(format!("credentials: {msg}"), None));
            },
            Err(e) => {
                return Err(ErrorData::internal_error(format!("transport: {e}"), None));
            },
        };

        let status = response.status();
        let retryable =
            retry_policy == RetryPolicy::RetryTransient && (status.as_u16() == 429 || status.is_server_error());
        let body_text = response.text().await.unwrap_or_default();

        if status.is_success() {
            return serde_json::from_str(&body_text)
                .map_err(|e| ErrorData::internal_error(format!("decode jsonrpc: {e} body={body_text}"), None));
        }

        if retryable && attempt < MAX_ATTEMPTS {
            warn!(
                target: "kiro_mcp",
                status = %status,
                attempt,
                backoff_ms = backoff.as_millis() as u64,
                "transient gateway error; retrying"
            );
            tokio::time::sleep(backoff).await;
            backoff = backoff.saturating_mul(2);
            continue;
        }

        // Either non-retryable by status/policy or retries exhausted.
        return Err(ErrorData::internal_error(
            format!("gateway returned {status}: {body_text}"),
            None,
        ));
    }
}

/// Pull `result` out of a JSON-RPC envelope, surfacing `error` as an
/// rmcp protocol error.
fn unwrap_jsonrpc(envelope: Value) -> Result<Value, ErrorData> {
    let parsed: JsonRpcResponse = serde_json::from_value(envelope.clone())
        .map_err(|e| ErrorData::internal_error(format!("decode jsonrpc envelope: {e} body={envelope}"), None))?;
    if let Some(err) = parsed.error {
        return Err(ErrorData::internal_error(
            format!("gateway error code={} message={}", err.code, err.message),
            err.data,
        ));
    }
    parsed
        .result
        .ok_or_else(|| ErrorData::internal_error("gateway response missing both result and error".to_string(), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwrap_jsonrpc_surfaces_error_envelope() {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32601, "message": "Method not found" }
        });
        let err = unwrap_jsonrpc(body).expect_err("error envelope must surface");
        assert!(err.message.contains("Method not found"), "got: {err:?}");
        assert!(err.message.contains("-32601"), "got: {err:?}");
    }

    #[test]
    fn unwrap_jsonrpc_returns_result_value() {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "tools": [] }
        });
        let result = unwrap_jsonrpc(body).expect("result envelope must unwrap");
        assert_eq!(result, json!({ "tools": [] }));
    }

    #[test]
    fn unwrap_jsonrpc_rejects_envelope_with_neither_result_nor_error() {
        let body = json!({ "jsonrpc": "2.0", "id": 1 });
        let err = unwrap_jsonrpc(body).expect_err("malformed envelope must error");
        assert!(err.message.contains("missing both"), "got: {err:?}");
    }
}
