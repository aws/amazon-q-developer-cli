# ACP MCP Server Integration

**Status:** In Progress
**Started:** 2026-01-14

## Problem Statement

Enable the ACP agent to accept MCP server configurations from clients via `session/new` and `session/load` requests, launch those servers, and expose their tools to the agent. Additionally, create a mock MCP server binary for testing.

## Requirements

1. Standalone mock MCP server binary that reads JSONL files and produces mock tool responses
2. Support stdio and HTTP transports (HTTP already supported in agent crate)
3. Replace agent config MCP servers with ACP-provided ones (warn on name conflicts)
4. Forward OAuth requests to ACP client for HTTP servers
5. Integration tests verifying end-to-end MCP functionality

## Background

- `sacp` crate provides `McpServer` union type (stdio/http/sse variants) in `NewSessionRequest.mcp_servers`
- Agent crate has `McpManager` + `McpServerConfig` (Local/Remote) for launching servers
- `McpServerActorEvent::OauthRequest` emits OAuth URLs for remote servers
- Current `AcpSession` doesn't process `mcp_servers` from requests

## Proposed Solution

1. Create `mock-mcp-server` binary in a new crate
2. Add conversion from ACP `McpServer` types to agent `McpServerConfig`
3. Pass MCP configs through session creation flow to `Agent`
4. Forward `McpServerEvent::OauthRequest` as ACP extension notifications
5. Write integration tests using mock MCP server

## Task Breakdown

### Task 1: Create mock MCP server binary crate
- Create `crates/mock-mcp-server/` with basic Cargo.toml
- Implement stdio MCP server using `rmcp` crate
- Support loading tool definitions from JSONL file (tool name, description, schema)
- Return mock responses based on JSONL response mappings
- Demo: Run `mock-mcp-server --tools tools.jsonl` and call tools via MCP protocol

### Task 2: Add HTTP support to mock MCP server
- Add HTTP transport mode using `rmcp` HTTP server capabilities
- Support `--transport stdio|http` and `--port` flags
- Demo: Run mock server in HTTP mode and connect via HTTP MCP client

### Task 3: Convert ACP McpServer types to agent McpServerConfig
- Create conversion function `acp_mcp_to_agent_config(sacp::schema::McpServer) -> McpServerConfig`
- Handle stdio → `LocalMcpServerConfig` mapping
- Handle http → `RemoteMcpServerConfig` mapping (SSE deprecated, skip for now)
- Add unit tests for conversion
- Demo: Unit tests pass for all transport type conversions

### Task 4: Wire MCP servers through session creation
- Update `AcpSessionConfig` to include `mcp_servers: Vec<(String, McpServerConfig)>`
- Update `NewSessionRequest` handler to extract and convert `mcp_servers`
- Update `LoadSessionRequest` handler similarly
- Pass MCP configs to `AcpSessionBuilder` → `Agent`
- Warn on name conflicts with agent config MCP servers
- Demo: Create session with MCP server config, verify it reaches Agent

### Task 5: Forward OAuth requests to ACP client
- Create ACP extension notification type for OAuth requests (e.g., `_kiro.dev/mcp/oauth_request`)
- Subscribe to `McpServerEvent::OauthRequest` in session manager
- Forward as extension notification to client
- Demo: HTTP MCP server triggers OAuth, client receives notification

### Task 6: Update AgentCapabilities for MCP support
- Set `mcpCapabilities.http = true` in `InitializeResponse`
- Keep `mcpCapabilities.sse = false` (deprecated)
- Demo: Client can verify HTTP MCP support via capabilities

### Task 7: Write integration tests for MCP via ACP
- Create JSONL mock tool definitions for test MCP server
- Write test: `session/new` with stdio MCP server, verify tools available
- Write test: Tool call through MCP server returns expected result
- Write test: HTTP MCP server with OAuth flow
- Demo: All integration tests pass

## Progress Log

