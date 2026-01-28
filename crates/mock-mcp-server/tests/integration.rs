//! Integration tests for mock MCP server

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

/// Helper to send JSON-RPC request and read response
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
    writeln!(stdin, "{}", request_str).unwrap();
    stdin.flush().unwrap();

    let mut response_line = String::new();
    stdout.read_line(&mut response_line).unwrap();

    serde_json::from_str(&response_line).unwrap()
}

#[test]
fn test_stdio_list_tools() {
    // Create temp config file
    let config_content = r#"{"type": "tool", "name": "echo", "description": "Echoes input", "input_schema": {"type": "object", "properties": {"message": {"type": "string"}}}}
{"type": "tool", "name": "add", "description": "Adds two numbers", "input_schema": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}}}
{"type": "response", "tool": "echo", "response": {"echoed": "test"}}
{"type": "response", "tool": "add", "response": {"result": 42}}"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.jsonl");
    std::fs::write(&config_path, config_content).unwrap();

    // Start the mock server
    let mut child = Command::new(env!("CARGO_BIN_EXE_mock-mcp-server"))
        .args(["--config", config_path.to_str().unwrap(), "--transport", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start mock-mcp-server");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize
    let init_response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        1,
    );
    assert!(
        init_response.get("result").is_some(),
        "Initialize failed: {:?}",
        init_response
    );

    // Send initialized notification
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", serde_json::to_string(&notif).unwrap()).unwrap();
    stdin.flush().unwrap();

    // List tools
    let tools_response = send_jsonrpc(&mut stdin, &mut stdout, "tools/list", serde_json::json!({}), 2);

    let result = tools_response.get("result").expect("tools/list failed");
    let tools = result.get("tools").expect("No tools in response");
    let tools_array = tools.as_array().unwrap();

    assert_eq!(tools_array.len(), 2);

    let tool_names: Vec<&str> = tools_array
        .iter()
        .map(|t| t.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"echo"));
    assert!(tool_names.contains(&"add"));

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(1));
    let _ = child.kill();
}

#[test]
fn test_stdio_call_tool() {
    let config_content = r#"{"type": "tool", "name": "greet", "description": "Greets someone"}
{"type": "response", "tool": "greet", "response": {"greeting": "Hello, World!"}}"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.jsonl");
    std::fs::write(&config_path, config_content).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mock-mcp-server"))
        .args(["--config", config_path.to_str().unwrap(), "--transport", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start mock-mcp-server");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize
    let _ = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        1,
    );

    // Send initialized notification
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", serde_json::to_string(&notif).unwrap()).unwrap();
    stdin.flush().unwrap();

    // Call tool
    let call_response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "tools/call",
        serde_json::json!({
            "name": "greet",
            "arguments": {"name": "Test"}
        }),
        2,
    );

    let result = call_response.get("result").expect("tools/call failed");
    let content = result.get("content").expect("No content in response");
    let content_array = content.as_array().unwrap();
    assert!(!content_array.is_empty());

    let text = content_array[0].get("text").unwrap().as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["greeting"], "Hello, World!");

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(1));
    let _ = child.kill();
}

#[test]
fn test_stdio_tool_not_found() {
    let config_content = r#"{"type": "tool", "name": "existing", "description": "Exists"}"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.jsonl");
    std::fs::write(&config_path, config_content).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mock-mcp-server"))
        .args(["--config", config_path.to_str().unwrap(), "--transport", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start mock-mcp-server");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize
    let _ = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        1,
    );

    // Send initialized notification
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", serde_json::to_string(&notif).unwrap()).unwrap();
    stdin.flush().unwrap();

    // Call tool that has no response configured
    let call_response = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "tools/call",
        serde_json::json!({
            "name": "existing",
            "arguments": {}
        }),
        2,
    );

    // Should get an error since no response is configured
    let error = call_response.get("error").expect("Expected error response");
    assert!(error.get("message").is_some());

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(1));
    let _ = child.kill();
}

#[test]
fn test_server_exits_on_stdin_close() {
    use std::time::Instant;

    let config_content = r#"{"type": "tool", "name": "test", "description": "Test"}"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.jsonl");
    std::fs::write(&config_path, config_content).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mock-mcp-server"))
        .args(["--config", config_path.to_str().unwrap(), "--transport", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start mock-mcp-server");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize the server
    let _ = send_jsonrpc(
        &mut stdin,
        &mut stdout,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        1,
    );

    // Close stdin - this should cause the server to exit
    drop(stdin);
    drop(stdout);

    // Server should exit within a reasonable time
    let start = Instant::now();
    let status = child.wait_timeout(Duration::from_secs(5)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        status.is_some(),
        "Server should exit when stdin is closed, but it didn't exit within 5 seconds"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "Server took too long to exit: {:?}",
        elapsed
    );
}

#[test]
fn test_http_server_handle() {
    use mock_mcp_server::{
        MockMcpServerBuilder,
        MockResponse,
        ToolDef,
    };

    // Build and spawn HTTP server
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "greet".to_string(),
            description: "Greets someone".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"name": {"type": "string"}}}),
        })
        .add_response(MockResponse {
            tool: "greet".to_string(),
            input_match: None,
            response: serde_json::json!({"greeting": "Hello!"}),
        })
        .spawn_http()
        .unwrap();

    // Give server time to start
    std::thread::sleep(Duration::from_millis(500));

    assert!(handle.is_running());
    assert!(handle.url().starts_with("http://127.0.0.1:"));
    assert!(handle.port() > 0);

    // Clone handle and verify strong count
    let handle2 = handle.clone();
    assert_eq!(handle.strong_count(), 2);

    // Drop clone
    drop(handle2);
    assert_eq!(handle.strong_count(), 1);

    // Server should still be running
    assert!(handle.is_running());

    // Drop last handle - server should be killed
    drop(handle);
}
