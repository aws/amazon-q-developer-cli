# ACP Session Cancellation Implementation

## Overview

Implement proper session cancellation support in the Agent Client Protocol (ACP) implementation to handle `session/cancel` notifications from clients. This ensures ACP compliance and provides users with the ability to interrupt ongoing agent operations.

## Current Architecture

**Agent Cancellation**: The `AgentHandle::cancel()` method exists and properly cancels ongoing operations (tool execution, model requests) and sets the agent to idle state.

**Session Management**: The `SessionManager` handles communication between ACP clients and agent sessions, including tool approval requests.

**Limitations**:
- No handling of `CancelNotification` in the ACP agent
- Pending `session/request_permission` requests are not responded to with cancelled outcome
- Agent cancellation doesn't propagate proper ACP-compliant responses

## Target Architecture

**ACP-Compliant Cancellation Flow**:
1. Client sends `CancelNotification`
2. Agent cancels ongoing operations via existing `AgentHandle::cancel()`
3. All pending `session/request_permission` requests receive cancelled responses
4. Original `session/prompt` request returns with `StopReason::Cancelled`

**Key Requirements**:
- Handle `CancelNotification` at the ACP protocol level
- Ensure all pending permission requests are responded to with cancelled outcome
- Return proper `cancelled` stop reason for prompt requests
- Handle race conditions between cancellation and ongoing operations
- Maintain existing agent cancellation behavior

## Implementation Plan

### 1. Add CancelNotification Handler

**File**: `crates/chat-cli/src/agent/acp/acp_agent.rs`

Add `CancelNotification` handling to the SACP agent builder in the `execute()` function:

```rust
.on_receive_message(
    {
        let session_tx = session_manager_handle.clone();
        async move |message: MessageCx, _cx: JrConnectionCx<AgentToClient>| {
            if let MessageCx::Notification(notif) = &message {
                let method = notif.method();
                
                // Handle cancel notifications using proper SACP types
                if method == AGENT_METHOD_NAMES.session_cancel {
                    if let Ok(cancel_notif) = serde_json::from_value::<CancelNotification>(notif.params().clone()) {
                        session_tx.cancel_session(&cancel_notif.session_id).await;
                        return Ok(sacp::Handled::Yes);
                    }
                }
                
                // Handle existing extension notifications
                if method == methods::SESSION_TERMINATE
                    && let Ok(params) = serde_json::from_value::<TerminateSessionNotification>(notif.params().clone())
                {
                    session_tx.terminate_session(&params.session_id).await;
                    return Ok(sacp::Handled::Yes);
                }
            }
            Ok(sacp::Handled::No { message, retry: false })
        }
    },
    sacp::on_receive_message!(),
)
```

### 2. Extend Session Manager for Cancellation

**File**: `crates/chat-cli/src/agent/acp/session_manager.rs`

Add cancellation support to the session manager:

```rust
// Add to SessionManagerRequestData enum
#[derive(Debug)]
pub(crate) enum SessionManagerRequestData {
    // ... existing variants
    CancelSession,
}

// Add to SessionManagerHandle
impl SessionManagerHandle {
    pub async fn cancel_session(&self, session_id: &SessionId) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::CancelSession,
            })
            .await;
    }
}

// Add to SessionManager::handle_request
async fn handle_request(&mut self, request: SessionManagerRequest) {
    match data {
        // ... existing cases
        SessionManagerRequestData::CancelSession => {
            if let Some(session_handle) = self.sessions.get(&session_id) {
                // Cancel the session
                let _ = session_handle.cancel().await;
                
                // Cancel any pending approval requests for this session
                self.cancel_pending_approvals(&session_id).await;
            }
        }
    }
}
```

### 3. Update AcpSession for Cancel Handling

**File**: `crates/chat-cli/src/agent/acp/acp_agent.rs`

Enhance `AcpSession` to handle cancellation properly:

```rust
// Add cancel request type
#[derive(Debug)]
pub enum AcpSessionRequest {
    // ... existing variants
    Cancel,
}

// Update AcpSession to track cancellation state
struct AcpSession {
    // ... existing fields
    is_cancelled: Arc<AtomicBool>,
}

// Add cancel handling to handle_request
async fn handle_request(&mut self, req: AcpSessionRequest) {
    match req {
        // ... existing cases
        AcpSessionRequest::Cancel => {
            self.is_cancelled.store(true, Ordering::Relaxed);
            
            // Cancel the underlying agent
            if let Err(e) = self.agent.cancel().await {
                error!("Failed to cancel agent: {}", e);
            }
            
            // If there's a pending prompt response, respond with cancelled
            if let Some(respond_to) = self.pending_prompt_response.take() {
                let respond_to = respond_to.into_inner();
                let _ = respond_to.respond(PromptResponse::new(StopReason::Cancelled));
            }
        }
    }
}

// Update agent event handling to check for cancellation
async fn handle_agent_event(&mut self, event: AgentEvent) {
    // Check if session was cancelled
    if self.is_cancelled.load(Ordering::Relaxed) {
        match event {
            AgentEvent::EndTurn(_) | AgentEvent::Stop(_) => {
                // Override with cancelled response if we have a pending prompt
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    let respond_to = respond_to.into_inner();
                    let _ = respond_to.respond(PromptResponse::new(StopReason::Cancelled));
                    return;
                }
            }
            _ => {}
        }
    }
    
    // ... existing event handling
}
```

