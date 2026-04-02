# MCP Registry Lifecycle in V1 (chat_cli)

**Date**: 2026-04-01  
**Scope**: Complete data flow from startup through subagent usage

---

## 1. Registry Fetching

### Trigger: Session Startup

The registry fetch is triggered once during `ChatCommand::run()` in `crates/chat-cli/src/cli/chat/mod.rs`.

**Step 1 — Determine enterprise status and get MCP config** (`mod.rs:443-470`):
- Non-enterprise users: MCP enabled by default, no registry URL (`mcp_registry_url = None`)
- Enterprise users: calls `os.client.get_mcp_config()` which returns `(enabled: bool, registry_url: Option<String>)`
- On API failure: MCP defaults to disabled, `mcp_api_failure = true`

**Step 2 — Fetch registry if URL exists** (`mod.rs:548-600`):
```
if mcp_enabled && mcp_registry_url.is_some() {
    McpRegistryClient::new().fetch_registry(registry_url).await
}
```
- `McpRegistryClient` is a simple HTTP client with 10-second timeout (`mcp_registry.rs:300-305`)
- `fetch_registry()` does: HTTP GET → parse JSON → validate structure → return `McpRegistryResponse` (`mcp_registry.rs:310-335`)
- No caching on initial fetch — it always hits the network

### Cache Behavior

The `CachedRegistry` struct (`mcp_registry.rs:381-400`) stores:
- `data: McpRegistryResponse` — the actual registry
- `fetched_at: OffsetDateTime` — timestamp
- `source_url: String` — the URL it was fetched from

Cache TTL is **24 hours** (`MCP_CACHE_TTL_HOURS = 24`, `mcp_registry.rs:45`).

`fetch_with_cache()` (`mcp_registry.rs:337-370`) checks:
1. Is cache present AND same URL AND not stale? → return cached data
2. Otherwise → fetch fresh, update cache

The initial startup does NOT use `fetch_with_cache()` — it calls `fetch_registry()` directly. The cache is populated manually afterward (`mod.rs:1143-1148`). `fetch_with_cache()` is used only during periodic refresh via `conversation.fetch_mcp_registry()`.

---

## 2. Registry Storage

### Where it lives

Registry data is stored in `ConversationState` (`conversation.rs:260-271`):
```rust
pub mcp_registry_url: Option<String>,
pub mcp_registry_cache: Option<CachedRegistry>,
pub mcp_registry_error_type: Option<RegistryErrorType>,
```

These fields are serializable (`serde`) so they persist with conversation state.

### How it's populated

**New conversations** (`mod.rs:1138-1150`):
- `conversation.mcp_registry_url` set from the API response
- `conversation.mcp_registry_cache` set from the freshly-fetched `registry_data`
- Server versions cached via `conversation.cache_mcp_server_version()` for later sync checks (`mod.rs:1170-1180`)

**Resumed conversations** (`mod.rs:1228`):
- Registry state is deserialized from the persisted conversation
- `ensure_fresh_mcp_data()` is called to refresh if stale

**New conversation within session** (`mod.rs:1251-1279`):
- Registry state (`mcp_registry_url`, `mcp_registry_cache`, `mcp_registry_error_type`) is carried over from the old conversation via `take()` — avoids redundant API calls

### How it flows to consumers

Registry data is extracted from the cache on demand:
```rust
self.conversation.mcp_registry_cache.as_ref().map(|c| c.data.clone())
// or
self.conversation.mcp_registry_cache.as_ref().map(|c| &c.data)
```

This pattern appears at:
- Tool invocation (`mod.rs:3592-3594`) — passed as `Option<&McpRegistryResponse>` to `tool.invoke()`
- Agent swap (`conversation.rs:1501-1502`) — passed to `tool_manager.swap_agent()`
- Periodic sync (`mod.rs:1885`) — returned from `ensure_fresh_mcp_data()`

---

## 3. Registry Application to V1 Main Agent

### Pre-ToolManager Filtering

Before the `ToolManager` is created, registry filtering is applied to ALL agents (`mod.rs:556-578`):

```rust
for agent in agents.agents.values_mut() {
    apply_registry_filtering_to_agent(agent, &registry)?;
}
```