### 2026-01-14 - Tasks 1 & 2 Complete
Implemented mock MCP server binary with stdio and HTTP transport support:
- ✅ Created `crates/mock-mcp-server/` with Cargo.toml
- ✅ Implemented stdio MCP server using `rmcp` crate
- ✅ Support loading tool definitions from JSONL file
- ✅ Return mock responses based on JSONL response mappings
- ✅ Added HTTP transport support using `StreamableHttpService`
- ✅ Added `--transport stdio|http` and `--port` flags
- ✅ Verified proper shutdown when stdin is closed
- ✅ Created `lib.rs` with `MockMcpServerHandle` for HTTP server management
  - Cloneable handle with automatic process cleanup on last drop
  - `add_tool()` and `add_response()` for dynamic configuration
  - `spawn()` for deferred server startup
- ✅ Unit tests: 7 passing (4 main.rs + 3 lib.rs)
- ✅ Integration tests: 5 passing (list_tools, call_tool, tool_not_found, shutdown, http_handle)
- ✅ Doc tests: 1 passing

### 2026-01-14 - Task 3 Complete
Implemented ACP McpServer to agent McpServerConfig conversion:
- ✅ Created `crates/chat-cli/src/agent/acp/mcp_conversion.rs` module
- ✅ Implemented `convert_mcp_server()` function handling all transport types
- ✅ Implemented `convert_stdio()` for stdio → `LocalMcpServerConfig`
- ✅ Implemented `convert_http()` for http → `RemoteMcpServerConfig`
- ✅ Implemented `convert_sse()` for sse → `RemoteMcpServerConfig`
- ✅ Unit tests: 8 passing (all transport type conversions verified)

### 2026-01-14 - Task 4 Complete
Wired MCP servers through session creation:
- ✅ Updated `AcpSessionConfig` to include `mcp_servers: Vec<sacp::schema::McpServer>`
- ✅ Updated `NewSessionRequest` handler to extract and convert `mcp_servers`
- ✅ MCP configs passed to `AcpSessionBuilder` → `Agent`
- ✅ Warnings logged on name conflicts with agent config MCP servers

### 2026-01-15 - Task 7 In Progress (HTTP Open Server Path)
Implemented integration test for MCP via ACP with HTTP transport (open server, no auth):
- ✅ Added `wait_ready()` method to `MockMcpServerHandle` for reliable server startup detection
- ✅ Added `find_binary()` to use pre-built binary instead of `cargo run` for faster test execution
- ✅ Created `mcp_tool_call.jsonl` mock response file that triggers MCP tool call
- ✅ Added `new_session_with_mcp()` method to test client for passing MCP server configs
- ✅ Added JSON conversion between `sacp::schema::McpServer` and `acp::McpServer` types
- ✅ Integration test `mcp_server_tool_call_triggers_permission_request` passes
- ✅ Verified end-to-end: MCP server config → agent initialization → tool discovery → tool call → permission request

Key findings:
- The agent sends a probe POST request to HTTP MCP servers to determine if auth is needed
- Mock server returns 415 (Unsupported Media Type) which is treated as "no auth needed"
- This allows the open HTTP server code path to work without implementing OAuth endpoints

### 2026-01-16 - Task 5 Complete
Implemented OAuth request forwarding to ACP client via extension notifications:
- ✅ Added `MCP_OAUTH_REQUEST` constant (`_kiro.dev/mcp/oauth_request`) in `extensions.rs`
- ✅ Added `McpOauthRequestNotification` struct with `session_id`, `server_name`, `oauth_url`
- ✅ Added `SendExtNotification` variant to `SessionManagerRequestData`
- ✅ Added `send_ext_notification()` method to `SessionManagerHandle`
- ✅ Handler sends `AgentNotification::ExtNotification` to client
- ✅ Added handler for `AgentEvent::InitializeUpdate` (MCP OAuth events during init)
- ✅ Added handler for `AgentEvent::Mcp` with `handle_mcp_event()` method
- ✅ Added `probe_status()` to `MockMcpServerBuilder` for configuring probe response
- ✅ Added `--probe-status` CLI arg and OAuth discovery endpoint to mock server
- ✅ Added `ext_notifications` capture to test client
- ✅ Integration test `http_mcp_server_oauth_request_triggers_ext_notification` passes

Key findings:
- OAuth flow requires server to implement `/.well-known/oauth-authorization-server` endpoint
- `agent_client_protocol` strips leading underscore from extension method names when parsing
- MCP events during initialization come through `AgentEvent::InitializeUpdate`, not `AgentEvent::Mcp`
