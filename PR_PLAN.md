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

### Phase 2.5: Test Infrastructure Refactor
7. **MockLLM architecture refactor** - Move from single-actor to stateless per-turn model
   - *Current problem*: MockLLM uses old single-actor pattern, doesn't match real LLM behavior
   - *Solution*: Refactor to per-turn MockLLMContext with conversation history, streaming tx channel
   - *Test*: Multi-turn conversations work with clean test API (`read_user_message()`, `respond_to_user()`)
8. **ApiClient streaming integration** - Return streams instead of collecting vectors
   - *Current problem*: Mock LLM collects all events synchronously into `Vec<ChatResponseStream>`
   - *Solution*: Return proper streaming response like real LLM clients (CodeWhisperer/QDeveloper)
   - *Test*: Mock responses stream incrementally, match real LLM streaming behavior
9. **Actor test harness cleanup** - Align test infrastructure with new MockLLM model
   - *Dependencies*: Requires MockLLM refactor completion
   - *Test*: ACP actor tests work with new stateless MockLLMContext API

### Phase 3: Advanced Features
10. **Tool system integration** - Implement ACP tool permissions and execution through actors
    - *Test*: Tools work through session actors, permission requests flow correctly
    - *Note*: Session actors handle tool execution, report via ACP `ToolCall` messages
11. **File operation routing** - Replace builtin file tools with ACP versions in session actors
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

**✅ COMPLETED - Phase 2.5: Test Infrastructure Refactor**

The MockLLM system has been successfully refactored to work properly with the actor-based ACP system and match real LLM behavior patterns.

**Key Improvements Implemented:**
- **Stateless per-turn architecture**: Each user message spawns fresh MockLLMContext with full conversation history
- **Proper streaming integration**: ApiClient now returns streaming channels instead of collected vectors
- **Sophisticated pattern matching**: New regex-based conversation matching with named capture groups
- **Declarative test API**: Simple tuple-based pattern definitions replace complex imperative code
- **Proper error handling**: Result<Option<T>> types with clear error messages for regex compilation failures

**Completed Work:**
7. ✅ **MockLLM architecture refactor** - **COMPLETE**
   - **Achieved**: Stateless per-turn `MockLLMContext` with conversation history + streaming channels
   - **API**: `match_conversation()` with regex patterns, `try_patterns()` for declarative matching
   - **Benefits**: Matches real LLM behavior, enables proper actor system testing
8. ✅ **ApiClient streaming integration** - **COMPLETE** 
   - **Achieved**: Returns proper streaming `Receiver<Result<ChatResponseStream>>` like real LLM clients
   - **Fixed**: Performance regression from unnecessary `conversation.clone()` 
   - **Benefits**: True async streaming behavior, no more synchronous collection
9. ✅ **Actor test harness integration** - **COMPLETE**
   - **Achieved**: Clean integration with declarative `try_patterns()` API
   - **Compatibility**: All existing tests fixed with streaming helper functions
   - **Benefits**: 20+ lines of imperative pattern matching → 5 lines of declarative config

**New Declarative Test API:**
```rust
// Before: Complex imperative pattern matching
if ctx.match_and_respond(&[], r"(?i)hi,?\s+claude", "Hi, you! What's your name?").await? {
    return Ok(());
}
// After: Simple declarative patterns with automatic regex substitution
ctx.try_patterns(&[
    (&[], r"(?i)hi,?\s+claude", "Hi, you! What's your name?"),
    (&[r"assistant.*name"], r"(?P<name>\w+)", "Hi $name, I'm Q!"),
]).await
```

**Phase 3: Advanced Features**  
10. ⚠️ **Tool system integration** - Basic tool execution works, need ACP permissions
    - Current: Tool use shows as `[Tool execution]` placeholder
    - Missing: ACP `session/request_permission` flow, proper `ToolCall` messages
11. ⚠️ **File operation routing** - Need ACP file operations instead of direct filesystem
    - Current: Uses direct filesystem access
    - Missing: Route `fs_read`/`fs_write` through ACP protocol

**Minor TODOs:**
- Session configuration from ACP (currently uses defaults)
- Cancel operations implementation (currently no-op)
- Set session mode implementation (currently returns method not found)

**Current State:** The ACP server is **functionally complete** for basic chat functionality with **sophisticated test infrastructure**. Users can connect editors, create sessions, send prompts, and receive streaming AI responses. The actor architecture is solid and now has a stateless MockLLM system that matches real LLM behavior patterns, enabling comprehensive testing of the actor-based system with declarative pattern matching APIs.

**🔍 READY FOR REVIEW** 
The MockLLM refactor work is complete and ready for in-depth code review. Key areas for review:
- **MockLLM architecture** (`crates/chat-cli/src/mock_llm.rs`) - Stateless per-turn design with streaming
- **Pattern matching API** - Regex-based conversation matching with declarative `try_patterns()` 
- **ApiClient integration** - Streaming compatibility and performance fixes
- **Test compatibility** - Helper functions maintaining existing test functionality
- **Error handling** - Proper `Result<Option<T>>` types with clear error messages