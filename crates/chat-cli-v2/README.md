# Kiro CLI V2 — ACP-Based Chat Backend

This crate implements the ACP (Agent Client Protocol) server that bridges the TypeScript TUI frontend to the core agent engine. It owns everything between the TUI and the agent engine: receiving ACP requests, managing sessions, calling the LLM API, persisting state, and sending events back. The actual agent loop (tool execution, permissions, hooks, MCP servers) lives in the `agent` crate.

```
┌─────────────────┐     ACP (JSON-RPC)      ┌──────────────────────┐
│   TUI Client    │ ◄──────────────────────► │   Rust Agent         │
│  (TypeScript)   │     stdio transport      │   (this crate)       │
└─────────────────┘                          └──────────────────────┘
        │                                              │
   AcpClient                                    SessionManager
        │                                              │
   Kiro class                                   AcpSession(s)
        │                                              │
   React TUI                                    Agent (agent crate)
```

## Crate Structure

| Directory | Purpose |
|-----------|---------|
| `agent/` | ACP protocol layer, LLM streaming, and session persistence. Implements the ACP server using the `sacp` SDK, bridges to the `agent` crate's engine, and persists session state to disk. See `agent/README.md` for details. |
| `api_client/` | AWS API client. Calls the Q Developer streaming API (`send_message`), model listing, usage limits, and telemetry. Includes `IpcMockApiClient` for test injection. |
| `auth/` | Authentication. Builder ID (OIDC device flow), PKCE, SSO portal, social/external IdP login, token refresh. |
| `cli/` | CLI entry point and subcommands. Argument parsing, `chat` (launches TUI or legacy mode), `user` (login/logout/whoami), `mcp`, agent config loading. |
| `database/` | SQLite storage. Settings, credentials, auth profiles, conversation metadata, client ID. Includes migration system. |
| `mcp_registry.rs` | MCP server registry. Fetches remote registries, caches responses, validates server definitions, filters tools. |
| `telemetry/` | Event tracking. Usage metrics, tool use events, auth events, heartbeats. Background thread with batching. |
| `util/` | Shared utilities. Path resolution, knowledge store, file URI handling, environment variables, system info. |
| `os/` | Platform abstraction. Filesystem operations (`Fs` with chroot for testing), environment detection, diagnostics. |
| `theme/` | Terminal styling. Color definitions and crossterm extensions for the legacy V1 terminal UI. |

## Development

### Building

```bash
# Build the binary
cargo build -p chat_cli

# Run directly
cargo run -p chat_cli -- chat
```

### Testing

```bash
# Run unit tests
cargo test -p chat_cli

# Run ACP integration tests
cargo test -p chat_cli --test acp
```

## ACP Integration Tests

Integration tests validate the ACP protocol implementation by spawning the agent as a subprocess and communicating via stdio.

### Test Infrastructure

Tests are located in `tests/acp.rs` with shared utilities in `tests/common/`.

Notable test utilities:
- **`AcpTestHarness`**: Spawns the `chat_cli acp` subprocess with sandboxed directories
- **`AcpTestClient`**: ACP client, tracks notifications for making assertions
- **`TestPaths`**: Manages isolated test directories for sessions, logs, and working directory

### Mock API Request & Response

The test harness supports injecting mock LLM API responses via IPC, allowing tests to simulate agent behavior without real API calls:

1. **IPC Connection**: After creating a session, call `harness.wait_for_ipc()` to establish the mock injection channel
2. **Push Responses**: Use `harness.push_mock_response()` or `harness.push_mock_responses_from_file()` to inject `ChatResponseStream` events
3. **End Stream**: Call `harness.push_mock_response_end()` to signal the end of a response

The agent uses `IpcMockApiClient` when `KIRO_TEST_MODE` is set, which receives mock responses through the IPC socket instead of calling the real LLM API.

The test harness can retrieve LLM API requests via `harness.get_captured_requests(session_id)`, which returns `Vec<ConversationState>` - the full request structure including history, current message, and tools.

#### Recording Live API Responses

To generate mock test data from live traffic:

```bash
KIRO_RECORD_API_RESPONSES_PATH=tests/mock_responses/{test_name}.jsonl cargo run -- chat --legacy-mode
```

> [!warning]
> The old agent loop uses different tool names than the agent crate, e.g. `fs_write` vs `write`.
> TODO: Have a mechanism for recording test data using the new ACP agent loop.

This records all API response events to the specified file. The format is JSONL with blank lines separating response streams (one stream per prompt). Comments starting with `//` are ignored when parsing.

### Test Output

All test artifacts are written to `test_output/{test_name}/`:

```
test_output/
├── initialize/
│   └── agent.log
└── new_session_creates_files/
    ├── agent.log
    ├── cwd/
    └── sessions/
        ├── {session_id}.json
        ├── {session_id}.jsonl
        └── {session_id}.lock
```

This makes debugging easier - logs and session files persist after test runs.

