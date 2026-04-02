# V2 ACP Code Path: MCP Registry Integration Points

**Date**: 2026-04-01
**Scope**: Where MCP registry support needs to be added in the V2 ACP code path

---

## 1. ACP Session Initialization Flow

The V2 ACP session lifecycle:

```
CLI args (AcpSpawnArgs)
  → execute() in acp_agent.rs:2688
    → SessionManager::builder().spawn() in acp_agent.rs:2697
      → SessionManager actor loop (session_manager.rs:157)
        → load_agents() at startup (session_manager.rs:162)

NewSessionRequest (from TUI)
  → session_manager.handle_request() 
    → AcpSessionBuilder::start_session() (acp_agent.rs:765)
      → AcpSession::with_builder() (acp_agent.rs:920)
        → Agent::new() (acp_agent.rs:1044)
          → Agent::spawn() → Agent::initialize() → launch_mcp_servers()
```

**Key entry points:**
- `execute()` — `crates/chat-cli-v2/src/agent/acp/acp_agent.rs:2688` — Creates `SessionManager`, passes `AcpSpawnArgs`
- `SessionManager::spawn()` — `crates/chat-cli-v2/src/agent/acp/session_manager.rs:140` — Loads agent configs via `load_agents()` at line 162
- `NewSessionRequest` handler — `crates/chat-cli-v2/src/agent/acp/acp_agent.rs:2770` — Creates `AcpSessionConfig`, calls `session_tx.start_session()`
- `SessionManager::handle_request()` — `crates/chat-cli-v2/src/agent/acp/session_manager.rs:444` — Merges ACP MCP servers into agent config, builds `AcpSessionBuilder`

## 2. Agent Creation in V2

The `Agent` (from `agent` crate) is created in `AcpSession::with_builder()`:

**File**: `crates/chat-cli-v2/src/agent/acp/acp_agent.rs:920-1070`

```rust
// Line 1044
let mut agent = Agent::new(
    snapshot,
    builder.local_mcp_path,
    builder.global_mcp_path,
    model,
    McpManager::default().spawn(),
    builder.is_subagent,
    builder.code_intelligence,
    knowledge_provider,
    task_store,
    builder.agent_configs.clone(),
).await?;
```

**`AgentSnapshot`** is constructed at lines 940-970 (new session) or 930-940 (loaded session). It contains:
- `agent_config: LoadedAgentConfig` — includes MCP server definitions
- `settings: AgentSettings` — runtime settings (mcp_init_timeout, trust_all_tools, etc.)
- `permissions: RuntimePermissions`

**`AgentSettings`** (`crates/agent/src/agent/types.rs:155-170`):
```rust
pub struct AgentSettings {
    pub mcp_init_timeout: Duration,
    pub disable_auto_compact: bool,
    pub trust_all_tools: bool,
}
```
⚠️ **No registry URL field exists in `AgentSettings` or `AgentSnapshot`.**

## 3. MCP Server Lifecycle in V2

**MCP Manager**: `crates/agent/src/agent/mcp/mod.rs`

The `McpManager` is an actor that manages MCP server lifecycle:
- `McpManagerHandle::launch_server(name, config)` — Spawns `McpServerActor` (line 195)
- `McpServerActor` → `McpService` → `rmcp::RunningService`
- Servers go through: Not Launched → Initializing → Initialized (or Error)

**Server launching** happens in `Agent::initialize()` → `Agent::launch_mcp_servers()`:
- `crates/agent/src/agent/mod.rs:670` — `initialize()` method
- `crates/agent/src/agent/mod.rs:710` — `launch_mcp_servers()` iterates `cached_mcp_configs.configs`

**`LoadedMcpServerConfigs::from_agent_config()`** (`crates/agent/src/agent/agent_config/mod.rs:258`):
- Reads MCP servers from `LoadedAgentConfig.config().mcp_servers()`
- Optionally loads from workspace/global `mcp.json` if `use_legacy_mcp_json` is true
- Returns `LoadedMcpServerConfigs { configs, overridden_configs }`

**`McpServerConfig`** enum (`crates/agent/src/agent/agent_config/definitions.rs:531`):
```rust
pub enum McpServerConfig {
    Local(LocalMcpServerConfig),   // stdio: command, args, env
    Remote(RemoteMcpServerConfig), // http: url, headers, oauth
}
```
⚠️ **No `Registry` variant exists.** Registry-type servers are currently only handled in V1's `mcp_registry.rs` by converting them to Local/Remote before they reach the agent crate.

## 4. ACP MCP Server Merging (Session Manager)

When the TUI sends MCP servers via `NewSessionRequest`, they're merged in `session_manager.rs:444-475`:

