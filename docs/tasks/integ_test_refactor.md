# Integration Test Refactor - Session-Aware Mock Responses

## Problem

The current test infrastructure doesn't support multiple concurrent sessions (e.g., main session + subagent sessions). Mock API responses are pushed through a single channel that only reaches one session.

## Solution

Move `IpcServer` to `SessionManager` and create a shared `MockResponseRegistry` actor that routes mock responses by session_id.

## Implementation - COMPLETE

### Files Modified

1. `crates/chat-cli/src/api_client/mod.rs` - Added `MockResponseRegistry`, modified `IpcMockApiClient`
2. `crates/chat-cli/src/agent/ipc_server.rs` - Added `session_id` to commands, routes to registry
3. `crates/chat-cli/src/agent/acp/session_manager.rs` - Spawns `IpcServer` and `MockResponseRegistry`
4. `crates/chat-cli/src/agent/acp/acp_agent.rs` - Removed `IpcServer` spawn, accepts registry via builder
5. `crates/chat-cli/tests/common/harness.rs` - Updated to pass `session_id`

### Test Harness API

```rust
/// Push mock response events for a specific session.
/// Pass `None` to signal end of response stream.
pub async fn push_mock_response(&mut self, session_id: &str, events: Option<Vec<ChatResponseStream>>);
```
