# ACP-layer integration tests

End-to-end tests for the TUI + `KasAcpClient` + `@kiro/client` stack, running
against an in-process mock ACP server. Tests spawn the real TUI under a
real PTY and drive it through the same UI entrypoints a user would, while
scripting the agent side of the ACP protocol from the test code.

## When to use this vs `integ_tests/`

| You want to verify...                                                       | Use                       |
| --------------------------------------------------------------------------- | ------------------------- |
| UI rendering / input handling / menus / keybindings                         | `integ_tests/`            |
| Dispatcher / effects / app-store flows                                      | `integ_tests/`            |
| `KasAcpClient.initialize()` filter, `executeCommand`, cached state          | `acp_integ_tests/` (here) |
| Exact `session/list` / `session/new` / etc. ACP request shape the TUI sends | `acp_integ_tests/` (here) |
| TUI response to agent notifications (`current_mode_update`, etc.)           | `acp_integ_tests/` (here) |
| Agent -> client requests (`session/request_permission`)                     | `acp_integ_tests/` (here) |

`integ_tests/` mocks at the `SessionClient` interface - everything under it
(including all of `KasAcpClient`) is stubbed. `acp_integ_tests/` mocks at the
ACP wire layer via a Unix socket, so `KasAcpClient` and `@kiro/client`
run for real.

**Scope: KAS only.** V2 (`RustAcpClient`) is on its way out; this harness
does not support it.

## Running

```bash
bun run --cwd packages/tui test:acp-integ
# or for one file:
bun test ./acp_integ_tests/initialize-handshake.test.ts
```

Prerequisites (one-time):

```bash
bun install
# twinki is a workspace dep consumed by the TUI; must be built at least once.
bun run --cwd packages/twinki/packages/twinki build
```

## Writing a test

```ts
import { describe, it, expect, afterEach } from 'bun:test';
import { AcpTestCase } from './shared/AcpTestCase';

describe('my feature', () => {
  let tc: AcpTestCase | null = null;
  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('does the thing', async () => {
    tc = new AcpTestCase({ testName: 'my-feature' });

    // Handlers return canned responses only. DO NOT put `expect(...)` inside
    // a handler - if it throws, the server catches it and returns a
    // JSON-RPC error to the TUI, and the symptom surfaces downstream as a
    // confusing assertion failure. Assert on `receivedRequests` in the test
    // body instead (see below).
    tc.mock.on('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: { sessionCapabilities: {} },
    }));
    tc.mock.on('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();

    // Drive the TUI.
    await tc.sendKeys('/agent');
    await tc.pressEnter();

    // Assert on what the TUI sent (errors here surface as test failures).
    const reqs = tc.mock.receivedRequests('session/new');
    expect(reqs).toHaveLength(1);
    expect((reqs[0]!.params as { cwd: string }).cwd).toBeDefined();

    // Push notifications to the TUI.
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: { sessionUpdate: 'current_mode_update', currentModeId: 'spec' },
    });

    // Inspect TUI state.
    const state = await tc.getStore();
    expect(state.agent?.name).toBe('spec');
  });
});
```

## `AcpMockServer` API

```ts
// Register a handler for requests with the given method.
server.on<Req, Resp>(method: string, handler: (params: Req) => Promise<Resp> | Resp): this;

// Fire a notification at the TUI (no response expected).
server.notify(method: string, params: unknown): void;

// Send a request to the TUI and await its response. Used for agent -> client
// calls like `session/request_permission`.
server.request<Resp>(method: string, params: unknown): Promise<Resp>;

// Test assertion helpers.
server.receivedRequests(method?: string): Array<{ method: string; params: unknown }>;
server.receivedNotifications(method?: string): Array<{ method: string; params: unknown }>;

// Control.
server.awaitConnection(): Promise<void>;  // resolves on first client connect
server.close(): Promise<void>;
```

## How it works

```
Test runner process                  TUI subprocess (PTY)
┌──────────────────────┐             ┌──────────────────────────────┐
│ AcpMockServer        │             │ MockAcpTransport ({readable, │
│ (listens on socket)  │ ◄────────► │  writable} Stream shape that │
│ .on / .notify        │   Unix      │  @kiro/client consumes)       │
│ .request / .received │   socket    │                               │
└──────────────────────┘             │ KiroClient → KasAcpClient →   │
         ▲                           │ Kiro → dispatcher → UI        │
         │                           └──────────────────────────────┘
         ▼                                    ▲
┌──────────────────────┐                     │ PTY
│ AcpTestCase          │ ─────────────────────┘
│ (extends TestCase)   │
└──────────────────────┘
```

Activation is via `KIRO_ACP_MOCK_SOCKET` env var. `KasAcpClient` checks it
at construction and, if set, connects to the socket instead of spawning
a KAS subprocess. The rest of the client (SDK, command dispatch, cached
state, notifications) is identical to production behavior.

See `packages/tui/docs/acp-recording.md` for the companion feature
(`KIRO_ACP_RECORD_PATH`) that captures real wire traffic to a JSONL file.
