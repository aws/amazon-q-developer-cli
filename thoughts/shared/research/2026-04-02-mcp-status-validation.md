# Research: `/mcp` Slash Command Status Validation

**Date:** 2026-04-02  
**Goal:** Understand how `/mcp` reports server status and what changes are needed to validate OAuth token validity before showing green status.

## 1. `/mcp` Command Handler Location

- **File:** `crates/chat-cli/src/cli/chat/cli/mcp.rs`
- **Entry point:** `McpArgs::execute()` at line ~170
- **Status display:** `execute_status()` at line ~178 (invoked when no subcommand is given, i.e. bare `/mcp`)
- **Subcommands:** `List`, `Add`, `Remove` (line ~161-167)

## 2. How Status Is Determined (Green Checkmark)

The `/mcp` status display does **NOT** show a green checkmark directly. Instead, it displays the `mcp_load_record` entries verbatim. The green checkmark (`✓`) is rendered at **initial load time** by `queue_success_message()` (tool_manager.rs:2307), which writes into a string buffer that becomes a `LoadingRecord::Success`.

### Status determination flow:

1. During server initialization (tool_manager.rs:~1830-1970), each MCP server is spawned and its tool list is fetched.
2. If the tool list fetch succeeds → `LoadingRecord::success(record)` is pushed (tool_manager.rs:1946)
3. If it fails but process started → `LoadingRecord::warn(record)` (tool_manager.rs:1944)
4. If the server fails to start entirely → `LoadingRecord::err(record)` (tool_manager.rs:~1960)
5. `/mcp` (execute_status) at line ~270-290 simply iterates `mcp_load_record` and prints each record's timestamp + content string. If no load record exists for a configured server, it shows "Server is still initializing..."

**Key insight:** The status shown by `/mcp` is a **snapshot from load time**. It reflects whether the server *initially loaded successfully*, not whether it's currently healthy or has valid auth.

### The green checkmark text comes from:
```rust
// tool_manager.rs:2307
fn queue_success_message(name: &str, time_taken: &str, output: &mut impl Write) -> eyre::Result<()> {
    queue!(output,
        StyledText::success_fg(),
        style::Print("✓ "),
        StyledText::info_fg(),
        style::Print(name),
        StyledText::reset(),
        style::Print(" loaded in "),
        StyledText::warning_fg(),
        style::Print(format!("{time_taken} s\n")),
        StyledText::reset(),
    )
}
```

## 3. Does It Check Token Validity?

**No.** The `/mcp` status command (`execute_status`) does zero token or auth validation. It:

1. Checks `session.conversation.mcp_enabled` (line ~179)
2. Gets `pending_clients()` list (line ~202)
3. Locks `mcp_load_record` (line ~207)
4. Iterates configured servers and prints their load records (lines ~230-300)

There is no access to `RunningService`, `AuthClientWrapper`, or any token state in the status path. The only references to `clients` in mcp.rs are `pending_clients()` calls.

## 4. Available Data Structures

### ToolManager (tool_manager.rs:601)
The `execute_status` function accesses `session.conversation.tool_manager` which has:
- `clients: HashMap<String, InitializedMcpClient>` — the actual MCP client connections
- `pending_clients: Arc<RwLock<HashSet<String>>>` — servers still initializing
- `mcp_load_record: Arc<Mutex<HashMap<String, Vec<LoadingRecord>>>>` — historical load status
- `schema: HashMap<ModelToolName, ToolSpec>` — loaded tool specs

### InitializedMcpClient (client.rs:646)
```rust
pub enum InitializedMcpClient {
    Pending(JoinHandle<Result<RunningService, McpClientError>>),
    Ready(RunningService),
}
```

### RunningService (client.rs:246)
```rust
pub struct RunningService {
    pub inner_service: InnerService,
    pub transport_type: TransportType,
    auth_client: Option<AuthClientWrapper>,  // PRIVATE field
}
```
- `auth_client` is **private** — not accessible from outside the module
- Has `is_transport_closed()` method (public)
- Has `call_tool()` and `get_prompt()` with auth retry via `decorate_with_auth_retry!` macro

### AuthClientWrapper (oauth_util.rs:136)
```rust
pub struct AuthClientWrapper {
    pub cred_full_path: PathBuf,
    pub auth_client: AuthClient<Client>,  // from rmcp::transport::auth
}
```
- `refresh_token()` — refreshes token and persists to `cred_full_path`
- `cred_full_path` — path to the persisted OAuth token file on disk

### Auth Retry Mechanism (client.rs:164)
The `decorate_with_auth_retry!` macro wraps `call_tool` and `get_prompt`:
- On first failure, attempts `auth_client.refresh_token()`
- If refresh succeeds, retries the operation
- If refresh fails, returns original error
- **This only runs during actual tool calls, NOT during status checks**

## 5. Changes Needed for Token Validation in `/mcp`

### Option A: Add a `validate_token()` method to RunningService

Since `auth_client` is private on `RunningService`, add a public method:

```rust
// In client.rs, on impl RunningService
pub async fn is_auth_valid(&self) -> Option<bool> {
    // Returns None if no auth needed (stdio), Some(true/false) for HTTP with OAuth
    let auth = self.auth_client.as_ref()?;
    // Try reading the token file and checking expiry
    // Or attempt a lightweight refresh_token() call
    Some(auth.refresh_token().await.is_ok())
}
```

**Considerations:**
- `refresh_token()` makes a network call — may be slow for `/mcp` status
- Could instead read `cred_full_path` and check `expires_in` / `expires_at` from the persisted `OAuthTokenResponse`
- The `OAuthTokenResponse` (from rmcp) likely has `expires_in` field (seen in social.rs patterns)

### Option B: Read token file directly (lighter weight)

Add a method that reads the credential file at `auth_client.cred_full_path` and checks if the token is expired without making a network call:

```rust
pub async fn check_token_status(&self) -> TokenStatus {
    match &self.auth_client {
        None => TokenStatus::NoAuthRequired,
        Some(wrapper) => {
            match tokio::fs::read_to_string(&wrapper.cred_full_path).await {
                Ok(content) => {
                    // Parse OAuthTokenResponse, check expires_at
                    // Return Valid/Expired
                }
                Err(_) => TokenStatus::Missing,
            }
        }
    }
}
```

### Option C: Lightweight ping via `list_tools`

Call `list_tools` on the server — if it returns 401/auth error, the token is invalid. This is the most accurate but slowest approach.

### Required Changes Summary

1. **client.rs** — Add public method on `RunningService` to expose token validity (since `auth_client` is private)
2. **tool_manager.rs** — Add a public method on `ToolManager` to check auth status per server (iterating `clients`)
3. **cli/mcp.rs** (`execute_status`) — After printing load records, also check current auth status for each server and display a warning if token is expired
4. **TransportType awareness** — Only HTTP/Registry servers can have OAuth; Stdio servers never need token validation (TransportType enum at custom_tool.rs:41)

### Data Access Path from `/mcp`

```
session.conversation.tool_manager.clients  →  HashMap<String, InitializedMcpClient>
    → InitializedMcpClient::Ready(RunningService)
        → RunningService.auth_client (PRIVATE - needs new public method)
            → AuthClientWrapper.cred_full_path / .refresh_token()
```

### Key Risk

- `refresh_token()` is async and makes a network call — could slow down `/mcp` display
- Reading the token file is faster but requires knowing the `OAuthTokenResponse` schema and expiry semantics from rmcp
- Some servers may not use OAuth at all (stdio transport) — need to handle gracefully
