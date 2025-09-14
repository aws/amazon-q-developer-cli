# RFC: Agent Client Protocol Integration for Amazon Q CLI

- **Feature Name**: `acp`
- **Start Date**: 2025-09-14
- **RFC PR**: (TBD)
- **Amazon Q Issue**: (TBD)

## Summary

Add Agent Client Protocol (ACP) server capability to Amazon Q CLI, allowing editors like Zed and Neovim to use Q as an AI coding assistant through a standardized JSON-RPC interface.

**What is ACP?** Agent Client Protocol is a JSON-RPC standard that lets editors communicate with AI agents over stdio. Instead of building custom integrations for each editor, agents implement ACP once and work with any ACP-compatible editor.

**User Experience:** Users run `q acp` to start Q in server mode, then configure their editor to connect to this process. The editor handles the UI while Q provides the AI capabilities - same models, tools, and features as `q chat`.

## Motivation

**Problem:** Currently Q CLI provides two options to users: an interactive, CLI-based chat interface and a non-interactive mode. But some use-cases demand interaction but in a programmatic or scripted fashion. This includes custom GUI in editors, automation tools, IDEs, web interfaces, and other applications. But right now each application must adapt to each agent independently. This means applications are likely only to build on the most widely used alternatives (e.g., with the Claude Code SDK, which provides programmatic access to the Claude Code agent).

**Solution:** ACP provides an alternative, using a JSON-RPC protocol inspired by MCP to let any application integrate with any agent, sending user input and receiving the agent's responses in a streaming fashion.

**Immediate Benefits:** This provides immediate value to Q CLI users by allowing them to access Q from editors that support ACP (Zed, Neovim) with native integration - same models, tools, and MCP servers, but in their preferred editor instead of switching to terminal.

**Strategic Benefits:** Supporting ACP also helps boost the protocol itself. The more editors and agents use ACP, the more likely it will succeed as a standard. This avoids the problem of tooling being built atop proprietary options like the Claude Code SDK, which would lock Q CLI out of future editor integrations.

## Guide-level Explanation

**Setup:**
```bash
# Start Q in ACP server mode
q acp --agent my-profile

# Configure editor to connect to this process
# (Editor-specific configuration)
```

**Usage:** Once connected, users interact with Q through their editor's AI interface:
- Chat with Q in editor panels/sidebars
- Q can read/write files through the editor (sees unsaved changes)
- Tool execution with editor-native permission prompts
- Same models, agents, and MCP servers as terminal Q CLI

**JSON-RPC Communication:** Editor and Q communicate over stdio using structured messages:
```json
// Editor → Q: User sends a message
{"method": "session/prompt", "params": {"sessionId": "123", "messages": [...]}}

// Q → Editor: Streaming response
{"method": "session/update", "params": {"sessionId": "123", "update": {"AgentMessageChunk": {...}}}}
```

## Reference-level Explanation

### Architecture Overview

```
┌─────────────┐    JSON-RPC      ┌─────────────┐    Internal APIs    ┌───────────────┐
│   Editor    │ ◄──────────────► │ ACP Adapter │ ◄─────────────────► │  Q Chat       │
│ (Zed, etc.) │     (stdio)      │             │                     │ Infrastructure│
└─────────────┘                  └─────────────┘                     └───────────────┘
```

The ACP adapter acts as a protocol translator, mapping between ACP concepts and Q's internal chat system.

### Mapping ACP to Q CLI

**Core Concept Mappings:**
- **ACP Sessions** → **Q Conversation IDs** (same UUID)
- **ACP Prompts** → **Q Chat pipeline** (reuse existing processing)
- **ACP Streaming** → **Q Response streaming** (protocol translation)
- **ACP Tool calls** → **Q Tool system** (existing infrastructure)
- **ACP Permissions** → **Q Agent permissions** (existing flow)

**Non-trivial Integration Areas:**

#### Creating and loading sessions

The ACP session lifecycle has four key points:

* `initialize` message -- ACP connection setup
   * In Q CLI, we would check that the user is logged in and report back with our capabilities.
* `session/new` -- create a new conversation
   * In Q CLI, we will create a new `SessionId` (a UUID) and a fresh `ConversationState` and add it into the SQLite database.
   * This will use the agent configured with `q acp --agent XXX` (or default agent).
   * This will use the ACP versions of `fs_read`, `fs_write`, and other file system operations.
   * ACP allows the caller to provide a list of MCP servers, these will be **added to** the agent's base MCP configuration (additive approach).
   * The working directory is specified in the ACP message and stored per session.
   * Each session gets its own `ToolManager` instance with session-specific MCP servers.
* `session/load_session` -- resume a session
   * In Q CLI, we will fetch the `ConversationState` from the database and recreate the session context.

#### Receiving user prompts

