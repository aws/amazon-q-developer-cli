---
doc_meta:
  validated: 2026-01-28
  commit: 0fce279f
  status: validated
  testable_headless: false
  category: slash_command
  title: /mcp
  description: View MCP server status, authentication requirements, and available tools
  keywords: [mcp, servers, status, auth, tools]
  related: [cmd-mcp, agent-config]
---

# /mcp

See MCP server loaded and manage MCP servers.

## Overview

The `/mcp` command displays status of MCP servers and provides subcommands to manage them. Shows server state, authentication requirements, available tools, and OAuth URLs if needed.

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

Add an MCP server from the registry (only available if a registry has been configured by admin).

```
/mcp add
```

### remove

Remove an enabled MCP server (only available if a registry has been configured by admin).

```
/mcp remove
```

### help

Print help message or help for specific subcommand.

```
/mcp help
/mcp help <subcommand>
```

## Options

- `-h, --help` - Print help

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
  Status: ⚠ Needs authentication
  OAuth URL: https://github.com/login/oauth/...
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
/mcp help
/mcp help add
```

**Output**:
```
Usage: /mcp add
Add an MCP server from the registry
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
