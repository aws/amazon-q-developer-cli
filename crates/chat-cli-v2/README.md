# kiro-cli

Rust backend for the Kiro CLI chat application. This crate provides the ACP (Agent Client Protocol) agent implementation that handles LLM interactions, tool execution, and session management.

## Role in Architecture

The chat-cli crate serves as the backend layer in the Kiro CLI chat application:

- **ACP Agent**: Implements the Agent Client Protocol server for communication with the TUI frontend
- **LLM Integration**: Handles API calls to the language model service
- **Tool Execution**: Manages built-in tools (file operations, bash execution, AWS CLI, etc.)
- **Session Management**: Persists conversation state and session metadata
- **MCP Support**: Integrates Model Context Protocol servers for extensibility

## ACP Actor Architecture

This crate implements a tokio actor-based architecture for handling ACP protocol messages. Each actor
follows the **handle/request/response pattern**: a `Handle` type provides an async API that sends
typed `Request` messages to the actor and awaits `Response` messages via oneshot channels.

### Actor Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              SACP Protocol Layer                                │
│                                                                                 │
│  Receives ACP requests from the TUI client over stdio and converts them into   │
│  SessionManager messages. Sends ACP notifications back to the client.          │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              SessionManager                                     │
│                                                                                 │
│  Central coordinator that owns all active sessions. Routes requests to the      │
│  appropriate AcpSession and forwards notifications back to the ACP client.      │
│                                                                                 │
│  Key responsibilities:                                                          │
│  - Session lifecycle (create, load, terminate)                                  │
│  - Client connection management                                                 │
│  - Tool approval request routing                                                │
│  - Agent mode switching                                                         │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                          ┌────────────┴────────────┐
                          ▼                         ▼
┌─────────────────────────────────┐   ┌─────────────────────────────────┐
│         AcpSession              │   │         AcpSession              │
│                                 │   │                                 │
│  Represents a single ACP        │   │  (Multiple sessions can exist   │
│  session. Bridges the ACP       │   │   concurrently, e.g. subagents) │
│  protocol to the Agent.         │   │                                 │
│                                 │   └─────────────────────────────────┘
│  - Session persistence          │
│  - Custom extension handlers    │
│    (e.g. slash commands)        │
│  - Protocol translation         │
└─────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────┐
│            Agent                │
│      (from `agent` crate)       │
│                                 │
│  Core LLM agent implementation. │
│  See agent crate README for     │
│  its internal architecture.     │
└─────────────────────────────────┘
```

### Message Flow

**Prompt Request Flow (Ingress):**
```
TUI ──[PromptRequest]──► SessionManager ──► AcpSession ──► Agent
```

**Event Flow (Egress):**
```
Agent ──[AgentEvent]──► AcpSession ──► SessionManager ──[SessionNotification]──► TUI
```

## ACP Actor Architecture

This crate implements a tokio actor-based architecture for handling ACP protocol messages. Each actor
follows the **handle/request/response pattern**: a `Handle` type provides an async API that sends
typed `Request` messages to the actor and awaits `Response` messages via oneshot channels.

### Actor Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              SACP Protocol Layer                                │
│                                                                                 │
│  Receives ACP requests from the TUI client over stdio and converts them into   │
│  SessionManager messages. Sends ACP notifications back to the client.          │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              SessionManager                                     │
│                                                                                 │
│  Central coordinator that owns all active sessions. Routes requests to the      │
│  appropriate AcpSession and forwards notifications back to the ACP client.      │
│                                                                                 │
│  Key responsibilities:                                                          │
│  - Session lifecycle (create, load, terminate)                                  │
│  - Client connection management                                                 │
│  - Tool approval request routing                                                │
│  - Agent mode switching                                                         │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                          ┌────────────┴────────────┐
                          ▼                         ▼
┌─────────────────────────────────┐   ┌─────────────────────────────────┐
│         AcpSession              │   │         AcpSession              │
│                                 │   │                                 │
│  Represents a single ACP        │   │  (Multiple sessions can exist   │
│  session. Bridges the ACP       │   │   concurrently, e.g. subagents) │
│  protocol to the Agent.         │   │                                 │
│                                 │   └─────────────────────────────────┘
│  - Session persistence          │
│  - Custom extension handlers    │
│    (e.g. slash commands)        │
│  - Protocol translation         │
└─────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────┐
│            Agent                │
│      (from `agent` crate)       │
│                                 │
│  Core LLM agent implementation. │
│  See agent crate README for     │
│  its internal architecture.     │
└─────────────────────────────────┘
```

### Message Flow

**Prompt Request Flow (Ingress):**
```
TUI ──[PromptRequest]──► SessionManager ──► AcpSession ──► Agent
```

**Event Flow (Egress):**
```
Agent ──[AgentEvent]──► AcpSession ──► SessionManager ──[SessionNotification]──► TUI
```

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

