# KAS Slash Command Parity Tracker

Status of slash commands in KAS mode (`KasAcpClient`) vs Rust ACP backend (`RustAcpClient`).

**Architecture**: The TUI dispatches commands through `executeCommand()` and `getCommandOptions()` on the `SessionClient`. In Rust mode these forward to the backend via `_kiro.dev/commands/execute` / `_kiro.dev/commands/options`. In KAS mode we use `kiroClient.sendExtMethod('_kiro/commands/execute', ...)`.

**Key difference**: KAS uses `_kiro/commands/execute` (no `.dev`), Rust uses `_kiro.dev/commands/execute`.

## Workflow

- Using `jj` for version control (not git)
- Each slash command implementation gets its own rev (`jj new`)
- Keep revs focused: one command per rev for clean history
- Changes may span both `kiro-cli` and `kiro-agent` repos (e.g., KAS server-side handler in kiro-agent, TUI client-side wiring in kiro-cli)

## Command Status

| # | Command | inputType | Rust ACP | KAS Server | KasAcpClient | Notes |
|---|---------|-----------|----------|------------|--------------|-------|
| 1 | `/agent` | selection | ✅ | ✅ `slashAgent()` | ✅ Forwarded | Swap works. Create/edit not in KAS. |
| 2 | `/model` | selection | ✅ | ✅ `slashModel()` | ❌ Blocked | See blockers below. |
| 3 | `/quit` | local | ✅ | N/A | ✅ Local | Handled client-side, never hits server. |
| 4 | `/help` | panel | ✅ | ✅ | ✅ Forwarded | |
| 5 | `/context` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 6 | `/compact` | — | ✅ | ❌ | ❌ | See blockers below. |
| 7 | `/clear` | — | ✅ | ✅ | ✅ Forwarded | |
| 8 | `/usage` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 9 | `/mcp` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 10 | `/tools` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 11 | `/knowledge` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 12 | `/prompts` | selection | ✅ | ❌ | ❌ | See blockers below. |
| 13 | `/feedback` | selection | ✅ | N/A | ✅ Client-side | Static options, opens GitHub URLs. |
| 14 | `/chat` | selection | ✅ | ✅ | ✅ Partial | List + switch + new work. Save/load to file not supported (Rust-specific export format). |
| 15 | `/code` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 16 | `/hooks` | panel | ✅ | ❌ | ❌ | See blockers below. |
| 17 | `/plan` | — | ✅ | ✅ | ✅ Forwarded | ⚠️ Rust switches to `kiro_planner` agent (custom agent profile with its own system prompt/tools). KAS maps to `spec` mode instead — not equivalent. |
| 18 | `/paste` | — | ✅ | ❌ | ❌ | See blockers below. |
| 19 | `/reply` | — | ✅ | ❌ | ❌ | See blockers below. |
| 20 | `/guide` | — | ✅ | ❌ | ❌ | See blockers below. |

**Legend**: ✅ = implemented, ❌ = not implemented / stubbed

## Blockers for Remaining Commands

### `/model` — Blocked on KAS model config provider
KAS has `slashModel()` implemented server-side, but `getAvailableModels()` returns empty in standalone mode. The model list is injected by the IDE via `setModelConfigProvider()`, which is never called when running via `acp-server.js`. Needs either: (a) a standalone model config provider for CLI mode, or (b) the model list passed from the CLI at startup.

### `/tools` — Blocked on permission model mismatch
Rust has per-tool trust/deny/allowed-by-config status. KAS uses autopilot (on/off) + runtime policy engine evaluation. The policy engine requires specific tool call arguments to evaluate — there's no static "what's the default status of this tool" query. Listing tools is straightforward (`session.workspace.getChatTools()`), but the `status` field cannot accurately reflect the Rust behavior. Trust/untrust/reset subcommands have no KAS equivalent.

### `/context` — Blocked on KAS context API
Rust exposes context breakdown (token usage per source) and add/remove/clear operations. KAS manages context internally via `ProgressiveContextManager` and steering, but doesn't expose an equivalent slash command API. Needs new KAS server-side handler with access to context state.

### `/compact` — Blocked on KAS compaction API
Rust calls `agent.compact_conversation()` which summarizes history. KAS has summarization (`SummarizationDetectionNode`) but it's triggered automatically, not via a slash command. Needs a new KAS handler to trigger compaction on demand.

### `/mcp` — Blocked on KAS MCP management API
Rust lists MCP servers with status and supports add/remove. KAS has `mcpConfigManager` but doesn't expose list/add/remove via slash commands. Needs new KAS handler.

### `/knowledge` — Blocked on KAS knowledge base
Rust has a full knowledge store (`semantic-search-client` crate) with add/remove/search/update. KAS doesn't have an equivalent knowledge base system. Would need significant new functionality.

### `/prompts` — Blocked on KAS prompt listing
Rust lists available MCP prompts and executes them. KAS has skill/prompt support via `SlashCommandManager` but doesn't expose a listing API through `executeSlashCommand`. Needs new KAS handler.

### `/usage` — Blocked on billing API
Rust calls the Q Developer API for usage/billing info. KAS doesn't have access to the same billing endpoint in standalone mode. May not apply to KAS mode at all.

### `/code` — Blocked on KAS code intelligence
Rust has the `code-agent-sdk` crate for LSP-based code intelligence. KAS has its own code tools but doesn't expose workspace status/init/logs/overview via slash commands. Needs new KAS handler.

### `/hooks` — Blocked on KAS hooks API
Rust lists configured hooks from agent config. KAS has hook support but doesn't expose a listing API via slash commands. Needs new KAS handler.

### `/paste` — Blocked on native clipboard access
Rust uses the `arboard` crate for native clipboard image reading. The TUI runs in bun/Node which doesn't have native clipboard image access without additional native addons. Cannot be implemented client-side in KAS mode without a native dependency.

### `/reply` — Blocked on message access from client layer
Opens `$EDITOR` pre-filled with the last assistant message (quoted). Rust gets the message from the agent's conversation history. `KasAcpClient` doesn't have access to the TUI's message store. Options: (a) add a KAS server-side handler that reads `previousMessages`, or (b) refactor to a TUI-local command that reads from the Zustand store directly.

### `/guide` — Blocked on guide agent
Rust switches to `kiro_guide`, a built-in agent profile with its own system prompt and tools for helping users learn Kiro CLI. KAS doesn't have an equivalent guide agent/mode. Would need a new KAS mode or custom agent profile.

## File References

- **KasAcpClient**: `packages/tui/src/acp-client.ts` (class `KasAcpClient`)
- **KAS_BUILTIN_META**: `packages/tui/src/acp-client.ts` (bottom of file)
- **KAS slash dispatch**: `kiro-agent/packages/kiro-agent/src/agent.ts` (`executeSlashCommand`)
- **KAS builtin commands**: `kiro-agent/packages/kiro-agent/src/slash-commands/builtin-commands.ts`
- **Rust command enum**: `crates/agent/src/agent/tui_commands/command.rs`
- **Rust command handlers**: `crates/chat-cli-v2/src/agent/acp/commands/`
- **TUI command dispatcher**: `packages/tui/src/commands/dispatcher.ts`
- **TUI command effects**: `packages/tui/src/commands/effects.ts`
