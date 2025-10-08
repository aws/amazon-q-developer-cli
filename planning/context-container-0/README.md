# ContextContainer Implementation - Planning Documents

## Overview

This directory contains the design and implementation plan for adding `ContextContainer` to the Worker struct in the Agent Environment Architecture.

## Goal

Enable Workers to maintain conversation history and other contextual information, moving away from passing prompts as task inputs toward a model where context is managed within the Worker itself.

## Key Change

**Before**:
```rust
let input = AgentLoopInput { prompt: "hello" };
session.run_agent_loop(worker, input, ui)?;
```

**After**:
```rust
worker.context_container.conversation_history
    .lock().unwrap()
    .push_input_message("hello".to_string());
session.run_agent_loop(worker, AgentLoopInput {}, ui)?;
```

## Documents

### 1. [design.md](./design.md)

Complete design document covering:
- Architecture overview
- Core types (ConversationEntry, ConversationHistory, ContextContainer)
- Integration points with Worker and AgentLoop
- Design decisions and rationale
- Future expansion plans

**Read this first** to understand the overall approach.

### 2. [implementation-plan.md](./implementation-plan.md)

Step-by-step implementation guide with:
- 5 phases of implementation
- Detailed code snippets for each step
- Testing strategy
- Rollout checklist
- Known issues and workarounds

**Use this** as the execution guide.

### 3. [api-reference.md](./api-reference.md)

Complete API documentation including:
- Type definitions and methods
- Usage patterns and examples
- Thread safety considerations
- Migration guide from old pattern
- Testing examples

**Reference this** when implementing or using the API.

## Quick Start

To implement this design:

1. Read [design.md](./design.md) to understand the architecture
2. Follow [implementation-plan.md](./implementation-plan.md) phase by phase
3. Refer to [api-reference.md](./api-reference.md) for specific API details

## Implementation Phases

1. **Phase 1**: Create context types (no breaking changes)
2. **Phase 2**: Add ContextContainer to Worker (breaking change)
3. **Phase 3**: Update AgentLoop to use context (breaking change)
4. **Phase 4**: Update demo
5. **Phase 5**: Remove prompt from AgentLoopInput (optional)

## Key Design Principles

1. **Minimal abstraction**: Reuse existing types (UserMessage, AssistantMessage)
2. **Two-phase commit**: Stage input message, then commit with assistant response
3. **Thread-safe**: Use Arc<Mutex<>> for interior mutability
4. **Extensible**: ContextContainer designed for future additions
5. **No persistence**: Keep in memory only (for now)
6. **Worker-to-worker communication**: Use "input_message" terminology (not "user_message") since workers can spawn other workers
7. **Orchestration support**: `commit_turn()` works without staged input for monitoring/orchestration tasks

## Dependencies

The implementation reuses existing types from:
- `crates/chat-cli/src/cli/chat/conversation.rs`
  - `UserMessage`
  - `AssistantMessage`
  - `ToolUse`
  - `ToolUseResult`

## Key Changes from Original Design

1. **Module path**: `context_container/` instead of `context/` to avoid naming collisions
2. **Field order**: `context_container` placed after `name` in Worker struct (most critical data)
3. **Terminology**: `input_message` instead of `user_message` (workers can spawn workers)
4. **Commit behavior**: `commit_turn()` doesn't require staged input (supports orchestration tasks)
5. **Model provider**: Uses `Arc<dyn ModelProvider>` instead of concrete type

## Testing

- Unit tests for ConversationHistory operations
- Integration tests with AgentLoop
- Manual testing with demo

## Future Enhancements

After initial implementation:
- Tool result support
- Metadata tracking (timestamps, token counts)
- History trimming/summarization
- Serialization for persistence
- Sticky context files
- Helper methods on Worker for ergonomics
- Lock-free alternatives to Arc<Mutex<>>
- Conversation branching/checkpointing

## Questions?

Refer to the "Open Questions" section in [design.md](./design.md) for unresolved design decisions.

## Status

- [x] Design complete
- [x] Implementation plan complete
- [x] API reference complete
- [ ] Implementation started
- [ ] Phase 1 complete
- [ ] Phase 2 complete
- [ ] Phase 3 complete
- [ ] Phase 4 complete
- [ ] Phase 5 complete
- [ ] Testing complete
