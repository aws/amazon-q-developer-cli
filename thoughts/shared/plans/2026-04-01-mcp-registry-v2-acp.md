# Plan: Add MCP Registry Support to V2 ACP Code Path

**Date**: 2026-04-01
**Status**: Draft
**Research**: `thoughts/shared/research/2026-04-01-mcp-registry-lifecycle-v1.md`, `thoughts/shared/research/2026-04-01-v2-acp-mcp-registry-integration-points.md`

---

## Problem

The V2 ACP code path has zero MCP registry integration. Enterprise users who rely on registry-managed MCP servers get no registry resolution when using V2. The `mcp_registry.rs` module in `chat-cli-v2` already contains all the necessary functions (`McpRegistryClient`, `resolve_registry_servers_for_agent_config`, `apply_registry_filtering_to_agent`) — they just aren't called from the ACP session initialization path.

## Key V1 vs V2 Differences Affecting Approach

| Aspect | V1 | V2 | Impact |
|--------|----|----|--------|
| Agent config type | `crate::cli::agent::Agent` (custom struct) | `agent::agent_config::LoadedAgentConfig` (agent crate) | `apply_registry_filtering_to_agent` operates on V1's `Agent` type, NOT `LoadedAgentConfig`. V2 must use `resolve_registry_servers_for_agent_config` which operates on `LoadedAgentConfig`. |
| Agent loading | `Agents::load()` returns V1 `Agent` structs | `load_agents()` returns `Vec<LoadedAgentConfig>` | Registry filtering in V1 happens on V1 `Agent` before conversion. In V2, filtering must happen on `LoadedAgentConfig` directly. |
| Registry-type server handling | V1 has two phases: (1) `apply_registry_filtering_to_agent` on V1 `Agent` pre-ToolManager, (2) `process_mcp_servers` expands to `CustomToolConfig` in ToolManager | V2 uses `resolve_registry_servers_for_agent_config` which directly adds `Local`/`Remote` `McpServerConfig` entries to `LoadedAgentConfig` | V2's approach is simpler — one function call resolves everything. No two-phase process needed. |
| Subagent creation | Explicit `registry_data` parameter threaded through tool chain | Orchestrated sessions go through same `SessionManager::start_session()` path | V2 subagents get registry support for free if `SessionManager` has registry data and applies it at session creation. |
| State storage | `ConversationState` (serializable, persisted) | `SessionManager` (in-memory actor) | V2 doesn't need serialization — registry data lives in the `SessionManager` actor for the process lifetime. |
| Refresh | `ensure_fresh_mcp_data()` on every user turn, 24h TTL | Nothing | Refresh is a separate concern; can be added later. Initial implementation should focus on startup fetch. |

## Critical Insight: Which Function to Use

