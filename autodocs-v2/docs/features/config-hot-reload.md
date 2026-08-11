---
doc_meta:
  title: Config Hot-Reload
  description: Live reload of agent configs and MCP servers without restarting or losing conversation context
  category: feature
  keywords: [hot-reload, reload, live, config, agent, mcp, servers, watch, file, change, edit, reconcile]
  related: [agent-configuration, mcp-registry, slash-mcp]
  validated: 2026-06-22
  commit: 118327875
  status: validated
  testable_headless: false
---

# Config Hot-Reload

Live reload of agent configs and MCP servers without restarting or losing conversation context.

## Overview

When you edit an agent configuration file or `mcp.json`, changes are automatically detected and applied to your running session. Only affected MCP servers are restarted — unchanged servers keep running. Your conversation history and context are preserved.

This eliminates the need to restart the CLI or start a new session when tweaking agent settings or MCP server configurations.

## How It Works

The CLI watches these configuration directories for changes:
- `~/.kiro/agents/` — Global agent configurations
- `.kiro/agents/` — Workspace agent configurations (when `.kiro/` exists)
- Directories containing `mcp.json` files

When a change is detected:
1. Agent configurations are reloaded from disk
2. MCP servers are reconciled surgically:
   - **Added servers** — Started
   - **Removed servers** — Stopped
   - **Changed servers** — Restarted (stopped then started)
   - **Unchanged servers** — Left running

Changes are applied when the session is idle (not during an active response).

## What Gets Reloaded

### Agent Configuration Changes

Any field in your agent JSON is reloaded:
- `prompt` — System prompt updates
- `tools` — Available tool changes (note: the tool-spec cache is only rebuilt when the MCP reconcile plan is non-empty, i.e., when `mcpServers` actually changed; a tools-only edit updates the in-memory config but won't change the session's active tool list until an MCP server change also occurs)
- `allowedTools` — Auto-approval changes
- `toolsSettings` — Tool configuration changes
- `resources` — Context file changes (note: like `tools`, resource subscriptions are only re-evaluated when the MCP reconcile plan is non-empty; a resources-only edit without an accompanying MCP server change does not refresh resource paths)
- `hooks` — Hook configuration changes
- `mcpServers` — MCP server changes (see below)

### MCP Server Changes

The reconciliation is surgical — only changed servers are affected:

| Change | Action |
|--------|--------|
| New server added to config | Server starts |
| Server removed from config | Server stops |
| Server `command` changed | Server restarts |
| Server `args` changed | Server restarts |
| Server `env` values changed | Server restarts |
| Server `url` changed | Server restarts |
| Server `disabled` toggled | Server starts/stops |
| Any other field changed (e.g. `disabledTools`, `timeout_ms`) | Server restarts |
| `env` key order changed (no value change) | No action (unchanged) |

### mcp.json Changes

Changes to `mcp.json` files are also detected:
- `~/.kiro/settings/mcp.json` — Global MCP configuration
- `.kiro/settings/mcp.json` — Workspace MCP configuration

## Usage

No commands needed — hot-reload is automatic.

### Typical Workflow

1. Start a chat session:
   ```bash
   kiro-cli chat
   ```

2. Edit your agent configuration in another terminal or editor:
   ```bash
   # Edit the agent config
   vim ~/.kiro/agents/my-agent.json
   ```

3. Save the file — changes are applied automatically

4. Continue your conversation with the updated configuration

### Example: Adding an MCP Server

1. Open your agent config:
   ```json
   {
     "name": "my-agent",
     "tools": ["read", "write"],
     "mcpServers": {}
   }
   ```

2. Add a new MCP server:
   ```json
   {
     "name": "my-agent",
     "tools": ["read", "write"],
     "mcpServers": {
       "git": {
         "command": "mcp-server-git",
         "args": ["--stdio"]
       }
     }
   }
   ```

3. Save — the git MCP server starts automatically

4. Use `/mcp` to verify the server is running

### Example: Updating Environment Variables

1. Change an MCP server's environment:
   ```json
   {
     "mcpServers": {
       "github": {
         "command": "mcp-server-github",
         "env": {
           "GITHUB_TOKEN": "$NEW_TOKEN"
         }
       }
     }
   }
   ```

