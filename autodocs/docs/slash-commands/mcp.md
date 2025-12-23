---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /mcp
  description: View MCP server status, authentication requirements, and available tools
  keywords: [mcp, servers, status, auth, tools]
  related: [cmd-mcp, agent-config]
---

# /mcp

View MCP server status, authentication requirements, and available tools.

## Overview

Displays status of all MCP servers configured in current agent. Shows server state, authentication requirements, available tools, and OAuth URLs if needed.

## Usage

```
/mcp
```

## Output

Shows for each server:
- Server name and command
- Status (initialized, loading, needs auth)
- Available tools
- OAuth URL (if authentication required)

## Related

- [kiro-cli mcp](../commands/mcp.md) - Manage MCP servers
- [Agent Configuration](../agent-config/overview.md) - Configure MCP servers

## Examples

### Example 1: View MCP Status

```
/mcp
```

**Output**:
```
@git (mcp-server-git)
  Status: ✓ Initialized
  Tools: git_status, git_commit, git_log

@github (mcp-server-github)
  Status: ⚠ Needs authentication
  OAuth URL: https://github.com/login/oauth/...
  Tools: (not loaded)
```

## Troubleshooting

### Issue: Server Not Initialized

**Symptom**: Server shows "loading" or "needs auth"  
**Cause**: Server starting or requires OAuth  
**Solution**: Wait for initialization or complete OAuth flow

### Issue: No Servers Shown

**Symptom**: Empty output  
**Cause**: No MCP servers configured  
**Solution**: Add servers to agent configuration or use `kiro-cli mcp add`

### Issue: Tools Not Available

**Symptom**: Server initialized but tools not working  
**Cause**: Server error or incompatible version  
**Solution**: Check server logs. Verify server version compatibility.
