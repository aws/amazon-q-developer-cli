# Kiro CLI source map

Use V2 plus the TypeScript TUI for current behavior. Read V1 for explicit classic questions and bug parity checks.

```text
packages/tui/  <--- ACP --->  crates/chat-cli-v2/  --->  crates/agent/
TypeScript UI                  sessions/server          agent, tools, MCP

crates/chat-cli/ = V1 classic CLI and integrated UI
```

## Current paths

| Area | Start here |
|---|---|
| V2 ACP server and sessions | `crates/chat-cli-v2/src/agent/acp/` |
| V2 CLI surface | `crates/chat-cli-v2/src/cli/` |
| Agent loop, tools, MCP, subagents | `crates/agent/src/agent/` |
| TUI components, hooks, and state | `packages/tui/src/` |
| Terminal renderer and input | `packages/twinki/` |
| Top-level flags and engine selection | `crates/chat-cli/src/cli/mod.rs`, `crates/chat-cli/src/cli/chat/mod.rs` |
| Release notes | `crates/chat-cli/src/cli/feed.json` |
| Kiro bot | `crates/kiro-bot/` |

## Frequent entry points

| Topic | Path |
|---|---|
| ACP agent | `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` |
| Session manager | `crates/chat-cli-v2/src/agent/acp/session_manager.rs` |
| Agent loop | `crates/agent/src/agent/mod.rs` |
| Tool execution | `crates/agent/src/agent/task_executor/mod.rs` |
| MCP manager | `crates/agent/src/agent/mcp/mod.rs` |
| TUI entry | `packages/tui/src/index.tsx` |
| TUI ACP client | `packages/tui/src/acp-client.ts` |
| TUI state | `packages/tui/src/stores/app-store.ts` |
| Twinki input | `packages/twinki/packages/twinki/src/hooks/useInput.ts` |
| V1 conversation | `crates/chat-cli/src/cli/chat/conversation.rs` |

`crates/chat-cli-ui/` is a Rust protocol shim, not the TypeScript TUI. Treat `crates/chat-cli-v2/src/cli/chat/legacy/` and `crates/chat-cli/src/cli/chat/v1_export/` as migration code unless the user asks about migration.