2. Save — only the github server restarts; other servers continue running

### Example: Disabling a Server Temporarily

1. Add `"disabled": true` to skip a server without removing it:
   ```json
   {
     "mcpServers": {
       "slow-server": {
         "command": "mcp-server-slow",
         "disabled": true
       }
     }
   }
   ```

2. Save — the server stops but config is preserved for later

## Debouncing

Rapid consecutive saves (e.g., editor auto-save) are debounced — the CLI waits 500ms after the last change before reloading. This prevents unnecessary server churn during active editing.

## Session-Injected Servers

Servers added via `/mcp add` during a session are persisted to disk by writing directly to the active agent's configuration file. On the next hot-reload cycle the file watcher detects the change and reconciles the new server into the running set.

**Exception**: If the active agent has no file path (e.g., the built-in default agent or an in-memory-only agent), `/mcp add` changes are held in memory only and will be lost when the session ends.

## Examples

### Example 1: Watch Reload in Action

```
You: /mcp
@git (mcp-server-git)
  Status: ✓ Initialized
  Tools: git_status, git_commit, git_log

# In another terminal: edit agent.json, add @github server, save

You: /mcp
@git (mcp-server-git)
  Status: ✓ Initialized
  Tools: git_status, git_commit, git_log

@github (mcp-server-github)
  Status: ✓ Initialized
  Tools: get_issues, create_issue, list_repos
```

### Example 2: Server Restart on Config Change

```
# Initial config has GITHUB_TOKEN=old-token
# Edit config to set GITHUB_TOKEN=new-token and save

# Only @github restarts; @git keeps running
# Logs show:
#   Agent config file change detected, reloading
#   MCP server github restarting (config changed)
```

### Example 3: Removing a Server

```
# Remove "slow-server" from mcpServers in config and save

# Server stops automatically
# Other servers unaffected
```

## Troubleshooting

### Issue: Changes Not Detected

**Symptom**: Edited config file but nothing reloads  
**Cause**: File not in a watched directory, or `.kiro/` doesn't exist for workspace configs  
**Solution**: Ensure the file is in `~/.kiro/agents/` (global) or `.kiro/agents/` (workspace). For workspace, the `.kiro/` directory must exist.

### Issue: Reload Delayed

**Symptom**: Changes take a few seconds to apply  
**Cause**: Debouncing (500ms) or session not idle  
**Solution**: Wait for debounce period. If mid-response, changes apply after completion.

### Issue: Server Not Restarting

**Symptom**: Changed server config but server keeps old behavior  
**Cause**: Only cosmetic changes (e.g., env key reordering) don't trigger restart  
**Solution**: Make an actual value change. The reconciler compares the full serialized JSON config — any field with a different value (command, args, env, url, disabledTools, timeout_ms, etc.) triggers a restart. Only key-order changes within maps are ignored.

### Issue: Session-Injected Server Disappeared

**Symptom**: Server added via `/mcp add` gone after reload  
**Cause**: The active agent has no config file path (e.g., built-in default agent), so changes could not be persisted to disk  
**Solution**: Create a named agent config file (`~/.kiro/agents/<name>.json`) and switch to it, then re-add the server. For agents with a file path, `/mcp add` writes directly to the file and the watcher reconciles it.

### Issue: Reload During Response

**Symptom**: Config change doesn't apply immediately  
**Cause**: Reconciliation waits for idle state to avoid corrupting in-progress turns  
**Solution**: Normal behavior — changes apply after the current response completes.

## Limitations

- Hot-reload only works for agent configs and `mcp.json` — other settings require restart
- Workspace `.kiro/agents/` is only watched if `.kiro/` already exists (won't create `.kiro/` in arbitrary directories)
- Changes during active responses are queued until idle
- Some agent fields (like `model`) may require a full agent swap to take effect

## Related

- [Agent Configuration](agent-configuration.md) — Full agent config reference
- [MCP Registry](mcp-registry.md) — Enterprise MCP server management
- [/mcp](../slash-commands/mcp.md) — View MCP server status