### 4. Handle Pending Permission Requests

**File**: `crates/chat-cli/src/agent/acp/session_manager.rs`

Add mechanism to cancel pending approval requests:

```rust
// Add to SessionManager to track pending approvals
impl SessionManager {
    // Track pending approval requests by session
    pending_approvals: HashMap<SessionId, Vec<PendingApproval>>,
    
    async fn cancel_pending_approvals(&mut self, session_id: &SessionId) {
        if let Some(pending) = self.pending_approvals.remove(session_id) {
            for approval in pending {
                // Send cancelled response to each pending approval
                if let Some(cx) = self.client_connection.as_ref() {
                    // This would require modifying handle_approval_request to support cancellation
                    // The exact implementation depends on how we track pending requests
                }
            }
        }
    }
}

// Update handle_approval_request to handle cancellation
async fn handle_approval_request(
    id: String,
    tool_use: ToolUseBlock,
    agent: AgentHandle,
    cx: JrConnectionCx<AgentToClient>,
    session_id: SessionId,
) -> Result<(), sacp::Error> {
    // ... existing permission request logic
    
    let response = cx
        .send_request(RequestPermissionRequest::new(/* ... */))
        .block_task()
        .await;

    let approval_result = match response {
        Ok(resp) => match resp.outcome {
            sacp::schema::RequestPermissionOutcome::Cancelled => {
                agent::protocol::ApprovalResult::Deny {
                    reason: Some("Operation was cancelled".to_string()),
                }
            }
            // ... existing cases
        },
        Err(_) => {
            // Handle cancellation during request
            agent::protocol::ApprovalResult::Deny {
                reason: Some("Request was cancelled".to_string()),
            }
        }
    };

    // ... rest of existing logic
}
```

### 5. Add Session Handle Cancel Method

**File**: `crates/chat-cli/src/agent/acp/acp_agent.rs`

Add cancel method to `AcpSessionHandle`:

```rust
impl AcpSessionHandle {
    pub async fn cancel(&self) -> Result<(), sacp::Error> {
        self.tx.send(AcpSessionRequest::Cancel).await
    }
}
```

### 6. Import Required Types

**File**: `crates/chat-cli/src/agent/acp/acp_agent.rs`

Add the required imports for proper SACP types:

```rust
use sacp::schema::{
    // ... existing imports
    CancelNotification,
    StopReason,
    AGENT_METHOD_NAMES,
    // ... rest of imports
};
```

## Testing Strategy

### Unit Tests
- Test `CancelNotification` parsing and routing
- Test cancellation of ongoing agent operations
- Test proper response to pending permission requests with `RequestPermissionOutcome::Cancelled`
- Test race conditions between cancel and completion

### Integration Tests
- Test full cancellation flow from client notification to agent response
- Test cancellation during different agent states (idle, executing tools, waiting for approval)
- Test multiple rapid cancellations
- Test cancellation of subagent operations

### Edge Cases
- Cancellation after operation completion
- Multiple cancellation requests for same session
- Cancellation during agent initialization
- Network failures during cancellation

## Implementation Notes

### Race Condition Handling
- Use atomic flags to track cancellation state
- Ensure cancellation responses take precedence over normal completion
- Handle cases where cancellation arrives after operation completion

### Error Handling
- Log cancellation failures but don't propagate as errors to client
- Ensure cancellation always results in proper ACP response
- Handle partial cancellation scenarios gracefully

### ACP Compliance
- Always respond to `session/prompt` with `StopReason::Cancelled` when cancelled
- Ensure all pending `session/request_permission` receive `RequestPermissionOutcome::Cancelled` responses
- Follow ACP specification for cancellation behavior

## Progress Tracking

- [x] Add `CancelNotification` handler to ACP agent using proper SACP types
- [x] Extend SessionManager with cancellation support
- [x] Update AcpSession for cancel handling
- [x] Implement pending permission request cancellation (already implemented)
- [x] Add session handle cancel method
- [x] Add required imports and types from SACP schema
- [x] Write unit tests for cancellation logic
- [ ] Write integration tests for full cancellation flow
- [ ] Test edge cases and race conditions
- [ ] Verify ACP compliance with cancellation specification
- [ ] Update documentation and examples

## Dependencies

- Requires understanding of existing agent cancellation mechanism
- Depends on SACP library for ACP protocol handling and proper type usage
- May require updates to session state tracking
- Needs coordination with tool approval request handling
