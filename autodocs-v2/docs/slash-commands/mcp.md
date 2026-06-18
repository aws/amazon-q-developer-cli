---
doc_meta:
  validated: 2026-06-17
  commit: 97c6ac4c6
  status: validated
  testable_headless: false
  category: slash_command
  title: /mcp
  description: View MCP server status, authentication requirements, and available tools
  keywords: [mcp, servers, status, auth, tools, governance, add, remove, persist, oauth, authenticate, clipboard]
  related: [cmd-mcp, agent-config, mcp-registry]
---

# /mcp

See MCP server loaded and manage MCP servers.

## Overview

The `/mcp` command displays status of MCP servers and provides subcommands to manage them. Shows server state, authentication requirements, and available tools. When a server requires OAuth, you can authenticate directly from the panel — the authorization URL is copied to your clipboard for you to open in a browser.

## Usage

```
/mcp [COMMAND]
```

## Commands

### list (default)

List all MCP servers (shows registry servers if configured by admin, or local configured servers).

```
/mcp
/mcp list
```

### add

Add an MCP server from the registry (only available if a registry has been configured by admin). Changes are persisted to your agent's configuration file.

```
/mcp add
```

### remove

Remove an enabled MCP server (only available if a registry has been configured by admin). Changes are persisted to your agent's configuration file.

```
/mcp remove
```

## Output

Shows for each server:
- Server name and command
- Status (initialized, loading, needs auth)
- Available tools
- Authentication action (if OAuth required — press Enter to copy the OAuth URL to your clipboard)

## Related

- [kiro-cli mcp](../commands/mcp.md) - Manage MCP servers
- [Agent Configuration](../features/agent-configuration.md) - Configure MCP servers

## Examples

### Example 1: View MCP Status (Default/List)

```
/mcp
/mcp list
```

**Output**:
```
@git (mcp-server-git)
  Status: ✓ Initialized
  Tools: git_status, git_commit, git_log

@github (mcp-server-github)
  Status: ⚠ Needs authentication · Enter to authenticate
  Tools: (not loaded)
```

### Example 2: Add MCP Server

```
/mcp add
```

**Output**:
```
Select MCP server to add:
  filesystem - File system operations
  database - Database connectivity
  web-scraper - Web scraping tools
```

### Example 3: Remove MCP Server

```
/mcp remove
```

**Output**:
```
Select MCP server to remove:
* @filesystem (enabled)
* @database (enabled)
  @web-scraper (disabled)
```

### Example 4: Get Help

```
## Troubleshooting

### Issue: Server Not Initialized

**Symptom**: Server shows "loading" or "needs auth"  
**Cause**: Server starting or requires OAuth  
**Solution**: Wait for initialization or press Enter on the server in the `/mcp` panel to start authentication. Outside the panel, press Ctrl+y when the status bar shows an OAuth prompt. The OAuth authorization URL will be copied to your clipboard — open it in a browser to complete the OAuth flow.

### Issue: No Servers Shown

**Symptom**: Empty output  
**Cause**: No MCP servers configured  
**Solution**: Add servers to agent configuration or use `kiro-cli mcp add`

### Issue: MCP Disabled by Administrator

**Symptom**: Panel shows "MCP has been disabled by your administrator" or "Failed to retrieve MCP settings — MCP disabled"  
**Cause**: Your organization's administrator has disabled MCP via the Kiro console, or the governance API could not be reached (fail-closed for security)  
**Solution**: Contact your administrator to request MCP access. If the message mentions "Failed to retrieve," this may be temporary — retry later.

### Issue: Tools Not Available

**Symptom**: Server initialized but tools not working  
**Cause**: Server error or incompatible version  
**Solution**: Check server logs. Verify server version compatibility.

### Issue: Tool Excluded Due to Validation Error

**Symptom**: Message "The following tools have been excluded due to validation errors"  
**Cause**: Tool fails validation requirements:
- Tool name exceeds 64 characters (including server prefix)
- Tool name contains invalid characters (must match `^[a-zA-Z][a-zA-Z0-9_]*$`)
- Tool description is empty  
**Solution**: Contact MCP server maintainer to fix tool specification.

### Issue: Large Description Warning

**Symptom**: Message "The following tools have large descriptions which may impact agent performance"  
**Cause**: Tool description exceeds 10,000 characters  
**Solution**: Tool still works but may slow down agent responses. Consider asking server maintainer to shorten description.
