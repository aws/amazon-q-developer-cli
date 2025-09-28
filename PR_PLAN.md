# ACP Server Implementation Plan

This document tracks implementation progress for the Agent Client Protocol (ACP) server integration. It will be removed before the PR lands.

## Implementation Plan

The implementation uses an actor-based pattern for clean message passing instead of shared state with RwLocks. Implementation proceeds in commit-sized units:

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