```rust
// session_manager.rs:444
let agent_config_to_use = if !config.mcp_servers.is_empty() {
    let mut ephemeral = base_agent_config.config().clone();
    let converted: Vec<_> = config.mcp_servers.into_iter()
        .filter_map(|server| convert_mcp_server(server).ok())
        .collect();
    ephemeral.add_mcp_servers(converted);
    LoadedAgentConfig::new(ephemeral, ConfigSource::BuiltIn, resolved_prompt)
} else {
    base_agent_config.clone()
};
```

**`convert_mcp_server()`** (`crates/chat-cli-v2/src/agent/acp/mcp_conversion.rs:21`):
- Converts `sacp::schema::McpServer` (Stdio/Http/Sse) → `agent::McpServerConfig` (Local/Remote)
- ⚠️ **No registry type handling** — only Stdio, Http, Sse are supported

## 5. Subagent Creation in V2

V2 subagents are created via the **orchestration system** in `session_manager.rs`, NOT via the `agent_crew` tool directly creating `Agent` instances.

**Orchestrated session spawning** (`session_manager.rs:1190-1260`):
```rust
let config = AcpSessionConfig::new(new_sid, cwd)
    .is_subagent(true)
    .initial_agent_name(agent_str)
    .user_embedded_msg(embedded_msg);
session_tx.start_session(&new_sid, config, None).await
```

This goes through the same `SessionManager::handle_request()` path as main sessions, meaning:
- Agent configs are loaded from the same `self.agent_configs` pool
- MCP servers come from the agent config (no registry resolution)
- The `AcpSessionBuilder` creates a full `Agent` with its own `McpManager`

**`agent_crew` tool** (`crates/agent/src/agent/tools/agent_crew.rs:160`):
- Sends `AgentEvent::SessionToolRequest(SessionTool::SpawnSession{...})` 
- The session manager handles the actual spawning
- ⚠️ **No registry data is passed to subagent sessions**

**V1 subagent** (`crates/chat-cli/src/agent/subagent.rs:251`):
- Explicitly calls `resolve_registry_servers_for_agent_config(&mut snapshot.agent_config, registry)`
- Registry data is passed as `Option<&McpRegistryResponse>` through the tool chain

## 6. Current Registry Usage in V2

### Existing `mcp_registry.rs` module
**File**: `crates/chat-cli-v2/src/mcp_registry.rs` (73,597 bytes — substantial implementation)

Contains:
- `McpRegistryClient` — HTTP client to fetch registry data (line 293)
- `McpRegistryResponse` — Registry response model with validation (line 152)
- `McpServerDefinition` — Server definitions (name, remotes, packages)
- `process_mcp_servers()` — Filters servers based on registry mode (line 430)
- `apply_registry_filtering_to_agent()` — Filters agent's MCP servers against registry (line 686)
- `resolve_registry_servers_for_agent_config()` — Resolves registry-type servers to Local/Remote configs (line 784)
- `convert_registry_to_config()` — Converts registry definitions to `CustomToolConfig`
- `CachedRegistry` — TTL-based caching (24 hours)

### Where it's used in V2 today
1. **`crates/chat-cli-v2/src/cli/mcp.rs`** — The `/mcp` slash command uses `McpRegistryClient` for `mcp add` from registry (line 529)
2. **`crates/chat-cli-v2/src/cli/agent/mod.rs:610`** — `apply_registry_filtering()` method on `Agents` struct
3. **`crates/chat-cli-v2/src/api_client/mod.rs:925`** — `get_mcp_config()` returns `(bool, Option<String>)` — enabled flag + registry URL

### What's NOT connected in V2 ACP path
⚠️ **The V2 ACP session initialization path (`execute()` → `SessionManager` → `AcpSession`) does NOT:**
1. Call `get_mcp_config()` to fetch registry URL from the API
2. Fetch registry data via `McpRegistryClient`
3. Call `apply_registry_filtering_to_agent()` on loaded agent configs
4. Call `resolve_registry_servers_for_agent_config()` for subagent configs
5. Pass registry data to orchestrated/subagent sessions

## 7. Settings/Config Flow

```
CLI args (--agent, --model, --trust-all-tools)
  → AcpSpawnArgs { agent, model, trust_all_tools }
    → execute() sets SessionManager fields:
        - session_manager.set_next_agent_name()
        - session_manager.set_next_model_id()
        - SessionManagerBuilder.trust_all_tools()
      → SessionManager stores:
        - next_agent_name: Option<String>
        - next_model_id: Option<String>
        - trust_all_tools: bool
      → AcpSessionBuilder receives:
        - .trust_all_tools(self.trust_all_tools)
        - .model_id(next_model_id)
      → AgentSnapshot.settings.trust_all_tools = builder.trust_all_tools
```

