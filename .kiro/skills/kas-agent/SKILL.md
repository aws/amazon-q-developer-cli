---
name: kas-agent
description: Guide for integrating with the KAS (Kiro Agent Server) TypeScript agent engine. Use when working on KAS support in kiro-cli, debugging KAS launch issues, understanding the KAS ACP interface, or modifying how the TUI communicates with KAS. Triggers on questions about KAS, kiro-agent, agent engine selection, or TypeScript agent integration.
---

# KAS Agent Interface

KAS (Kiro Agent Server) is the TypeScript agent engine from `@kiro/agent` that powers Kiro IDE. It is an alternative to the Rust ACP agent (`chat_cli acp`) for the V2 TUI.

## Architecture

```
Rust CLI (chat_cli)
  └─ launch_v2()                          # crates/chat-cli/src/embedded_tui.rs
       └─ spawns bun with TUI JS bundle
            └─ TUI (packages/tui)
                 └─ createAcpClient()     # packages/tui/src/acp-client.ts
                      ├─ RustAcpClient    # default: spawns `chat_cli acp`
                      └─ KasAcpClient     # when KIRO_AGENT_ENGINE=kas: spawns `node acp-server.js`
```

## How KAS Is Selected

1. User passes `--agent-engine=kas` CLI flag
2. `cli/mod.rs` sets `KIRO_AGENT_ENGINE=kas` env var
3. `embedded_tui.rs` reads env var, sets `KIRO_AGENT_PATH=node` and `KIRO_AGENT_ENGINE=kas` on the bun child process
4. TUI's `createAcpClient()` checks `KIRO_AGENT_ENGINE === 'kas'` and returns `KasAcpClient` instead of `RustAcpClient`

## KAS Server Resolution

`KasAcpClient` resolves the ACP server path in order:
1. `KIRO_KAS_SERVER_PATH` env var (dev/testing override)
2. Walk up from `__dirname` looking for `node_modules/@kiro/agent/dist/server/acp-server.js`

The server is spawned as: `node <acp-server.js> --transport=stdio`

## KAS ACP Server (`acp-server.ts`)

Entry point: `packages/kiro-agent/src/server/acp-server.ts`

CLI args:
- `--transport=stdio|ws` (default: stdio)
- `--auth=user|machine` (default: user)
- `--token-path=<path>` (custom auth token file)
- `--region=<region>` (AWS region override)
- `--endpoint=<url>` (Q service endpoint override)
- `--execution-environment=local|sandbox` (default: local)
- `--home-dir=<path>` (home directory override)
- `--sandbox=auto|seatbelt|bubblewrap|docker|none`

Env vars:
- `ACP_WS_PORT` — WebSocket port (default: 8082, ws transport only)
- `KIRO_LOG_LEVEL` — Log verbosity (error|warn|info|debug|trace)
- `KIRO_CHAT_LOG_FILE` — Log file path
- `KIRO_AGENT_VERSION` — Version string for user agent

## KiroAgent Class

Main class: `packages/kiro-agent/src/agent.ts` → `KiroAgent`

Constructor: `new KiroAgent(stream, options: KiroAgentOptions)`

Key options:
- `authProvider: IAuthProvider` — required
- `workspaceTrusted: boolean` — required
- `region?: string`, `endpoint?: string` — AWS overrides
- `executionEnvironment?: ExecutionEnvironment` — 'local' or 'sandbox'
- `sandbox?: SandboxConfig` — process isolation config
- `homeDir?: string` — home directory override
- `customUserAgent?: string`

## Agent Modes

KAS supports these modes (set via `setSessionConfigOption` after session creation):

Built-in: `vibe`, `spec`, `autonomous`
Internal: `custom`, `autocomplete`, `summarization`, `intent-classification`, `generate-hook`, `execute-hook`, `generate-steering`, `refine-steering`, `refine-requirements`, `refine-design`, `update-tasks`

The CLI passes mode via `KIRO_MODE` env var → TUI reads it and calls:
```typescript
kiroClient.setSessionConfigOption({ sessionId, configId: 'mode', value: mode });
```

## KasAcpClient vs RustAcpClient

Both implement `SessionClient` (defined in `packages/tui/src/acp-client.ts`).

| Aspect | RustAcpClient | KasAcpClient |
|--------|---------------|--------------|
| Backend | `chat_cli acp` (Rust) | `node acp-server.js` (TypeScript) |
| ACP SDK | `sacp` crate | `@agentclientprotocol/sdk` |
| Protocol | `@agentclientprotocol/sdk` (TS client) | `KiroClient` from `@kiro/client` |
| Session create | `client.newSession()` | `kiroClient.newSession()` + config options |
| Spawned by | TUI via `KIRO_AGENT_PATH` | TUI directly via `spawn('node', [...])` |

## Key Differences in Session Lifecycle

KAS sessions have extra setup after creation:
1. `setSessionConfigOption('autopilot', 'on')` — enables autopilot by default
2. `setSessionConfigOption('mode', mode)` — sets agent mode if `KIRO_MODE` is set

## Package Info

- Package: `@kiro/agent` (npm, published to CodeArtifact)
- Source: `kiro-agent` repo at `packages/kiro-agent/`
- Entry: `dist/index.js` (ESM) / `dist/index.cjs` (CJS)
- Server binary: `dist/server/acp-server.js`
- Dependencies: `@agentclientprotocol/sdk ^0.19.0`, `@langchain/langgraph`, `zod`, etc.

## Development

The simplest way to run KAS locally is via the TUI dev script:

```bash
# Run with KAS (handles CodeArtifact auth, bun install, and env vars automatically)
cd packages/tui
KIRO_AGENT_ENGINE=kas bun run dev --skip-rust-build

# Or use the convenience script from repo root
./scripts/test-kas.sh
```

The dev script automatically:
- Checks CodeArtifact token expiry and refreshes if needed
- Skips setting `KIRO_AGENT_PATH` (KAS uses `node` directly)
- Sets `KIRO_KAS_TOKEN_PATH` to `~/.aws/sso/cache/kiro-auth-token-cli.json`

For manual control or overriding the KAS server path:

```bash
# Override KAS server path for local kiro-agent development
KIRO_AGENT_ENGINE=kas KIRO_KAS_SERVER_PATH=/path/to/kiro-agent/dist/server/acp-server.js \
  bun run dev --skip-rust-build

# Full Rust binary path (production-like)
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
  cargo run -p chat_cli -- chat --agent-engine=kas
```

KAS logs: `~/.kiro/logs/<timestamp>/kiro.log`

## File References

- CLI agent engine enum: `crates/chat-cli/src/cli/chat/mod.rs` (AgentEngine)
- Env var propagation: `crates/chat-cli/src/cli/mod.rs`
- TUI launch with KAS env: `crates/chat-cli/src/embedded_tui.rs`
- TUI client factory: `packages/tui/src/acp-client.ts` (createAcpClient)
- KAS ACP server: `kiro-agent/packages/kiro-agent/src/server/acp-server.ts`
- KiroAgent class: `kiro-agent/packages/kiro-agent/src/agent.ts`
- Agent modes: `kiro-agent/packages/kiro-agent/src/types/shared-types.ts`
