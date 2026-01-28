---
name: mock-mcp-server
description: Mock MCP server for testing ACP MCP integration. Use when writing integration tests that need to spawn mock MCP servers with configurable tools and responses.
---

# Mock MCP Server

A mock MCP (Model Context Protocol) server for testing ACP MCP integration in Kiro CLI. Supports both stdio and HTTP transports with configurable tool definitions and mock responses.

## Quick Start

### For Stdio Transport (Agent-Spawned)

Create a JSONL config file with tool definitions and responses:

```jsonl
{"type": "tool", "name": "echo", "description": "Echoes input", "input_schema": {"type": "object", "properties": {"message": {"type": "string"}}}}
{"type": "response", "tool": "echo", "response": {"echoed": "hello world"}}
```

Run the server:
```bash
mock-mcp-server --config config.jsonl --transport stdio
```

### For HTTP Transport (Test-Managed)

Use the builder API in your integration tests:

```rust
use mock_mcp_server::{MockMcpServerBuilder, ToolDef, MockResponse};

#[test]
fn test_with_http_mcp_server() {
    // Build and spawn HTTP server (port auto-assigned)
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "greet".to_string(),
            description: "Greets someone".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"name": {"type": "string"}}
            }),
        })
        .add_response(MockResponse {
            tool: "greet".to_string(),
            input_match: None,
            response: serde_json::json!({"greeting": "Hello!"}),
        })
        .spawn_http()
        .unwrap();
    
    // Use in tests
    let url = handle.url(); // "http://127.0.0.1:{auto_port}/mcp"
    let port = handle.port(); // Automatically assigned port
    
    // Server automatically killed when handle dropped
}
```

## Config File Format

JSONL file with two entry types:

**Tool Definition:**
```json
{"type": "tool", "name": "tool_name", "description": "Tool description", "input_schema": {...}}
```

**Mock Response:**
```json
{"type": "response", "tool": "tool_name", "response": {...}}
```

Comments (lines starting with `//`) are ignored.

### Mock Response Behavior

When a tool is called:
- The server looks for responses with `input_match` that matches the tool arguments
- If a matching response is found, it's returned
- If no match, falls back to a response without `input_match` (default response)
- If no response is configured at all, returns a `METHOD_NOT_FOUND` error
- The response object is serialized to pretty-printed JSON and returned in the tool result's text content

**Input Matching Rules:**
- `input_match` is an object with key-value pairs to match against tool arguments
- All fields in `input_match` must be present and equal in the arguments for a match
- Responses are checked in order; first match wins
- Responses without `input_match` serve as defaults

Example with input matching:
```jsonl
{"type": "tool", "name": "greet", "description": "Greets someone"}
{"type": "response", "tool": "greet", "input_match": {"name": "Alice"}, "response": {"greeting": "Hello Alice!"}}
{"type": "response", "tool": "greet", "input_match": {"name": "Bob"}, "response": {"greeting": "Hey Bob!"}}
{"type": "response", "tool": "greet", "response": {"greeting": "Hello stranger!"}}
```

- Calling `greet` with `{"name": "Alice"}` returns `{"greeting": "Hello Alice!"}`
- Calling `greet` with `{"name": "Bob"}` returns `{"greeting": "Hey Bob!"}`
- Calling `greet` with `{"name": "Charlie"}` returns `{"greeting": "Hello stranger!"}` (default)
- Calling `greet` with no arguments returns `{"greeting": "Hello stranger!"}` (default)

## Library API

### `MockMcpServerBuilder`

Builder pattern for creating and spawning HTTP mock MCP servers.

**Methods:**
- `new()` - Create a new builder
- `add_tool(tool)` - Add a tool definition (chainable)
- `add_response(response)` - Add a mock response (chainable)
- `spawn_http()` - Spawn HTTP server on auto-assigned port, returns `MockMcpServerHandle`

### `MockMcpServerHandle`

Handle to a running HTTP server process.

**Methods:**
- `url()` - Get HTTP URL (`http://127.0.0.1:{port}/mcp`)
- `port()` - Get the auto-assigned port number
- `is_running()` - Check if server is alive
- `strong_count()` - Get handle reference count

**Lifecycle:**
- Handle is `Clone`
- Server killed when last handle dropped
- Port automatically assigned to avoid conflicts

## Binary Options

```bash
mock-mcp-server --config <path> [--transport stdio|http] [--port <port>]
```

- `--config, -c` - Path to JSONL config file (required)
- `--transport, -t` - Transport type: `stdio` or `http` (default: `stdio`)
- `--port, -p` - Port for HTTP transport (default: `8080`)

## Testing

The crate includes comprehensive tests:

```bash
cargo test -p mock-mcp-server
```

- 3 lib unit tests (builder pattern, cloning, auto-port)
- 5 main unit tests (config parsing, response lookup, input matching)
- 4 stdio integration tests (MCP protocol: list_tools, call_tool, error handling, shutdown)
- 1 HTTP handle test (lifecycle management, auto-port assignment)
- 1 doc test

Note: HTTP tests verify handle lifecycle only. MCP protocol testing uses stdio transport since that's what ACP integration uses.
