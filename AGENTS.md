## Codebase Overview

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

