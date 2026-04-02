# V1 MCP OAuth Re-Authentication When Refresh Token Fails Mid-Session

**Date**: 2026-04-01  
**Status**: Research complete

## Summary

When an MCP OAuth token expires mid-session in V1 (`chat_cli`), the `decorate_with_auth_retry` macro attempts a `refresh_token()` call. If the server did not issue a refresh token (or the refresh token itself is expired/revoked), the refresh fails and the original error is returned with **no re-auth fallback**. The full browser-based OAuth flow only runs during initial connection (`HttpServiceBuilder::try_build`), never mid-session.

## 1. Initial Connection: `HttpServiceBuilder::try_build`

**File**: `crates/chat-cli/src/mcp_client/oauth_util.rs:196-340`

The state machine cycles through:
1. `TryUnauthenticated` → attempt without auth
2. `TryAuthenticated(false)` → load cached creds or run full browser OAuth via `get_auth_manager`
3. `FailedBecauseTokenMightBeExpired` → call `refresh_token()`, if that fails, delete cached creds and `auth_client.take()`, then retry `TryAuthenticated(true)` which triggers a fresh browser OAuth
4. `TryAuthenticated(true)` → if this also fails → `Exhausted`

Key: when refresh fails at line 316-322, it clears the cached token file and sets `auth_client = None`, forcing `TryAuthenticated(true)` to call `get_auth_manager` again which runs the full browser flow via `get_auth_manager_impl`.

**This entire state machine only runs once at startup.** The resulting `AuthClientWrapper` is stored in `RunningService` and used for all subsequent tool calls.

## 2. Mid-Session Tool Calls: `decorate_with_auth_retry` Macro

**File**: `crates/chat-cli/src/mcp_client/client.rs:155-196`

```rust
// Simplified flow:
1. Call tool → if Ok, return
2. If Err and auth_client exists → try refresh_token()
3. If refresh Ok → retry tool call once
4. If refresh Err → return ORIGINAL error (no re-auth)
5. If no auth_client → return original error
```

The comment at line 188-191 explicitly acknowledges the gap:
> "Currently our event loop just does not allow us easy ways to reauth entirely once a session starts since this would mean swapping of transport (which also means swapping of client)"

The V2 agent crate (`crates/agent/src/agent/mcp/service.rs:340-410`) has the **identical limitation** — same macro pattern, same comment.

## 3. `AuthClientWrapper::refresh_token`

**File**: `crates/chat-cli/src/mcp_client/oauth_util.rs:151-159`

```rust
pub async fn refresh_token(&self) -> Result<(), OauthUtilError> {
    let cred = self.auth_client.auth_manager.lock().await.refresh_token().await?;
    // ... persist to disk ...
}
```

This delegates to `rmcp::AuthorizationManager::refresh_token` (rmcp 0.17.0, `src/transport/auth.rs:1020-1060`) which:
1. Loads stored credentials from the `CredentialStore`
2. Extracts the `refresh_token` field — if `None`, returns `TokenRefreshFailed("No refresh token available")`
3. Calls `exchange_refresh_token` against the token endpoint
4. Saves new credentials to the store

**Failure modes where re-auth is needed:**
- Server never issued a refresh token (common with some OAuth providers)
- Refresh token expired or revoked
- Server returns an error on the refresh grant

## 4. Loopback Server: `make_svc`

**File**: `crates/chat-cli/src/mcp_client/oauth_util.rs:420-500`

The loopback server is **one-shot by design**:
- Uses a `oneshot::channel` sender wrapped in `Arc<Mutex<Option<Sender>>>` — `.take()` ensures only one callback is processed
- The spawned task does `listener.accept().await` for exactly one connection, then exits
- `LoopBackDropGuard` cancels the server via `CancellationToken` when dropped
- The `_dg` (drop guard) is held only within `get_auth_manager_impl` scope — it's dropped when that function returns

This means a new loopback server must be created for each re-auth attempt.

## 5. Required Changes for Mid-Session Re-Auth

### What needs to happen when refresh fails in the retry macro:

1. **Detect refresh failure** — already happens at `client.rs:183`
2. **Run full OAuth flow** — needs `get_auth_manager_impl` or equivalent
3. **Update the `AuthClient`'s `AuthorizationManager`** — replace the inner `Arc<Mutex<AuthorizationManager>>`
4. **Persist new credentials** — write token + registration to disk
5. **Retry the tool call** with the refreshed auth

### Specific code changes needed:

#### A. Extract re-auth logic into `AuthClientWrapper`

Add a `reauthorize` method to `AuthClientWrapper` (`oauth_util.rs`) that:
- Reads the persisted `Registration` to get `client_id`, `scopes`, `redirect_uri`
- Calls `get_auth_manager_impl` (spins up loopback server, opens browser, waits for callback)
- Replaces the `AuthorizationManager` inside `self.auth_client.auth_manager` (it's `Arc<Mutex<>>`, so lock + swap)
- Persists new credentials to `self.cred_full_path`

The `AuthClientWrapper` needs additional fields:
- `reg_full_path: PathBuf` — to read the registration
- `scopes: Vec<String>` — or read from registration
- `oauth_config: Option<OAuthConfig>` — for redirect_uri port config
- A way to send the OAuth URL to the user (messenger or channel)

#### B. Modify `decorate_with_auth_retry` macro (`client.rs`)

In the `Err(_)` branch of refresh (line 186-191), instead of returning the error:
```rust
Err(_) => {
    // NEW: attempt full re-auth
    match auth_client.reauthorize().await {
        Ok(_) => {
            // Retry with new token
            match &self.inner_service { ... }
        },
        Err(_) => Err(e), // truly exhausted
    }
}
```

#### C. Messenger integration

The `reauthorize` flow needs to display the OAuth URL to the user. Options:
- Store a `Messenger` (or `mpsc::Sender`) in `AuthClientWrapper`
- Use `open::that()` to auto-open browser + print URL to stderr as fallback

#### D. Loopback server reuse

No changes needed to `make_svc` itself — it's designed to be called multiple times. Each re-auth creates a fresh loopback server, which is correct.

### Key architectural constraint

The `AuthClient<Client>` wraps `AuthorizationManager` in `Arc<Mutex<>>` (`rmcp-0.17.0/src/transport/auth.rs:212`). The same `Arc` is shared with the `StreamableHttpClientTransport`. So updating the `AuthorizationManager` in-place (lock + replace inner state) will propagate to the transport automatically — **no transport swap needed**.

This contradicts the comment in the macro that says "swapping of transport" is required. The `Arc<Mutex<AuthorizationManager>>` is shared, so updating credentials in the manager is sufficient.

### Risk areas

- **Blocking the agent loop**: The browser OAuth flow blocks until the user completes auth. The tool call will hang. Need timeout + user messaging.
- **Concurrent tool calls**: If multiple tool calls fail simultaneously, need to serialize re-auth attempts (only one browser flow at a time).
- **Port conflicts**: If `oauth_config` specifies a fixed port, a second loopback server on the same port will fail. Port 0 (random) is safer.
