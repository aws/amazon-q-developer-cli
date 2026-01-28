# Agent Swap MCP Server Reload

**Status:** Complete
**Created:** 2026-01-20
**Completed:** 2026-01-20

## Problem Statement

Complete the `handle_swap_agent` routine in `crates/agent/src/agent/mod.rs` to properly:
1. Unload existing MCP servers (from the previous agent config)
2. Load/launch MCP servers associated with the new agent config

## Solution Summary

Extended `SwapAgentArgs` with MCP path fields and implemented the full swap logic in `handle_swap_agent`. The MCP paths are derived on-demand from `Os` via `PathResolver` at the call site, avoiding the need to store them on the `AcpSession` struct.

## Implementation

### Changes Made

1. **`crates/agent/src/agent/protocol.rs`**
   - Added `PathBuf` import
   - Extended `SwapAgentArgs` with `local_mcp_path` and `global_mcp_path` fields

2. **`crates/agent/src/agent/mod.rs`**
   - Added `McpManager` import
   - Implemented `handle_swap_agent`:
     - Terminates existing MCP servers
     - Creates new `McpManager`
     - Updates agent config and clears tool spec cache
     - Reloads `cached_mcp_configs` from new agent config
     - Launches new MCP servers

3. **`crates/chat-cli/src/agent/acp/acp_agent.rs`**
   - Updated `SwapAgent` request handler to derive MCP paths from `Os` via `PathResolver`
   - Passes paths to `SwapAgentArgs`

### Final Implementation (handle_swap_agent)

```rust
async fn handle_swap_agent(&mut self, args: SwapAgentArgs) -> Result<AgentResponse, AgentError> {
    if !matches!(self.active_state(), ActiveState::Idle) {
        return Err(AgentError::NotIdle);
    }

    // 1. Terminate existing MCP servers
    self.mcp_manager_handle.terminate();

    // 2. Create new MCP manager (terminate kills the old one)
    self.mcp_manager_handle = McpManager::default().spawn();

    // 3. Update agent config and clear caches
    self.agent_config = args.agent_config;
    self.cached_tool_specs = None;

    // 4. Reload MCP configs from new agent config
    self.cached_mcp_configs = LoadedMcpServerConfigs::from_agent_config(
        &self.agent_config,
        args.local_mcp_path.as_ref(),
        args.global_mcp_path.as_ref(),
    ).await;

    // 5. Launch new MCP servers
    for config in self.cached_mcp_configs.configs.iter().filter(|c| c.is_enabled()) {
        if let Err(e) = self.mcp_manager_handle
            .launch_server(config.server_name.clone(), config.config.clone())
            .await
        {
            warn!(?config.server_name, ?e, "failed to launch MCP server during swap");
        }
    }

    Ok(AgentResponse::SwapComplete)
}
```

## Testing

- ✅ All 82 agent crate unit tests pass
- ✅ All 15 ACP integration tests pass (1 ignored, unrelated)
- ✅ New test `agent_swap_reloads_mcp_servers` verifies:
  - Tool A from agent A works before swap
  - Tool B from agent B works after swap
  - No orphaned MCP server processes after swap