`apply_registry_filtering_to_agent()` (`mcp_registry.rs:839-895`):
1. Calls `process_mcp_servers()` to validate which agent servers exist in the registry
2. Removes registry-type servers that aren't in the registry
3. Filters the agent's `tools` list to only include tools from valid servers
4. Does NOT overwrite agent config — keeps original minimal `type: "registry"` entries

### ToolManager Server Processing

When `ToolManagerBuilder::build()` is called (`tool_manager.rs:307-320`):

```rust
let registry_data = self.registry_data.as_ref();
process_mcp_servers(&agent_servers, registry_data)
```

`process_mcp_servers()` (`mcp_registry.rs:413-475`):
- **Registry mode** (registry_data is Some): For each agent server, checks if it's `type: "registry"` or exists in registry. If yes, calls `convert_registry_to_config()` to expand the minimal config into a full `CustomToolConfig` with actual command/URL/args. Non-registry servers are ignored.
- **Non-registry mode** (registry_data is None): Only processes stdio/http servers. Registry-type servers are ignored.

`convert_registry_to_config()` (`mcp_registry.rs:480-550+`) converts registry definitions to `CustomToolConfig`:
- Remote servers → URL + headers from registry
- Local npm packages → `npx -y <package>@<version>` command
- Local pypi packages → `uvx` command
- Preserves agent-level overrides (timeout, headers, env)

### Agent Swap

When switching agents mid-session (`conversation.rs:1495-1555`):
1. Registry filtering is applied to the new agent
2. `tool_manager.swap_agent()` is called with `registry_data`
3. The tool manager rebuilds with the new agent's servers, using `process_mcp_servers()` again

---

## 4. Registry Application to Subagents

### Data Flow: mod.rs → tool.rs → use_subagent.rs → subagent.rs

**Step 1** — At tool invocation time (`mod.rs:3592-3607`):
```rust
let registry_data = self.conversation.mcp_registry_cache.as_ref().map(|c| &c.data);
tool.invoke(..., registry_data).await;
```

**Step 2** — `Tool::invoke()` routes to `UseSubagent::invoke()` (`tool.rs:210`):
```rust
Tool::UseSubagent(use_subagent) => use_subagent.invoke(os, agents, ..., registry_data).await
```

**Step 3** — `UseSubagent::invoke()` passes registry to each `Subagent` struct (`use_subagent.rs:337`):
```rust
invoke_subagent.as_subagent(..., registry_data)
```
The `Subagent` struct holds `registry_data: Option<&McpRegistryResponse>` (`use_subagent.rs:59`).

**Step 4** — `Subagent::query()` calls `resolve_registry_servers_for_agent_config()` (`subagent.rs:252-255`):
```rust
if let Some(registry) = self.registry_data {
    resolve_registry_servers_for_agent_config(&mut snapshot.agent_config, registry);
}
```

### What resolve_registry_servers_for_agent_config Does

(`mcp_registry.rs:695-770+`)

