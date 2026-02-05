## Codebase Overview

> **Note:** Kiro CLI has two architectures under active development:
> - **V1** (`chat_cli`) - Currently released in production
> - **V2** (`chat_cli_v2` + `packages/tui`) - In development, not yet released
>
> Both versions are actively maintained with ongoing feature development.

### Kiro CLI V2 Architecture Overview

Kiro CLI V2 uses a multi-process architecture with Agent Client Protocol (ACP) for communication between the TypeScript frontend and Rust backend:

- **Frontend**: React/Ink-based terminal UI (`packages/tui`) bundled with the main `kiro-cli` Rust binary
- **Backend**: Rust ACP agent implementation spawned with `kiro-cli chat acp` (`crates/chat-cli-v2`)
- **Shared Types**: Type definitions using `typeshare` for Rust → TypeScript generation

**Message Flow:**
```
TUI (TypeScript) ←─ ACP Protocol over stdio ─→ chat_cli_v2 (Rust) ─→ agent crate
```

**Actor Hierarchy in chat_cli_v2:**
- `SessionManager` - Central coordinator for all active sessions
- `AcpSession` - Bridges ACP protocol to Agent, handles session persistence
- `Agent` (from `agent` crate) - Core LLM agent execution

### Kiro CLI V1 Architecture Overview

Kiro CLI V1 uses an integrated architecture where the terminal UI is built directly into the Rust binary:

- **Single Binary**: `chat_cli` crate contains both CLI and terminal UI using crossterm
- **Legacy Agent**: Main agent implementation in `cli/chat/` (~6k LOC)
- **Subagent Support**: Uses the `agent` crate for `use_subagent` tool functionality only

### Architecture Note: `agent` vs `chat_cli` vs `chat_cli_v2` Crates

The `agent` crate is a reusable agent execution engine extracted from `chat_cli`:

**Current state:**
- `chat_cli` - CLI with integrated terminal UI (V1, production)
- `chat_cli_v2` - ACP-based backend for TUI frontend (V2, in development)
- `agent` - Core agent execution engine used by both V1 and V2

**Usage by version:**
- **V1 (chat_cli)**: Uses `agent` crate **only for subagent functionality** via `use_subagent` tool; main agent uses code in `cli/chat/`
- **V2 (chat_cli_v2)**: Uses `agent` crate as the **primary agent engine** for both main agents and subagents

**Dependency relationship:**
- `chat_cli` depends on `chat_cli_v2` (uses it as a library for ACP commands)
- Both `chat_cli` and `chat_cli_v2` depend on `agent`

**During this transition:**
- Active feature development occurs on both V1 and V2
- Some functionality exists in multiple crates (e.g., MCP client code, tool implementations)

### Core Crates

**chat_cli** - Main CLI application (~6k LOC in core modules)
- `cli/agent` - Agent config loading, validation, tool trust/untrust, MCP config management, prompt resolution
- `cli/chat` - Chat loop, conversation state, message formatting, token counting, MCP server lifecycle
  - `conversation.rs` - Conversation state, history management, tangent mode, checkpoints, tool use tracking
  - `mod.rs` - Main chat loop, tool execution, permission prompts, compact history, welcome/changelog
  - `context.rs` - Auto-load context files, steering files, YAML frontmatter parsing
  - `message.rs` - User/assistant message types, tool use results, environment context
  - `parser.rs` - Response stream parsing, tool use extraction, metadata handling
  - `prompt.rs` - Readline integration, syntax highlighting, command completion, paste handling
  - `tool_manager.rs` - MCP tool loading, prompt queries, server orchestration
  - `checkpoint.rs` - Git-based checkpoints for conversation state
  - `token_counter.rs` - Token/char counting for context management
- `cli/chat/tools` - Built-in tool implementations (fs_read, fs_write, grep, glob, code, use_subagent, etc.)
  - Each tool has: validation, permission evaluation, execution, queue description
- `cli/chat/cli` - Subcommands: knowledge, code, mcp, prompts, hooks, checkpoint, persist, compact
- `mcp_client` - MCP client with OAuth, stdio/HTTP transports, env var substitution
- `auth` - Builder ID (PKCE), social auth, portal auth, token refresh, credential storage
- `database` - SQLite for conversations, settings, secrets, auth profiles, migrations
- `telemetry` - Event tracking, metrics, user turn completion, tool use, profile switches
- `util` - Knowledge store, path resolution, file system abstraction
- `theme` - Terminal styling, ANSI colors, crossterm integration

**agent** - Agent execution engine (~2.3k LOC)
- `agent` - Main agent loop, tool/hook execution, permission evaluation, resource collection
  - `mod.rs` - Agent lifecycle, tool validation, hook execution, state management
  - `permissions.rs` - Path/command permission checks, globset matching, canonicalization
  - `types.rs` - Agent ID, conversation metadata, snapshots, settings
- `agent/agent_loop` - Stream event parsing, tool use extraction, request/response handling
  - `types.rs` - Stream events, content blocks, tool specs, metadata, errors
  - `mod.rs` - Agent loop state machine, stream parsing, tool use detection
