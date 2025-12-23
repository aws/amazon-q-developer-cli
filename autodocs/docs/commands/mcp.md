---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli mcp
  description: Manage Model Context Protocol servers with add, remove, list, import, and status operations
  keywords: [mcp, servers, protocol, manage]
  related: [slash-mcp, agent-config]
---

# kiro-cli mcp

Manage Model Context Protocol servers with add, remove, list, import, and status operations.

## Overview

Manages MCP servers that provide additional tools and resources. Add servers, remove them, list available servers, import configurations, and check server status.

## Usage

```bash
kiro-cli mcp <subcommand>
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

## Subcommands

### add

Add MCP server.

```bash
kiro-cli mcp add [OPTIONS]
```

**Parameters**:
- `--name <NAME>`: Server name (optional with registry - shows interactive menu)
- `--scope <SCOPE>`: Scope (default, workspace, global)
- `--command <COMMAND>`: Command to launch server (required for custom servers)
- `--args <ARGS>`: Arguments to pass to command
- `--agent <AGENT>`: Agent to add server to (defaults to global mcp.json)
- `--env <ENV>`: Environment variables
- `--timeout <TIMEOUT>`: Launch timeout in milliseconds
- `--disabled`: Disable server (don't load)
- `--force`: Overwrite existing server

### remove

Remove MCP server.

```bash
kiro-cli mcp remove --name <NAME> [OPTIONS]
```

**Parameters**:
- `--name <NAME>`: Server name (required)
- `--scope <SCOPE>`: Scope (default, workspace, global)
- `--agent <AGENT>`: Agent to remove server from

### list

List configured MCP servers.

```bash
kiro-cli mcp list [SCOPE]
```

**Parameters**:
- `[SCOPE]`: Optional scope filter (default, workspace, global)

### import

Import server configuration from file.

```bash
kiro-cli mcp import --file <FILE> [SCOPE] [OPTIONS]
```

**Parameters**:
- `--file <FILE>`: Configuration file to import (required)
- `[SCOPE]`: Target scope (default, workspace, global)
- `--force`: Overwrite existing servers

### status

Get server status.

```bash
kiro-cli mcp status --name <NAME>
```

**Parameters**:
- `--name <NAME>`: Server name (required)

## Examples

### Example 1: Add Server

```bash
kiro-cli mcp add --name git --command mcp-server-git --args --stdio
```

### Example 2: List Servers

```bash
kiro-cli mcp list
```

### Example 3: Remove Server

```bash
kiro-cli mcp remove --name git
```

### Example 4: Import Configuration

```bash
kiro-cli mcp import --file servers.json workspace
```

### Example 5: Check Server Status

```bash
kiro-cli mcp status --name git
```

## Related

- [/mcp](../slash-commands/mcp.md) - View server status in chat
- [Agent Configuration](../agent-config/overview.md) - Configure in agent

## Troubleshooting

### Issue: Server Not Found

**Symptom**: "Command not found" error  
**Cause**: MCP server binary not in PATH  
**Solution**: Install server or provide full path to command

### Issue: Server Won't Start

**Symptom**: Server fails to initialize  
**Cause**: Invalid args or missing dependencies  
**Solution**: Test command manually: `<command> <args>`. Check server documentation.

### Issue: Can't Remove Server

**Symptom**: Remove fails  
**Cause**: Server not in configuration  
**Solution**: Use `kiro-cli mcp list` to see configured servers