This function handles a different problem than `process_mcp_servers()`. Subagents use the `agent` crate's `LoadedAgentConfig` (not V1's `CustomToolConfig`), so registry servers that were dropped during deserialization (because `type: "registry"` isn't a known variant in the agent crate schema) need to be re-added:

1. Collects server names referenced in the agent's `tools` list (e.g., `@server-name/tool`)
2. Finds servers that are referenced but missing from the loaded config
3. For each missing server found in the registry, creates the appropriate `AgentMcpServerConfig`:
   - Remote → `RemoteMcpServerConfig` with URL/headers
   - npm → `LocalMcpServerConfig` with `npx` command
   - pypi → `LocalMcpServerConfig` with `uvx` command
4. Adds resolved servers to the agent config via `add_mcp_servers()`

---

## 5. Registry Sync/Refresh

### Trigger: Every User Input

`ensure_fresh_mcp_data()` is called at two points:
1. **Resumed conversations** — once at session creation (`mod.rs:1228`)
2. **Every user turn** — in `handle_input()` (`mod.rs:3060`)

### Refresh Logic (`mod.rs:1867-1990`)

```
ensure_fresh_mcp_data()
  ├── if !mcp_enabled → return None
  ├── if !should_refresh_mcp_cache() → return cached data
  │     (checks if mcp_last_checked is older than 24 hours)
  ├── Non-enterprise users → just update timestamp, return cache
  └── Enterprise users:
       ├── Call get_mcp_config() API to get fresh (enabled, registry_url)
       ├── If disabled → return None
       ├── If URL removed → clear cache, switch to non-registry mode
       ├── If URL changed or cache stale:
       │    ├── fetch_mcp_registry() (uses fetch_with_cache)
       │    ├── check_mcp_server_changes() → detect removed/updated servers
       │    ├── handle_server_changes_with_warnings() → warn user, terminate removed servers
       │    └── Return fresh registry
       └── On API failure → disable MCP, clear everything
```

### Server Change Detection (`conversation.rs:465-520`)

`check_mcp_server_changes()` compares cached server versions against the fresh registry:
- **Server removed from registry** → added to `servers_to_remove` (terminated with warning)
- **Server version changed** → added to `servers_to_restart` (restarted with old→new version warning)

There is NO periodic timer — refresh only happens when the 24-hour TTL expires AND the user submits input.

---

## 6. Error Handling

### Registry Fetch Failure at Startup (`mod.rs:580-600`)

When `fetch_registry()` fails during initial startup:
1. `mcp_enabled` is set to `false`
2. Error is categorized via `RegistryErrorType::from_error()` (`mcp_registry.rs:17-42`):
   - `NetworkConnectivity` — connection/timeout/DNS/HTTP 4xx-5xx errors
   - `RegistryData` — JSON parsing, validation failures
3. Error displayed to user via `display_registry_error_to_writer()` (`mcp_registry.rs:1403-1450`):
   - Network: "is not reachable. Check your network connection"
   - Data: "contains invalid data. Contact your administrator"
4. All MCP servers cleared from all agents

### Registry Fetch Failure During Refresh (`mod.rs:1940-1960`)

When `fetch_mcp_registry()` fails during periodic sync:
1. MCP disabled (`mcp_enabled = false`)
2. Cache cleared (`mcp_registry_cache = None`)
3. Server versions cleared
4. All MCP servers cleared from active agent
5. Error displayed to user with same `display_registry_error_to_writer()`

### API Failure (get_mcp_config) (`mod.rs:1975-1990`)

When the profile API call fails during refresh:
1. MCP disabled
2. `mcp_disabled_due_to_api_failure` flag set (distinguishes admin-disabled vs API-failure)
3. All registry state cleared

### Process Failure (`tool_manager.rs:313-320`)

When `process_mcp_servers()` fails (corrupt registry data):
- Returns empty server map — all MCP servers disabled
- Logged as error but doesn't crash

### Filtering Failure (`mod.rs:562-567`)

When `apply_registry_filtering_to_agent()` fails for a specific agent:
- That agent's MCP servers are cleared
- Other agents are unaffected
- Error logged

---

## Summary: Complete Lifecycle

```
Session Start
  │
  ├─ 1. get_mcp_config() API → (enabled, registry_url)
  │     [Enterprise only; non-enterprise defaults to enabled, no registry]
  │
  ├─ 2. fetch_registry(url) → McpRegistryResponse
  │     [HTTP GET, 10s timeout, JSON parse + validate]
  │
  ├─ 3. apply_registry_filtering_to_agent() for ALL agents
  │     [Remove invalid servers, filter tools list]
  │
  ├─ 4. ToolManagerBuilder.registry_data(registry).build()
  │     └─ process_mcp_servers() → expand registry entries to full configs
  │        └─ Spawn MCP server child processes
  │
  ├─ 5. Store in ConversationState:
  │     ├─ mcp_registry_url
  │     ├─ mcp_registry_cache (CachedRegistry with timestamp)
  │     └─ mcp_server_versions (for sync detection)
  │
  User Turn (every input)
  │
  ├─ 6. ensure_fresh_mcp_data()
  │     └─ If 24h TTL expired → re-fetch config + registry
  │        └─ Detect server removals/version changes → warn + restart
  │
  Tool Execution
  │
  ├─ 7. tool.invoke(..., registry_data) 
  │     └─ Only UseSubagent uses it
  │
  Subagent Invocation
  │
  └─ 8. Subagent::query()
        └─ resolve_registry_servers_for_agent_config()
           [Re-add registry servers dropped during agent config deserialization]
           └─ Agent spawns with full MCP server configs
```
