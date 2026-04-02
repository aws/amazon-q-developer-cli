# V2 (Agent Crate) MCP OAuth Code Research

**Date**: 2026-04-01  
**Scope**: `crates/agent/src/agent/mcp/` vs `crates/chat-cli/src/mcp_client/`

## 1. V2 HttpServiceBuilder and AuthClientWrapper

### HttpServiceBuilder (`crates/agent/src/agent/mcp/oauth_util.rs:186-330`)

The V2 `HttpServiceBuilder` implements a state machine for establishing HTTP MCP connections with OAuth fallback:

```
TryUnauthenticated → TryAuthenticated(false) → FailedBecauseTokenMightBeExpired → TryAuthenticated(true) → Exhausted
```

**Key differences from V1:**
- V2 takes `server_actor_event_tx: &mpsc::Sender<McpServerActorEvent>` for OAuth URL delivery (oauth_util.rs:199)
- V1 takes `messenger: &dyn Messenger` and `os: &Os` (chat-cli/mcp_client/oauth_util.rs:228-229)
- V2 takes `cred_dir: &Path` as explicit parameter to `try_build()` (oauth_util.rs:210)
- V1 derives `cred_dir` from `os.path_resolver().global().mcp_auth_dir()` inside `try_build()` (chat-cli/mcp_client/oauth_util.rs:253)

### AuthClientWrapper (`crates/agent/src/agent/mcp/oauth_util.rs:128-162`)

Identical implementation in both V1 and V2. Both:
- Hold `cred_full_path: PathBuf` and `auth_client: AuthClient<Client>`
- `refresh_token()` calls `auth_client.auth_manager.lock().await.refresh_token().await`
- Persist refreshed credentials to disk

**V1 location**: `crates/chat-cli/src/mcp_client/oauth_util.rs:128-162`  
**V2 location**: `crates/agent/src/agent/mcp/oauth_util.rs:128-162`

## 2. RunningMcpService and decorate_with_auth_retry

### V2 `decorate_with_auth_retry` macro (`crates/agent/src/agent/mcp/service.rs:333-420`)

The macro generates retry wrappers for MCP operations. On any error:
1. Attempts `auth_client.refresh_token()`
2. If refresh succeeds, retries the original operation once
3. If refresh fails, returns the original error

**Applied to** (service.rs:438-442):
- `call_tool(CallToolRequestParams) -> CallToolResult`
- `list_all_tools() -> Vec<RmcpTool>`
- `list_all_prompts() -> Vec<RmcpPrompt>`

Plus a manual implementation for `get_prompt()` (service.rs:444-480) with identical retry logic.

### V1 `decorate_with_auth_retry` macro (`crates/chat-cli/src/mcp_client/client.rs:164-198`)

Nearly identical macro. Applied to (client.rs:263-265):
- `call_tool(CallToolRequestParams) -> CallToolResult`
- `get_prompt(GetPromptRequestParams) -> GetPromptResult`

### Key issues present in BOTH versions:

1. **No error type discrimination** — Both have `// TODO: discern error type prior to retrying`. Every error triggers a token refresh attempt, not just 401/403 auth errors.
   - V2: service.rs:358, service.rs:397
   - V1: client.rs:175

2. **No full re-auth path** — Both have the comment: "Currently our event loop just does not allow us easy ways to reauth entirely once a session starts since this would mean swapping of transport (which also means swapping of client)"
   - V2: service.rs:374, service.rs:413
   - V1: client.rs:191

3. **Silent refresh failure** — When `refresh_token()` fails, both versions silently return the original error with no logging or user notification.

## 3. V2 Tool Call Auth Failure Handling

### Flow: Agent → McpManager → McpServerActor → RunningMcpService

1. Agent calls `McpManagerHandle::execute_tool()` (mod.rs:155)
2. McpManager routes to `McpServerActorHandle::execute_tool()` (mod.rs:282)
3. McpServerActor spawns async task calling `RunningMcpService::call_tool()` (actor.rs:262-273)
4. `call_tool()` uses `decorate_with_auth_retry` macro (service.rs:438)

**Re-auth mechanism**: Token refresh only. No full OAuth re-authorization flow exists post-initialization.

**What happens when refresh fails**:
- `McpServerActorError::Service` is returned to the agent (actor.rs:148-152)
- The error propagates up through `McpManagerHandle::execute_tool()` via oneshot channel
- The agent receives a tool execution error — no special handling for auth failures
- No mechanism to trigger a new OAuth browser flow mid-session

### Transport health check (actor.rs:240-258)
V2 checks `is_transport_closed()` before executing tools, providing early detection of dead connections. This is a V2-only feature not present in V1's tool execution path.

## 4. V1 vs V2 Code Relationship — Duplicated, Not Shared

The MCP OAuth code is **fully duplicated** between V1 and V2:

