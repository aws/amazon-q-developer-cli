# Kiro CLI TUI Testing

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Test Runner (Bun)                             │
│                                                                         │
│  - Spawns TUI in PTY                                                    │
│  - Sends keystrokes via PTY                                             │
│  - Connects to TUI IPC socket to inspect Zustand store                  │
│  - Connects to Agent IPC socket to inject mock LLM responses (E2E only) │
└─────────────────────────────────────────────────────────────────────────┘
        │                           │                           │
        │ PTY                       │ TUI IPC                   │ Agent IPC
        │ (stdin/stdout)            │ (Unix socket)             │ (Unix socket, E2E only)
        ▼                           ▼                           ▼
┌────────────────────────────────────────────┐    ┌───────────────────────┐
│              TUI Process (React/Ink)       │    │   Rust Agent          │
│                                            │    │   (chat_cli acp)      │
│  ┌──────────────┐    ┌──────────────────┐  │    │                       │
│  │ Zustand      │    │ TestModeProvider │  │    │  ┌─────────────────┐  │
│  │ Store        │◄──►│ (IPC server)     │  │    │  │ IpcMockApiClient│  │
│  └──────────────┘    └──────────────────┘  │    │  │ (mock LLM)      │  │
│                                            │    │  └─────────────────┘  │
│  ┌──────────────┐        ACP Protocol      │    │                       │
│  │ AcpClient    │◄────────────────────────►│    │                       │
│  └──────────────┘                          │    └───────────────────────┘
└────────────────────────────────────────────┘
         │                                                  ▲
         │ Integration: MockSessionClient (no Rust agent)   │
         └─────── E2E: Real AcpClient ──────────────────────┘

Integration Tests: TUI + MockSessionClient (no Rust)
E2E Tests:         TUI + Real Rust Agent (LLM API mocked)
```

### Integration test vs E2E test

Both tests share the same foundational architecture (real PTY, sequential execution, IPC-based state inspection), but with key differences:

| Aspect | Integration Tests | E2E Tests |
|--------|------------------|-----------|
| Backend | `MockSessionClient` (TypeScript) | Real `kiro-cli chat acp` (Rust) |
| What's mocked | Entire ACP layer | Only LLM API responses |
| IPC connections | TUI only | TUI + Rust agent |
| State inspection | Zustand store | Zustand store + Acp session state |
| Use case | UI behavior, state management | Full protocol flow, agent logic |


## Integration Testing (ACP client mocked)

The TUI package includes comprehensive integration testing capabilities that validate the interaction between UI components, state management, and terminal behavior in a real PTY environment with a mocked ACP client.

### Test Capabilities

The integration test framework provides three key testing capabilities:

- **Raw Terminal Output Validation**: Tests can capture and analyze actual terminal output, including ANSI escape sequences, to verify the visual presentation matches expectations
- **Zustand Store Inspection**: Direct access to the application's internal state via IPC communication, enabling assertions on input state, message history, exit sequences, and other store properties
- **Mock ACP Client Responses**: Ability to inject mock `AgentStreamEvent` instances (content chunks, tool calls, approval requests) through the `MockSessionClient` to test various agent interaction scenarios

### Architecture & Dependencies

The integration testing architecture uses a dual-process approach with IPC communication:

- **Real PTY Environment**: Tests spawn the TUI in an authentic pseudo-terminal using `bun-pty`, ensuring terminal interactions behave exactly as they would for end users
- **Mock Backend**: The Rust backend is replaced with a `MockSessionClient` implementation that simulates ACP protocol events without requiring a real agent process
- **IPC Socket Communication**: A Unix socket connection between the test runner and TUI process enables real-time state inspection via the `TestModeProvider` component
- **Test Mode Activation**: The `KIRO_TEST_MODE` environment variable must be set to `"true"` to trigger test-specific behavior, including mock client instantiation and IPC server setup

### Test Structure & API

Integration tests are located in `integ_tests/` and use the `TestCase` API for configuration and execution.

Tests follow a sequential execution model where each test case represents a series of interactions with the running TUI process. You can interleave assertions about terminal output or application state, send keyboard input to simulate user actions, and inject mock ACP responses to simulate backend behavior - all as the test progresses step by step.

> [!note]
> Always include a small delay (e.g., `sleepMs(50)`) between user input and `getStore` calls. PTY input processing and React state updates are asynchronous, so without this delay, `getStore` may return stale state.

> [!tip]
> Use `testCase.getSnapshotFormatted()` to get a human-readable terminal snapshot for debugging. This renders the current PTY buffer as a formatted string showing exactly what the user would see.

```typescript
// Basic test structure using TestCase builder pattern
const testCase = await TestCase.builder()
  .withTerminal({ width: 80, height: 24 })
  .withTimeout(15000)
  .launch();

// Wait for UI elements and send input
await testCase.waitForVisibleText('> ');
await testCase.sendKeys('hello world');

// IMPORTANT: Allow time for state to update before inspecting
await testCase.sleepMs(50);

// Inspect application state
const state = await testCase.getStore();
expect(state.input.lines[0]).toBe('hello world');

// Inject mock agent events
await testCase.mockSessionUpdate({
  type: AgentEventType.Content,
  content: { type: ContentType.Text, text: 'Response!' }
});
```

## E2E Testing

E2E tests validate the complete stack: TUI → ACP → Rust Agent. Unlike integration tests that mock the ACP layer, E2E tests run the real Rust backend with only the LLM API calls mocked.

### Architecture

E2E tests establish dual IPC connections:

1. **TUI IPC** (`KIRO_TEST_TUI_IPC_SOCKET_PATH`): Same as integration tests - inspect Zustand store via `getStore()`
2. **Agent IPC** (`KIRO_TEST_CHAT_IPC_SOCKET_PATH`): New connection to Rust backend - inspect agent state via `getAgentState()` and inject mock LLM responses via `pushSendMessageResponse()`

### Test Structure & API

E2E tests are located in `e2e_tests/` and use the `E2ETestCase` API:

```typescript
const testCase = await E2ETestCase.builder()
  .withTerminal({ width: 80, height: 24 })
  .withTestName('my-test')
  .launch();

try {
  await testCase.waitForVisibleText('>', 10000);

  // Inspect both TUI and agent state
  const store = await testCase.getStore();
  const agentState = await testCase.getAgentState();

  // Inject mock LLM response (ChatResponseStream events)
  await testCase.pushSendMessageResponse([
    { kind: 'AssistantResponseEvent', data: { content: 'Hello!' } },
  ]);
  await testCase.pushSendMessageResponse(null); // Signal end of response

  await testCase.sendKeys('hi\n');
  await testCase.waitForVisibleText('Hello!', 10000);
} finally {
  await testCase.cleanup();
}
```

### Running E2E Tests

```bash
# Run E2E tests (builds Rust binary first)
bun run test:e2e

# Skip Rust build if binary is already up to date
bun run test:e2e --skip-rust-build
```