- `agent/task_executor` - Async tool/hook execution with timeouts, caching, result handling
- `agent/tools` - Tool trait, built-in tools (ls, fs_write, image_read), tool specs, schemas
- `agent/mcp` - MCP manager actor, server lifecycle, OAuth handling, tool/prompt fetching
- `agent/rts` - RTS model integration, conversation state conversion, streaming
- `agent/agent_config` - Agent config v2025-08-22 schema, MCP servers, hooks, tool settings

**code-agent-sdk** - Code intelligence (~1.4k LOC)
- `sdk/client` - Main API: initialize, detect workspace, find symbols, goto definition, references
- `sdk/workspace_manager` - Workspace detection, LSP lifecycle, file watching, language detection
- `sdk/services/symbol_service` - LSP operations: completion, hover, diagnostics, symbols, references
- `sdk/services/tree_sitter_symbol_tests` - Tree-sitter symbol extraction tests
- `lsp/client` - LSP client: initialize, requests, notifications, diagnostics subscription
- `lsp/config` - LSP config: workspace folders, capabilities, initialization params
- `tree_sitter/workspace_analyzer` - Codebase overview, symbol search, repomap generation
- `mcp/server` - MCP server exposing code intelligence tools
- `model/types` - Request/response types for all operations
- `model/entities` - Symbol info, references, diagnostics, workspace edits
- `utils/file` - Text edit application, workspace edit handling
- `config/config_manager` - Language config loading, extension mapping, LSP server lookup

**semantic-search-client** - Knowledge base search (~1k LOC)
- `client/implementation` - Add/search/remove contexts, directory indexing, metadata persistence
- `client/context/context_manager` - Context lifecycle, BM25/semantic search, pagination
- `client/background/background_worker` - Async indexing, progress tracking, cancellation
- `embedding/candle` - Text embeddings using Candle ML (MiniLM, BGE models)
- `pattern_filter` - Include/exclude patterns, gitignore-style matching

**chat-cli-ui** - UI protocol and components
- `protocol` - Event types: agent events, tool calls, messages, reasoning, state deltas
- `subagent_indicator` - Terminal UI widget for subagent execution status
- `conduit` - Structured output routing to stdout/stderr

### Core Packages

**tui** (`packages/tui`) - React/Ink Terminal UI (V2)
- React-based terminal interface using Ink framework
- Communicates with Rust backend via ACP over stdio
- Zustand for state management (messages, approvals, context tracking)
- Key files:
  - `src/index.tsx` - Main app entry point
  - `src/kiro.ts` - Stateless session lifecycle manager
  - `src/acp-client.ts` - ACP protocol implementation
  - `src/stores/app-store.ts` - Zustand state store
  - `src/components/` - UI components (chat, layout, primitives)

**terminal-harness** (`packages/terminal-harness`) - E2E Testing Infrastructure
- Web-based terminal testing using bun-pty + xterm.js + Playwright
- Provides automated testing of CLI in real terminal environment
- Key files:
  - `pty-server.ts` - HTTP server with WebSocket for PTY communication
  - `shell.html` - Browser-based terminal interface
  - `tests/` - Playwright test suites

### AWS Client Crates

- `amzn-qdeveloper-streaming-client` - Q Developer streaming API
- `amzn-codewhisperer-client` - CodeWhisperer API
- `amzn-codewhisperer-streaming-client` - CodeWhisperer streaming API
- `amzn-consolas-client` - Consolas profile/customization API
- `amzn-toolkit-telemetry-client` - Telemetry API

### Key Features by Location

- **Agent Management**: `chat_cli/src/cli/agent/mod.rs`
- **Conversation State**: `chat_cli/src/cli/chat/conversation.rs`
- **Tool Execution**: `agent/src/agent/task_executor/mod.rs`
- **MCP Integration**: `chat_cli/src/mcp_client/`, `agent/src/agent/mcp/`
- **Code Intelligence**: `code-agent-sdk/src/sdk/client.rs`
- **Knowledge Bases**: `chat_cli/src/util/knowledge_store.rs`
- **Authentication**: `chat_cli/src/auth/`
- **Checkpoints**: `chat_cli/src/cli/chat/checkpoint.rs`

## Testing

- Run single test: `cargo test -p chat_cli --bin chat_cli cli::chat::cli::persist::tests::test_save_and_load_file` or - `cargo test -p agent --lib test_mcp_server_config_stdio_deser`
- Run all tests in a module: `cargo test -p chat_cli --bin chat_cli persist::tests` (all tests in a module)

## Setup

After cloning the repository, run the setup script to install git hooks:

```bash
./scripts/setup-hooks.sh
```

This will install pre-commit hooks that run `cargo fmt` and `cargo clippy` checks before each commit.

## Common Commands

```bash
# Linting
cargo clippy --locked --workspace --color always -- -D warnings

# Formatting
cargo +nightly fmt
cargo +nightly fmt --check -- --color always

# Running
cargo run --bin chat_cli --
```

## Log Files

**macOS/Linux**: `$TMPDIR/kiro-log/kiro-chat.log` (or `$XDG_RUNTIME_DIR/kiro-log/kiro-chat.log`)
**Windows**: `%TEMP%/kiro-log/logs/kiro-chat.log`

MCP logs: Same directory, `mcp.log`