| Component | V1 Location | V2 Location | Identical? |
|-----------|-------------|-------------|------------|
| `OauthUtilError` | `chat-cli/mcp_client/oauth_util.rs:42-76` | `agent/mcp/oauth_util.rs:68-87` | ~95% (V1 has `Directory` variant, V2 has `MissingCredentials`) |
| `AuthClientWrapper` | `chat-cli/mcp_client/oauth_util.rs:128-162` | `agent/mcp/oauth_util.rs:128-162` | 100% identical |
| `HttpServiceBuilderState` | `chat-cli/mcp_client/oauth_util.rs:168-175` | `agent/mcp/oauth_util.rs:178-184` | 100% identical |
| `HttpServiceBuilder` | `chat-cli/mcp_client/oauth_util.rs:183-340` | `agent/mcp/oauth_util.rs:193-330` | ~90% (different messenger/event patterns) |
| `get_auth_manager()` | `chat-cli/mcp_client/oauth_util.rs:342-400` | `agent/mcp/oauth_util.rs:332-390` | ~90% (different params) |
| `get_auth_manager_impl()` | `chat-cli/mcp_client/oauth_util.rs:402-440` | `agent/mcp/oauth_util.rs:392-430` | ~85% (V1 uses `messenger.send_oauth_link()`, V2 uses `server_actor_event_tx.send(OauthRequest)`) |
| `compute_key()` | `chat-cli/mcp_client/oauth_util.rs:442-448` | `agent/mcp/oauth_util.rs:432-438` | 100% identical |
| `start_authorization()` | `chat-cli/mcp_client/oauth_util.rs:455-500` | `agent/mcp/oauth_util.rs:445-490` | 100% identical |
| `get_stub_credentials()` | `chat-cli/mcp_client/oauth_util.rs:503-516` | `agent/mcp/oauth_util.rs:493-506` | 100% identical |
| `make_svc()` | `chat-cli/mcp_client/oauth_util.rs:518-590` | `agent/mcp/oauth_util.rs:508-580` | 100% identical |
| `LoopBackDropGuard` | `chat-cli/mcp_client/oauth_util.rs:78-86` | `agent/mcp/oauth_util.rs:90-98` | 100% identical |
| `Registration` | `chat-cli/mcp_client/oauth_util.rs:100-114` | `agent/mcp/oauth_util.rs:112-126` | 100% identical |
| `OAuthMeta` | `chat-cli/mcp_client/oauth_util.rs:93-98` | `agent/mcp/oauth_util.rs:103-108` | 100% identical |
| `OAuthConfig` | `chat-cli/cli/chat/tools/custom_tool.rs:53-60` | `agent/mcp/oauth_util.rs:62-67` | ~80% (V1 has extra `oauth_scopes` field) |
| `decorate_with_auth_retry` | `chat-cli/mcp_client/client.rs:164-198` | `agent/mcp/service.rs:333-420` | ~90% (V2 has two macro variants, handles `InnerService` enum) |
| `InnerService` | `chat-cli/mcp_client/client.rs:201-218` | `agent/mcp/service.rs:498-520` | 100% identical |

### Why they diverge

The primary divergence point is **how OAuth URLs reach the user**:
- **V1**: Uses `Messenger` trait → `send_oauth_link()` → writes to terminal directly
- **V2**: Uses `mpsc::Sender<McpServerActorEvent>` → `OauthRequest` event → bubbles up through `McpManager` → `McpManagerHandle` → broadcast to agent → ACP protocol → TUI

### V1 uses V2's MCP code only for subagents
`crates/chat-cli/src/agent/subagent.rs:13-15` imports `agent::mcp::{McpManager, McpServerEvent}` — confirming V1's main chat loop uses its own MCP client, while subagents use the agent crate's implementation.

## 5. Shared Issues Requiring Fixes in Both Versions

| Issue | V1 | V2 | Fix Shareable? |
|-------|----|----|----------------|
| No error type discrimination before retry | ✅ | ✅ | Yes — same macro pattern |
| No full re-auth (OAuth browser flow) mid-session | ✅ | ✅ | Partially — different event delivery |
| Silent refresh failure (no user notification) | ✅ | ✅ | Partially — different UI channels |
| No retry count limit in macro | ✅ | ✅ | Yes — same macro pattern |

## 6. Consolidation Opportunities

### High-value shared code (can be extracted to agent crate or new shared crate):
- `AuthClientWrapper` — 100% identical
- `Registration`, `OAuthMeta`, `LoopBackDropGuard` — 100% identical
- `compute_key()`, `start_authorization()`, `get_stub_credentials()`, `make_svc()` — 100% identical
- `HttpServiceBuilderState` — 100% identical
- `OauthUtilError` — nearly identical, unify variants

### Requires abstraction to share:
- `HttpServiceBuilder` — needs trait/callback for OAuth URL delivery (messenger vs event channel)
- `get_auth_manager()` / `get_auth_manager_impl()` — same OAuth URL delivery divergence
- `decorate_with_auth_retry` macro — V2 version handles `InnerService` enum, V1 uses simpler `inner_service` field
- `OAuthConfig` — V1 has extra `oauth_scopes` field nested inside it

### Recommended approach:
1. Extract shared OAuth primitives (`AuthClientWrapper`, `Registration`, `compute_key`, `start_authorization`, `make_svc`, etc.) into the `agent` crate's `mcp::oauth_util` module
2. Define a trait for OAuth URL delivery (replacing both `Messenger::send_oauth_link` and `McpServerActorEvent::OauthRequest`)
3. Have V1's `mcp_client` re-export/delegate to the shared code
4. Fix the `decorate_with_auth_retry` issues once in the shared location