⚠️ **No registry URL flows through this path.** The `AcpSpawnArgs` struct has no registry field. The `SessionManager` has no registry state. The `AcpSessionBuilder` has no registry parameter.

---

## Integration Points for MCP Registry Support

### Point 1: Fetch Registry URL at ACP Startup
**Where**: `execute()` in `crates/chat-cli-v2/src/agent/acp/acp_agent.rs:2688`
**What**: After creating `SessionManager`, call `os.client.get_mcp_config()` to get registry URL, then fetch registry data via `McpRegistryClient`.
**Why**: Registry URL comes from the API (enterprise profile config), needs to be fetched once at startup.

### Point 2: Store Registry Data in SessionManager
**Where**: `SessionManager` struct in `crates/chat-cli-v2/src/agent/acp/session_manager.rs:220`
**What**: Add fields: `registry_data: Option<McpRegistryResponse>`, `registry_cache: Option<CachedRegistry>`, `registry_url: Option<String>`
**Why**: Registry data needs to be shared across all sessions (main + subagents).

### Point 3: Apply Registry Filtering to Agent Configs at Session Creation
**Where**: `SessionManager::handle_request()` in `crates/chat-cli-v2/src/agent/acp/session_manager.rs:444`
**What**: Before creating `AcpSessionBuilder`, apply `resolve_registry_servers_for_agent_config()` to the `LoadedAgentConfig`. This is where ACP MCP servers are already merged — registry resolution should happen here too.
**Why**: Agent configs loaded by `load_agents()` will have registry-type servers dropped during deserialization (see `agent_config/mod.rs:335`). They need to be resolved back to Local/Remote configs using registry data.

### Point 4: Apply Registry Filtering at Agent Config Load Time
**Where**: `SessionManager::spawn()` in `crates/chat-cli-v2/src/agent/acp/session_manager.rs:162`
**What**: After `load_agents()`, apply `apply_registry_filtering_to_agent()` to filter agent tools against registry.
**Alternative**: Could be done lazily at session creation time (Point 3).

### Point 5: Subagent/Orchestrated Session Registry Propagation
**Where**: `handle_spawn_orchestrated()` in `crates/chat-cli-v2/src/agent/acp/session_manager.rs:1190`
**What**: Orchestrated sessions go through the same `start_session()` path, so if Point 3 is implemented correctly, subagents will automatically get registry-resolved configs.
**Verify**: Ensure the `base_agent_config` lookup for subagent agent names also gets registry resolution.

### Point 6: Agent Swap with Registry
**Where**: `handle_swap_agent()` in `crates/agent/src/agent/mod.rs:1352`
**What**: When swapping agents (e.g., `/agent switch`), the new `LoadedAgentConfig` needs registry resolution before `LoadedMcpServerConfigs::from_agent_config()` is called.
**How**: The swap is initiated from `AcpSession` which calls `agent.swap_agent(SwapAgentArgs{...})`. The `SwapAgentArgs` config should be registry-resolved before being sent.

### Point 7: McpServerConfig Registry Variant (Optional)
**Where**: `crates/agent/src/agent/agent_config/definitions.rs:531`
**What**: Consider adding a `Registry` variant to `McpServerConfig` so registry-type servers survive deserialization and can be resolved later.
**Current behavior**: Registry-type entries in `mcp.json` are silently dropped during deserialization (line 335 in `agent_config/mod.rs`).
**Trade-off**: Adding a variant means the `agent` crate needs registry awareness. The current approach of resolving at the `chat-cli-v2` layer keeps the `agent` crate registry-agnostic.

---

## Summary of Gaps

| Component | V1 Status | V2 Status | Gap |
|-----------|-----------|-----------|-----|
| Registry URL fetch from API | ✅ `chat/mod.rs:443` | ❌ Not called in ACP path | Need to add to `execute()` or `SessionManager` |
| Registry data fetch | ✅ `chat/mod.rs:548` | ❌ Not called | Need `McpRegistryClient::fetch_registry()` call |
| Agent config filtering | ✅ `chat/mod.rs:556` | ❌ Not applied | Need `apply_registry_filtering_to_agent()` |
| Registry server resolution | ✅ `subagent.rs:251` | ❌ Not applied | Need `resolve_registry_servers_for_agent_config()` |
| Subagent registry propagation | ✅ Via `registry_data` param | ❌ Not propagated | Handled automatically if SessionManager has registry data |
| Registry caching | ✅ `conversation.rs:265` | ❌ No cache | Need `CachedRegistry` in SessionManager |
| MCP enabled check | ✅ `chat/mod.rs:443` | ❌ Not checked | Need `is_mcp_enabled()` check |
