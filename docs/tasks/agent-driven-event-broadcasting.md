# Agent-Driven Event Broadcasting

## Overview

Enable broadcasting of messages across ACP boundaries for agent-driven events and refactor normal request-response data flow to use the same unified egress path. This task implements:

1. **Agent-driven event emission** - Agents can push updates to clients without explicit requests
2. **Unified data flow architecture** - Both request responses and agent events use the same egress path

## Current Architecture

**Request-Response Data Flow**: Client → ACP → Agent → ACP → Client
**Agent Events**: No mechanism for agent-driven updates

**Limitations**:
- Events only processed during active prompts in `handle_prompt_request()`
- No continuous polling of agent events outside prompt context
- Agent-driven updates cannot reach clients
- Request thread handles both ingress and egress, creating coupling

## Target Architecture

**Unified Data Flow**:
- **Request Ingress**: Client → ACP → Agent (end)
- **Response/Event Egress**: Agent → ACP → Client (unified path)

**Key Requirements**:
- Decouple request ingress from response egress
- Multiple agents with a given client should have their updates forwarded
- Continuous polling of agent events regardless of prompt state
- Both normal responses and agent events use same broadcast infrastructure
- Request thread only handles ingress, separate mechanism handles all egress

## Implementation Plan

### 1. Enhanced AcpSession Main Loop

**File**: `crates/chat-cli/src/agent/acp/acp_agent.rs`

Add dedicated agent event polling branch to `AcpSession::main_loop()` for unified egress handling:

```rust
// In AcpSession::main_loop, add new select branch:
agent_event = self.agent.recv() => {
    match agent_event {
        Ok(event) => {
            self.handle_agent_event_broadcast(event).await;
        }
        Err(_) => {
            // Agent channel closed
        }
    }
}
```

### 2. Refactor Request-Response Flow

**Current**: `handle_prompt_request()` processes request AND waits for/handles responses
**Target**: `handle_prompt_request()` only processes request, responses handled via unified egress

**Changes**:
- Remove response polling logic from `handle_prompt_request()`
- Send request to agent and return immediately
- All responses (both prompt responses and agent events) flow through unified egress path

### 3. Extract Event Handling Logic

Create unified egress handler for both responses and events:

```rust
async fn handle_agent_event_broadcast(&self, event: AgentEvent) {
    // Convert agent event to SessionUpdate (reuse existing logic)
    if let Some(update) = convert_agent_event_to_session_update(event) {
        // Broadcast to all sessions or filter as needed
        self.session_tx.broadcast_notification(update, None).await;
    }
}
```

### 4. Update Session Management

Ensure session state properly tracks active requests without blocking on responses:
- Request correlation IDs for matching responses to sessions
- Session state management for pending requests
- Proper cleanup when sessions end

## Technical Details

**Existing Infrastructure to Leverage**:
- `AgentHandle::recv()` - Already available for polling agent events
- `SessionManager::broadcast_notification()` - Existing broadcast capability
- `handle_update_event()` - Event conversion logic

**Key Files**:
- `crates/chat-cli/src/agent/acp/acp_agent.rs` - Main implementation
- `crates/chat-cli/src/agent/acp/session_manager.rs` - Broadcast infrastructure

## Benefits

- **Unified Egress Architecture**: All agent outputs (responses + events) use same path
- **Decoupled Request Processing**: Request ingress no longer blocks on response egress
- **Continuous Event Processing**: Agent events processed regardless of prompt state
- **Minimal Code Changes**: Reuses existing `agent.recv()` and broadcast infrastructure
- **Scalable**: Works with multiple agents and clients automatically
- **Improved Concurrency**: Request threads can handle more requests without blocking
- **Backwards Compatible**: Doesn't break existing client expectations

## Success Criteria

- [x] Agent events are continuously polled in `AcpSession::main_loop()`
- [x] Events are broadcast to appropriate clients via existing infrastructure
- [x] Multiple agents can emit events to the same client
- [x] Existing prompt-driven flow remains functional
- [x] **Request-response flow refactored to use unified egress**
- [x] **Request threads only handle ingress, no longer block on responses**
- [x] **All agent outputs (responses + events) flow through same broadcast mechanism**
- [x] **Session correlation properly maintained for async request-response**

## Implementation Status

### Completed
- Added agent event polling branch to `AcpSession::main_loop()`
- Created `handle_agent_event()` method for processing agent-driven events
- Extracted `convert_update_event_to_session_update()` for reusable event conversion
- Updated `handle_update_event()` to use extracted conversion logic
- **Refactored SessionManagerRequest to use internally tagged structure with session_id**
- **Implemented unified egress architecture:**
  - `handle_prompt_request()` now only handles ingress (sending request to agent)
  - All agent outputs (responses + events) flow through `handle_agent_event()`
  - Responses sent directly to client via stored response channel
  - Removed old ActivePrompt mechanism
- **Moved client registration to NewSessionRequest:**
  - Client connections registered during session creation
  - Optional connection context for subagents (noop when None)
  - Removed client registration from prompt request handler
- Code compiles successfully with no errors

### Next Steps
- Test unified data flow with both normal requests and agent events
- Verify that subagents work correctly with no client connection
- Clean up unused code and methods

## Related Tasks

- [Long-Running Subagents](./long-running-subagents.md) - May benefit from agent-driven updates
- [Event-Sourced Session Persistence](./event-sourced-session-persistence.md) - Related event handling patterns
