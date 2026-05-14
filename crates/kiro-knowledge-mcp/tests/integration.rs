//! End-to-end test: spawn the kiro-knowledge-mcp binary in --stub mode,
//! send tools/list + tools/call requests, parse responses.
//!
//! These tests exercise the full MCP wire format without requiring AWS.

use std::io::{
    BufRead,
    BufReader,
    Write,
};
use std::process::{
    Command,
    Stdio,
};
use std::time::Duration;

use wait_timeout::ChildExt;

fn cargo_bin() -> String {
    env!("CARGO_BIN_EXE_kiro-knowledge-mcp").to_string()
}

/// Send a JSON-RPC request and read the matching response.
fn send_jsonrpc(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    method: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });

    let request_str = serde_json::to_string(&request).unwrap();
    writeln!(stdin, "{request_str}").unwrap();
    stdin.flush().unwrap();

    let mut response_line = String::new();
    stdout.read_line(&mut response_line).unwrap();

    serde_json::from_str(&response_line)
        .unwrap_or_else(|e| panic!("parse JSON-RPC response: {e}\nline: {response_line}"))
}

/// Send the `notifications/initialized` notification (no response expected).
fn send_initialized_notification(stdin: &mut std::process::ChildStdin) {
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", serde_json::to_string(&notif).unwrap()).unwrap();
    stdin.flush().unwrap();
}

#[test]
fn lists_search_kiro_knowledge_tool() {
    let mut child = Command::new(cargo_bin())
        .args(["--stub"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize handshake.
    let init_response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }),
        1,
    );
    assert!(
        init_response.get("result").is_some(),
        "initialize failed: {init_response:?}"
    );

    send_initialized_notification(&mut stdin);

    // tools/list
    let tools_response = send_jsonrpc(&mut stdin, &mut stdout, "tools/list", serde_json::json!({}), 2);
    let result = tools_response
        .get("result")
        .unwrap_or_else(|| panic!("tools/list missing result: {tools_response:?}"));
    let tools = result.get("tools").and_then(|t| t.as_array()).expect("tools array");
    assert!(
        tools
            .iter()
            .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("search_kiro_knowledge")),
        "search_kiro_knowledge not in tools list: {tools:?}"
    );

    // Close stdin so the server exits cleanly.
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();
}

#[test]
fn calls_search_kiro_knowledge_returns_chunks() {
    let mut child = Command::new(cargo_bin())
        .args(["--stub"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // initialize
    let _ = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }),
        1,
    );
    send_initialized_notification(&mut stdin);

    // tools/call
    let call_response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "tools/call",
        serde_json::json!({
            "name": "search_kiro_knowledge",
            "arguments": { "query": "test" }
        }),
        3,
    );

    let result = call_response
        .get("result")
        .unwrap_or_else(|| panic!("tools/call missing result: {call_response:?}"));
    let content = result.get("content").and_then(|c| c.as_array()).expect("content array");
    let text = content
        .first()
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .expect("text content");
    assert!(text.contains("docs/stub.md"), "text: {text}");
    assert!(text.contains("Stub response"), "text: {text}");

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();
}

#[test]
fn rejects_unknown_tool_with_invalid_params_error() {
    let mut child = Command::new(cargo_bin())
        .args(["--stub"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }),
        1,
    );
    send_initialized_notification(&mut stdin);

    // tools/call with a bogus tool name.
    let response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "tools/call",
        serde_json::json!({
            "name": "definitely_not_a_real_tool",
            "arguments": { "query": "x" }
        }),
        2,
    );

    let error = response
        .get("error")
        .unwrap_or_else(|| panic!("expected JSON-RPC error, got: {response:?}"));
    let code = error.get("code").and_then(|c| c.as_i64()).expect("error code");
    assert_eq!(code, -32602, "expected invalid_params (-32602), got: {error:?}");
    let message = error.get("message").and_then(|m| m.as_str()).expect("error message");
    assert!(
        message.contains("definitely_not_a_real_tool"),
        "error message should mention the unknown tool: {message}"
    );

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();
}
