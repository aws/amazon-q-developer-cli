# Long-Running Subagents in ACP Runtime

## Overview

Reimplement the subagent feature in the new ACP-based agent runtime with support for user-initiated background subagent sessions.

## Current State

The existing subagent implementation (`crates/chat-cli/src/agent/subagent.rs`):
- Blocks the main agent during execution
- Creates ephemeral `Agent` instances per task
- Subagent lifetime is tied to task completion
- Communication via `Summary` tool

## Goals

1. Maintain tool-call modality for inter-agent communication
2. Default to blocking behavior (current UX)
3. Allow user to move subagent to background mid-execution
4. Backgrounded subagents become long-running with independent lifetime
5. Each backgrounded subagent has its own `AcpSession`

## Design

### Two Execution Modes

| Mode | Trigger | Subagent Lifetime | Main Agent |
|------|---------|-------------------|------------|
| Blocking (default) | Tool call | Ephemeral - dies on completion | Waits for result |
| Background | User action during execution | Long-running - until user terminates | Receives "moved to background" result, continues |

### Subagent Registry

Only **backgrounded** subagents are tracked:

```rust
struct SubagentInfo {
    session_id: SessionId,
    agent_name: String,
    initial_query: String,
    status: SubagentStatus,
}

enum SubagentStatus {
    /// Subagent is actively working
    Working(String),
    
    /// Subagent completed current task, awaits further instruction
    AwaitingInstruction,
}
```

### Context Injection

Active (backgrounded) subagents are prepended to user messages:

```
--- ACTIVE SUBAGENTS ---
[session_id: abc123] agent: "code-reviewer" status: awaiting_instruction
  initial_query: "Review the authentication module for security issues"
[session_id: def456] agent: "test-writer" status: working("Running integration tests")
  initial_query: "Write integration tests for the payment service"
--- END ACTIVE SUBAGENTS ---
```

### Tool Result When Backgrounded

When user moves a subagent to background, the tool call returns:

```
User has moved subagent to background.
  session_id: abc123
  agent: "code-reviewer"
  initial_query: "Review the authentication module for security issues"

The subagent will continue working independently. 
You will be informed of its status in future messages.
Do not wait for this task - proceed with other work.
```

### ACP Extensions

#### `_session/terminate` (Notification)

Terminates a backgrounded subagent session. Sent from TUI to agent.

```typescript
// Method: "_session/terminate"
interface TerminateSessionNotification {
  sessionId: SessionId;
}
```

Agent behavior on receive:
1. Cancel any ongoing prompt in the session
2. Drop the `AcpSessionHandle`
3. Remove from subagent registry

#### `_session/background` (Notification)

User requests to move a blocking subagent to background. Sent from TUI to agent.

```typescript
// Method: "_session/background"
interface BackgroundSessionNotification {
  sessionId: SessionId;
}
```

Agent behavior on receive:
1. Register subagent in registry with `Working` status
2. Return tool result to main agent indicating backgrounded
3. Continue subagent execution independently

### Sequence: Blocking Subagent (Default)

```
Main Agent                    ACP Runtime                 Subagent
    │                              │                          │
    │ ── InvokeSubagents ─────────>│                          │
    │                              │ ── spawn session ───────>│
    │                              │ ── prompt ──────────────>│
    │    [main agent blocked]      │                          │
    │                              │ <── updates ─────────────│
    │                              │ <── response ────────────│
    │ <── tool_result (summary) ───│                          │
    │                              │ ── drop session ────────>│ [dropped]
```

### Sequence: User Moves to Background

```
Main Agent          TUI              ACP Runtime           Subagent
    │                │                    │                    │
    │ ── InvokeSubagents ────────────────>│                    │
    │                │                    │ ── spawn ─────────>│
    │                │                    │ ── prompt ────────>│
    │  [blocked]     │                    │                    │
    │                │ ── _session/background ─>│              │
    │                │                    │                    │
    │                │                    │ ── register in     │
    │                │                    │    subagent registry
    │                │                    │                    │
    │ <── tool_result ────────────────────│                    │
    │    "moved to background"            │                    │
    │                │                    │    [continues]     │
    │  [unblocked]   │                    │                    │
```

### Sequence: Terminating a Backgrounded Subagent

```
TUI                           ACP Runtime                 Subagent
 │                                 │                          │
 │ ── _session/terminate ─────────>│                          │
 │                                 │ ── cancel if working     │
 │                                 │ ── drop handle ─────────>│ [dropped]
 │                                 │ ── remove from registry  │
```

## Implementation Tasks

- [x] 1. Define extension types (`_session/terminate`, `_session/background`)
- [x] 2. Add subagent info to session handle (for context injection)
- [x] 3. Handle `_session/terminate` notification
- [x] 4. Handle `_session/background` notification (marks session; unblocking tool is task 6)
- [x] 5. Implement subagent tool in new ACP agent runtime
  - Tool types in `crates/agent/src/agent/tools/spawn_subagent.rs`
  - Event handling in `crates/chat-cli/src/agent/acp/acp_agent.rs`
  - Uses `AgentEvent::SpawnSubagentRequest` with oneshot response channel
- [x] 6. Implement blocking subagent session spawning and execution
  - `ResourceProvider` actor vends `Os` on demand (avoids cloning for every prompt)
  - `AcpSessionHandle::internal_prompt` for subagent execution without ACP connection
  - Concurrent execution via `futures::future::join_all`
  - Summary failsafe handling (prompts subagent if no summary before EndTurn)
- [ ] 6b. Support backgrounding mid-execution (interrupt blocking wait, return early)
- [ ] 6c. Incorporate model_id in subagent spawning (use model from agent config or allow override)
- [ ] 7. Inject active subagent context into user messages
- [ ] 8. TUI: Add "move to background" action (separate task)
- [ ] 9. TUI: Add "terminate subagent" action (separate task)

### Task 5 Implementation Note

The subagent tool is fully wired up:
- `crates/agent/src/agent/tools/spawn_subagent.rs` - Tool definition and execution
- `crates/chat-cli/src/agent/acp/acp_agent.rs` - Event handler in ACP layer

The tool emits `AgentEvent::SpawnSubagentRequest` which the ACP layer intercepts and handles.

### Task 6 Implementation Note

Blocking subagent execution is implemented:
- `ResourceProvider` actor holds `Os` and vends clones only when needed
- `AcpSessionRequest::InternalPrompt` variant for subagent prompts (no `JrConnectionCx` needed)
- `handle_internal_prompt` waits for `SubagentSummary` event, with failsafe if subagent ends without summary
- Subagents run concurrently via `join_all`, results aggregated and returned

Backgrounding (task 6b) requires additional work:
- Need cancellation mechanism to interrupt `internal_prompt` when `_session/background` received
- Must track active subagent sessions by ID to route background notifications
- Return "backgrounded" result to main agent while subagent continues independently
