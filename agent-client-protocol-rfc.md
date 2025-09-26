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
┌─────────────┐    JSON-RPC      ┌─────────────┐    Actor Messages   ┌───────────────┐
│   Editor    │ ◄──────────────► │AcpAgentForward│ ◄─────────────────► │ AcpServerActor│
│ (Zed, etc.) │     (stdio)      │             │                     │               │
└─────────────┘                  └─────────────┘                     └───────────────┘
                                                                             │
                                                                    Actor Messages
                                                                             │
                                                                             ▼
                                                                   ┌───────────────┐
                                                                   │AcpSessionActor│
                                                                   │ (per session) │
                                                                   └───────────────┘
```

The ACP implementation uses Alice Ryhl's actor pattern for clean separation of concerns:

- **AcpAgentForward**: Thin forwarding layer implementing `acp::Agent` trait
- **AcpServerActor**: Top-level coordinator managing sessions and routing messages  
- **AcpSessionActor**: Per-session actors that own `ConversationState` and process prompts

**Key Benefits:**
- **No shared state**: Each actor owns its data (eliminates RwLocks and contention)
- **Natural backpressure**: Bounded channels prevent unbounded message queuing
- **Clean separation**: Protocol handling, session management, and conversation processing are separate
- **Easy testing**: Each actor can be tested independently with message injection

**Message Flow:**
When an ACP client sends a prompt:
1. `AcpAgentForward` receives JSON-RPC request
2. Forwards as `ServerMethod::Prompt` message to server actor via channel
3. Server actor routes as `SessionMethod::Prompt` to appropriate session actor
4. Session actor processes with `ConversationState` and streams responses back
5. Response flows back through the same channel hierarchy to ACP client

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

The implementation uses Alice Ryhl's actor pattern for clean message passing instead of shared state with RwLocks. Implementation proceeds in commit-sized units:

### Phase 1: Actor Foundation
1. **Actor pattern foundation** - Implement `AcpAgentForward`, `AcpServerHandle`, and `AcpSessionHandle` with message types
   - *Test*: Actor handles can be created, messages can be sent (stub responses)
   - *Note*: Uses `mpsc::channel(32)` for bounded message passing, `oneshot::channel()` for responses
2. **Basic command structure** - Add `q acp` subcommand with feature gating and actor system integration
   - *Test*: `q acp` command spawns actor system, handles `initialize` requests
   - *Note*: Uses `LocalSet` for !Send ACP futures, stdio transport with `AgentSideConnection`
3. **Server actor implementation** - Implement server actor loop with session management
   - *Test*: Can handle multiple ACP method calls, routes to appropriate handlers
   - *Note*: Server actor maintains `HashMap<SessionId, AcpSessionHandle>` for session routing

### Phase 2: Session Management  
4. **Session lifecycle** - Implement `new_session` and `load_session` with session actor spawning
   - *Test*: Can create sessions, spawn session actors, store session IDs
   - *Note*: Each session actor owns its `ConversationState` and `ToolManager` instance
5. **Basic prompt handling** - Implement session actor prompt processing (stub LLM responses)
   - *Test*: Can send prompts to sessions, session actors receive and respond
6. **Response streaming** - Wire up real LLM integration with ACP streaming notifications
   - *Test*: Prompts return actual AI responses, streaming works through actor system

### Phase 2.5: Test Infrastructure
7. **Actor test harness** - Adapt existing test harness to work with new actor system
   - *Test*: Can test actor system with mock LLMs, conversational test scripts work
   - *Note*: Test harness creates actors in-process, injects mock LLM responses
8. **Mock LLM integration** - Ensure mock LLM system works with session actors
   - *Test*: Mock LLM scripts can control session actor responses deterministically

### Phase 3: Advanced Features
9. **Tool system integration** - Implement ACP tool permissions and execution through actors
   - *Test*: Tools work through session actors, permission requests flow correctly
   - *Note*: Session actors handle tool execution, report via ACP `ToolCall` messages
10. **File operation routing** - Replace builtin file tools with ACP versions in session actors
    - *Test*: `fs_read`/`fs_write` work through editor, session actors route file operations
    - *Note*: Session actors use ACP file operations instead of direct filesystem access

**Architecture Benefits:**
- **Eliminates RwLocks**: No shared mutable state, each actor owns its data
- **Natural backpressure**: Bounded channels prevent memory issues under load  
- **Clean testing**: Each actor can be tested in isolation with message injection
- **Incremental development**: Can implement and test each actor independently

## Current Implementation Status

**✅ COMPLETED - Phase 1: Actor Foundation**
1. ✅ **Actor pattern foundation** - Complete actor system with message types
   - `AcpAgentForward`, `AcpServerHandle`, `AcpSessionHandle` implemented
   - Bounded channels (`mpsc::channel(32)`) with `oneshot` responses
   - Proper error propagation (`eyre::Result` internal, `acp::Error` protocol)
2. ✅ **Basic command structure** - `q acp` subcommand with actor integration  
   - Feature gating, `LocalSet` for !Send futures, stdio transport
   - `AgentSideConnection` integration working
3. ✅ **Server actor implementation** - Complete server actor with session routing
   - Session management with `HashMap<SessionId, AcpSessionHandle>`
   - Method routing for `initialize`, `new_session`, `load_session`, etc.

**✅ COMPLETED - Phase 2: Session Management**
4. ✅ **Session lifecycle** - Session creation and actor spawning working
   - `new_session` creates session actors with unique IDs
   - Each session actor owns its `ConversationState` and `ToolManager`
5. ✅ **Basic prompt handling** - Session actors process prompts correctly
   - Convert ACP prompts to Q CLI format, set in conversation state
6. ✅ **Response streaming** - Full LLM integration with streaming notifications
   - Real `SendMessageStream` integration, `ResponseEvent` → ACP conversion
   - Streaming `AssistantText`, `ToolUseStart`, `ToolUse` events via transport

**🚧 REMAINING WORK**

**Phase 2.5: Test Infrastructure**
7. ⚠️ **Actor test harness** - Need to adapt existing test infrastructure
8. ⚠️ **Mock LLM integration** - Ensure mock LLM works with session actors

**Phase 3: Advanced Features**  
9. ⚠️ **Tool system integration** - Basic tool execution works, need ACP permissions
   - Current: Tool use shows as `[Tool execution]` placeholder
   - Missing: ACP `session/request_permission` flow, proper `ToolCall` messages
10. ⚠️ **File operation routing** - Need ACP file operations instead of direct filesystem
    - Current: Uses direct filesystem access
    - Missing: Route `fs_read`/`fs_write` through ACP protocol

**Minor TODOs:**
- Session configuration from ACP (currently uses defaults)
- Cancel operations implementation (currently no-op)
- Set session mode implementation (currently returns method not found)

**Current State:** The ACP server is **functionally complete** for basic chat functionality. Users can connect editors, create sessions, send prompts, and receive streaming AI responses. The actor architecture is solid and ready for the remaining advanced features.