- `apply_registry_filtering_to_agent(agent: &mut crate::cli::agent::Agent, ...)` — operates on V1's `Agent` type. **Cannot be used directly** on V2's `LoadedAgentConfig`.
- `resolve_registry_servers_for_agent_config(config: &mut LoadedAgentConfig, registry: &McpRegistryResponse)` — operates on `LoadedAgentConfig`. **This is the right function for V2.** It finds servers referenced in the tools list but missing from the config (dropped during deserialization because `type: "registry"` isn't a known variant), looks them up in the registry, and adds them as `Local`/`Remote` `McpServerConfig` entries.

However, `resolve_registry_servers_for_agent_config` only re-adds missing servers — it does NOT filter/remove tools that reference servers not in the registry. For full parity with V1, we also need tool filtering. This can be done with a new helper that operates on `LoadedAgentConfig` (or by adapting the existing `filter_tools_by_registry` function).

---

## Phases

### Phase 1: Fetch and Store Registry Data at Startup

**Goal**: Fetch registry URL from API and registry data at `SessionManager` startup.

**Files to modify**:
- `crates/chat-cli-v2/src/agent/acp/session_manager.rs`

**Changes**:

1. Add fields to `SessionManager` struct (after line 260):
   ```rust
   /// MCP registry data for enterprise users, fetched once at startup
   mcp_registry_data: Option<McpRegistryResponse>,
   /// Whether MCP is enabled (from API profile)
   mcp_enabled: bool,
   ```

2. Add `mcp_registry_data` and `mcp_enabled` parameters to `SessionManager::new()` (line 271) and `SessionManagerBuilder`.

3. In `SessionManagerBuilder::spawn()` (line 141), after `load_agents()` completes and before `SessionManager::new()`, add registry fetch logic:
   ```rust
   // Fetch MCP registry for enterprise users
   let (mcp_enabled, mcp_registry_data) = match os.client.get_mcp_config().await {
       Ok((enabled, Some(registry_url))) if enabled => {
           let client = crate::mcp_registry::McpRegistryClient::new();
           match client.fetch_registry(&registry_url).await {
               Ok(registry) => (true, Some(registry)),
               Err(e) => {
                   error!(%e, "Failed to fetch MCP registry");
                   (true, None) // MCP enabled but no registry
               }
           }
       }
       Ok((enabled, None)) => (enabled, None), // No registry URL
       Err(e) => {
           error!(%e, "Failed to get MCP config from API");
           (true, None) // Default to enabled, no registry
       }
   };
   ```

**Verification**: Add `tracing::info!` logging to confirm registry fetch happens at startup. Check logs with `KIRO_LOG_LEVEL=chat_cli_v2=debug`.

**Success criteria**: Enterprise users see registry fetch in logs at ACP startup. Non-enterprise users skip it.

---

### Phase 2: Apply Registry Resolution at Session Creation

**Goal**: Resolve registry servers in agent configs before creating `AcpSession`.

**Files to modify**:
- `crates/chat-cli-v2/src/agent/acp/session_manager.rs`

**Changes**:

1. In `handle_request()` → `StartSession` branch (around line 460, after `agent_config_to_use` is constructed but before `AcpSessionBuilder`), apply registry resolution:
   ```rust
   // Resolve registry servers if registry data is available
   let agent_config_to_use = if let Some(ref registry) = self.mcp_registry_data {
       let mut config = agent_config_to_use;
       crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, registry);
       config
   } else {
       agent_config_to_use
   };
   ```

   This must happen AFTER the ACP MCP server merge (line 444-470) so that registry resolution sees the final merged config.

**Why this works for subagents too**: Orchestrated sessions (`handle_spawn_orchestrated` at line 1190) call `session_tx.start_session()` which routes to the same `StartSession` handler. The subagent's `LoadedAgentConfig` is looked up from `self.agent_configs` and goes through the same resolution path.

**Verification**: 
- Create an agent config with `tools: ["@registry-server/tool"]` where `registry-server` is defined in the registry
- Start a V2 ACP session — the server should be resolved and launched
- Spawn a subagent with the same agent — it should also get the resolved server

**Success criteria**: Registry-type MCP servers are resolved to `Local`/`Remote` configs and launched successfully in both main and subagent sessions.

---

### Phase 3: Tool Filtering Against Registry

**Goal**: Filter agent tools to remove references to servers not in the registry (parity with V1's `apply_registry_filtering_to_agent`).

**Files to modify**:
- `crates/chat-cli-v2/src/mcp_registry.rs` — add a new function
- `crates/chat-cli-v2/src/agent/acp/session_manager.rs` — call it

**Changes**:

1. Add a new function in `mcp_registry.rs` that operates on `LoadedAgentConfig`:
   ```rust
   /// Filter tools in a LoadedAgentConfig to remove references to servers
   /// not present in the registry. Companion to resolve_registry_servers_for_agent_config.
   pub fn filter_agent_config_tools_by_registry(
       agent_config: &mut LoadedAgentConfig,
       registry: &McpRegistryResponse,
   ) {
       let valid_servers: HashSet<&str> = registry.servers.iter()
           .map(|s| s.name.as_str())
           .collect();
       let existing_servers: HashSet<String> = agent_config.config().mcp_servers()
           .keys().cloned().collect();
       
       let filtered = agent_config.tools().into_iter().filter(|tool| {
           if tool == "*" { return true; }
           if let Some(stripped) = tool.strip_prefix('@') {
               let server_name = stripped.split('/').next().unwrap_or("");
               // Keep if: server exists in loaded config OR server is in registry
               existing_servers.contains(server_name) || valid_servers.contains(server_name)
           } else {
               true // Non-server-prefixed tools are always kept
           }
       }).collect();
       agent_config.config_mut().set_tools(filtered);
   }
   ```

2. Call it in `session_manager.rs` BEFORE `resolve_registry_servers_for_agent_config`:
   ```rust
   if let Some(ref registry) = self.mcp_registry_data {
       let mut config = agent_config_to_use;
       crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, registry);
       crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, registry);
       config
   }
   ```

**Verification**: Agent with tools referencing a server NOT in the registry should have those tools removed.

**Success criteria**: Tools referencing invalid/removed registry servers are filtered out before the agent starts.

---

### Phase 4: Agent Swap with Registry Resolution

**Goal**: When switching agents mid-session via `/agent switch`, apply registry resolution to the new agent config.

**Files to modify**:
- `crates/chat-cli-v2/src/agent/acp/session_manager.rs`

**Changes**:

1. In `handle_set_mode()` (line 369), apply registry resolution before calling `swap_agent()`:
   ```rust
   let agent_config = if let Some(ref registry) = self.mcp_registry_data {
       let mut config = agent_config.clone();
       crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, registry);
       crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, registry);
       config
   } else {
       agent_config.clone()
   };
   ```

**Verification**: Switch to an agent with registry servers mid-session — servers should resolve and launch.

**Success criteria**: Agent swap works with registry-type servers.

---

### Phase 5 (Future): Registry Refresh on User Input

**Goal**: Periodic refresh of registry data (24h TTL), matching V1's `ensure_fresh_mcp_data()` behavior.

**Not in initial scope.** This is a separate concern that can be added later. The initial implementation fetches once at startup, which covers the primary use case. Refresh would require:
- Adding `CachedRegistry` (with timestamp) to `SessionManager` instead of raw `McpRegistryResponse`
- A refresh check on each `PromptRequest` (user turn)
- Server change detection and restart logic
- Notification to active sessions about server changes

This is significantly more complex and should be a follow-up.

---

## Summary of Changes

| Phase | File | Change | LOC (est) |
|-------|------|--------|-----------|
| 1 | `session_manager.rs` | Add registry fields to `SessionManager`, fetch in `spawn()` | ~30 |
| 2 | `session_manager.rs` | Call `resolve_registry_servers_for_agent_config` in `StartSession` handler | ~8 |
| 3 | `mcp_registry.rs` | Add `filter_agent_config_tools_by_registry` function | ~25 |
| 3 | `session_manager.rs` | Call filter function before resolve | ~2 |
| 4 | `session_manager.rs` | Apply resolution in `handle_set_mode` | ~8 |
| **Total** | | | **~73** |

## Risks and Mitigations

1. **`get_mcp_config()` adds latency to ACP startup**: The API call happens in the `spawn()` async block, which already does `load_agents()`. The registry fetch (10s timeout) runs sequentially. Mitigation: could run in parallel with `load_agents()` using `tokio::join!`.

2. **Registry fetch failure blocks startup**: V1 disables all MCP on failure. V2 should be more lenient — log the error and continue without registry. Non-registry MCP servers should still work.

3. **`apply_registry_filtering_to_agent` type mismatch**: This function operates on V1's `Agent` type, not `LoadedAgentConfig`. We avoid this by using `resolve_registry_servers_for_agent_config` (which works on `LoadedAgentConfig`) plus a new `filter_agent_config_tools_by_registry` helper.

4. **No `McpServerConfig::Registry` variant in agent crate**: Registry-type entries are silently dropped during `LoadedAgentConfig` deserialization. The `resolve_registry_servers_for_agent_config` function handles this by detecting missing servers from the tools list. This is the same approach V1 uses for subagents and is the correct pattern for V2.