* `session/prompt` -- incoming user message
   * A new Q CLI `UserMessage` will be created and added into the conversation state.
   * The chat request will be sent to the backend server and responses streamed back via ACP `session/update` notifications.

#### File Operations

File operations using ACP do not go directly to the file system.
Instead they are directed over the protocol so that the editor can provide for a simulated file system.

When using ACP, Q CLI will provide different implementations of `fs_read`, `fs_write`, and other builtin file tools that target the operating system.
The parameters accepted by the tools do not have to be the same but we should be able to match them.

#### Tool Use

When using ACP, Q CLI will request permission to use a tool by sending a `session/request_permission` to the client and awaiting the response. Trusted tools will not trigger this flow.

When tool use occurs, Q CLI will report that tool use using the ACP `ToolCall` messages. ACP's messages provide for more distinctions than those made by Q CLI today (e.g., distinguishing file operations from shell, and allowing output that is both plain text but also can include diffs). We will begin with a simple mapping and then add custom logic for specific tools (e.g., `fs_write` can product diffs, `bash` can produce shell output).

#### Response Streaming
Q CLI will convert its existing streaming responses to ACP `session/update` notifications as the model generates content. Text responses and code blocks will be sent as `AgentMessageChunk` updates, allowing the editor to display them incrementally. Tool execution will be reported through `ToolCall` messages when tools start and `ToolCallUpdate` messages as they progress and complete (as described in the [Tool Use](#tool-use) section previously). This preserves Q CLI's existing streaming behavior while adapting it to ACP's notification model.

### Feature Gating

Following Q CLI's established pattern, ACP functionality will be feature-gated:

```rust
// In database/settings.rs
#[strum(message = "Enable Agent Client Protocol server (boolean)")]
EnabledAcp,

// Runtime check pattern
if !os.database.settings.get_bool(Setting::EnabledAcp).unwrap_or(false) {
    eprintln!("ACP is disabled. Enable with: q settings acp.enabled true");
    return Ok(ExitCode::FAILURE);
}
```

This allows:
- **Controlled rollout** during development and testing
- **User opt-in** for ACP functionality
- **Consistent patterns** with other Q CLI features (tangent mode, todo list, etc.)

## Implementation Plan

The implementation will proceed in commit-sized units, leveraging the existing `agent-client-protocol` Rust crate:

### Phase 1: Foundation
1. **Add ACP dependency** - Include `agent-client-protocol` crate in Q CLI's Cargo.toml
   - *Test*: Verify crate compiles and imports work
2. **Basic command structure** - Add `q acp` subcommand with feature gating and argument parsing (initially just prints status and exits)
   - *Test*: `q acp` command exists, respects feature flag, shows help text
   - *Note*: Use same pattern as `q chat --agent` for agent selection; add tests for both enabled/disabled feature flag states using `os.database.settings.set(Setting::EnabledAcp, bool)`
3. **Minimal Agent implementation** - Create stub `Agent` trait implementation that handles `initialize` only
   - *Test*: ACP client can connect, send `initialize`, get valid response
   - *Note*: Use ACP library's `AgentSideConnection::new` with stdio transport; test with library's example client or `ClientSideConnection::new`

### Phase 2: Session Management
4. **Session lifecycle foundation** - Implement `new_session` and `load_session` methods with Q's conversation state integration
   - *Test*: Can create sessions, session IDs stored in database, can reload existing sessions
   - *Note*: Each ACP session gets its own `ToolManager` instance (following existing pattern where each `ConversationState` has its own `ToolManager`); session-specific MCP servers are additive to agent's base configuration
5. **Basic prompt handling** - Implement `prompt` method to create UserMessages and add to conversation state (returns empty responses)
   - *Test*: Can send prompts to sessions, messages stored in conversation history
6. **Response streaming foundation** - Wire up basic text response streaming using ACP's `session/update` notifications
   - *Test*: Prompts return actual AI responses, streaming works in ACP client

### Phase 3: Advanced Features  
7. **Tool system integration** - Implement ACP permission requests and tool execution reporting
   - *Test*: Tools require permission, execution reported correctly, trusted tools bypass permission
   - *Note*: Q's trusted tools skip `session/request_permission` entirely; only untrusted tools trigger permission flow; permissions are per-tool-call with options like "allow once", "reject"
8. **File operation routing** - Replace builtin file tools with ACP versions that route through the protocol
   - *Test*: `fs_read`/`fs_write` work through editor, see unsaved changes, respect editor's file system view
   - *Note*: ACP file operations use absolute paths (e.g., `/home/user/project/src/main.py`); completely replace Q's builtin `fs_read`/`fs_write` tools with ACP versions that call client's `read_text_file`/`write_text_file` methods instead of direct filesystem access

Each phase builds on the previous, with concrete testability at every step using the ACP library's example client or compatible editors.

