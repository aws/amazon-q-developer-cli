# kiro-cli source-tree priorities

Reference for the kiro-help bot. Loaded on demand via `read` when the agent needs to know which paths to dive into.

The bot's CWD is a fresh `--depth 1` clone of `kiro-team/kiro-cli` at `/var/lib/kiro-cli`, refreshed on container start. Both V1 and V2 are actively maintained, but **V2 + the TypeScript TUI is the primary surface for current behavior**. Default to V2 paths first; check V1 only when explicitly asked about classic OR during bug-report triage for parity.

## Architecture (high level)

```
packages/tui/  (TypeScript/React)  ←── ACP over stdio ──→  crates/chat-cli-v2/  (Rust)  ──→  crates/agent/
                  TUI frontend                                  ACP server, sessions             core agent loop, tools, MCP

crates/chat-cli/  (Rust, V1 — legacy monolith with integrated TUI; uses crates/agent/ only for subagent)

KAS engine: --agent-engine=kas → KasAcpClient (TypeScript engine from Kiro IDE, alternative backend)
```

## Read-first paths

**V2 backend / agent engine (Rust):**

| Area | Path |
|---|---|
| ACP server, sessions, agent dispatch | `crates/chat-cli-v2/src/agent/acp/` |
| V2 chat surface (CLI entry, settings, feed) | `crates/chat-cli-v2/src/cli/` (NOT `cli/chat/legacy/`) |
| MCP registry, agent loader, launch options | `crates/chat-cli-v2/src/` |
| Core agent loop, tool execution, MCP, subagents | `crates/agent/src/agent/` |

**V2 TUI (TypeScript — this is where TUI behavior lives, NOT in `crates/chat-cli-ui/`):**

| Area | Path |
|---|---|
| TUI entry, ACP client, agent engine plumbing | `packages/tui/src/` |
| TUI components, hooks, stores, theme | `packages/tui/src/components/`, `hooks/`, `stores/` |
| Twinki terminal renderer | `packages/twinki/` |
| E2E test harness (PTY + xterm.js) | `packages/terminal-harness/` |

**Top-level CLI / cross-cutting:**

| Area | Path |
|---|---|
| Flag parsing, `--tui`/`--legacy-ui`/`--agent-engine` resolution | `crates/chat-cli/src/cli/mod.rs`, `crates/chat-cli/src/cli/chat/mod.rs` |
| Rust UI protocol shim (input bar, conduit — small crate, NOT the TUI) | `crates/chat-cli-ui/src/` |
| kiro-bot runtime, frontends, config (this bot itself) | `crates/kiro-bot/` |
| Release notes / changelog | `crates/chat-cli/src/cli/feed.json`, `crates/chat-cli-v2/src/cli/feed.json` |

**V1 binary (legacy but maintained — read for parity checks and explicit classic questions):**

| Area | Path |
|---|---|
| V1 main agent, conversation state | `crates/chat-cli/src/cli/chat/conversation.rs`, `crates/chat-cli/src/cli/chat/` |
| V1 MCP client (separate from `agent` crate's) | `crates/chat-cli/src/mcp_client/` |
| V1 tool manager | `crates/chat-cli/src/cli/chat/tool_manager.rs` |

**Skip unless explicitly asked about migration / legacy data:**

- `crates/chat-cli-v2/src/cli/chat/legacy/` — V1 data structures kept for migration
- `crates/chat-cli/src/cli/chat/v1_export/` — V1 session export shim

## Key files (jump table for common topics)

When the user's question maps to one of these, start here instead of grep-walking the tree.

| Topic | Path |
|---|---|
| ACP agent (V2 core) | `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` |
| Session manager | `crates/chat-cli-v2/src/agent/acp/session_manager.rs` |
| Agent loop | `crates/agent/src/agent/mod.rs` |
| Tool execution | `crates/agent/src/agent/task_executor/mod.rs` |
| MCP manager | `crates/agent/src/agent/mcp/mod.rs` |
| Subagent tool | `crates/agent/src/agent/tools/use_subagent.rs` |
| V1 conversation state | `crates/chat-cli/src/cli/chat/conversation.rs` |
| TUI entry point | `packages/tui/src/index.tsx` |
| ACP client (TUI) | `packages/tui/src/acp-client.ts` |
| TUI app state store | `packages/tui/src/stores/app-store.ts` |
| Twinki renderer | `packages/twinki/packages/twinki/src/renderer/tui.ts` |
| TUI input handling | `packages/twinki/packages/twinki/src/hooks/useInput.ts` |

**When the user's question is ambiguous about which surface they mean:** ask a one-line clarifying follow-up (e.g. "Are you on the new TUI or `--classic`?") rather than guessing. If you have to guess, guess V2 + TUI and say so.
