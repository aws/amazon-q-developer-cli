---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: slash_command
  title: /tools
  description: View available tools and manage tool permissions with trust, untrust, and reset operations
  keywords: [tools, permissions, trust, approve]
  related: [agent-configuration]
---

# /tools

View available tools and manage tool permissions with trust, untrust, and reset operations.

## Overview

The `/tools` command displays all available tools (native and MCP) with their permission status. Subcommands allow trusting/untrusting tools to control approval prompts during session.

## Usage

### View Tools

```
/tools
```

Shows all tools with permission labels.

### Manage Permissions

```
/tools trust <tool-names>
/tools untrust <tool-names>
/tools trust-all
/tools reset
```

## Subcommands

### (no subcommand)

Display all available tools and permissions.

```
/tools
```

Shows:
- Native tools (built-in)
- MCP server tools (by server)
- Permission status for each tool

**Permission Labels**:
- `allowed` - Auto-approved
- `requires-approval` - Requires confirmation
- `denied` - Tool is blocked

### trust

Trust specific tools for session.

```
/tools trust <tool-names...>
```

Tools will not prompt for confirmation.

### untrust

Revert tools to per-request confirmation.

```
/tools untrust <tool-names...>
```

### trust-all

Trust all tools (no confirmation prompts).

```
/tools trust-all
```

### reset

Reset all tools to agent's default permissions.

```
/tools reset
```

Removes session trust changes, restores agent configuration.

## Examples

### Example 1: View Tools

```
/tools
```

**Output**:
```
12 tools available
```

### Example 2: Trust Tool

```
/tools trust fs_write
```

**Output**:
```
fs_write now trusted
```

### Example 3: Trust Multiple Tools

```
/tools trust execute_bash grep
```

**Output**:
```
execute_bash, grep now trusted
```

### Example 4: Untrust Tool

```
/tools untrust fs_write
```

**Output**:
```
fs_write set to per-request confirmation
```

### Example 5: Trust All

```
/tools trust-all
```

**Output**:
```
All tools are now trusted for this session. Tools will run without approval prompts.
```

### Example 6: Reset Permissions

```
/tools reset
```

**Output**:
```
Tool trust has been reset to default permission levels.
```

### Example 7: Invalid Tool Name

```
/tools trust nonexistent_tool
```

**Output**:
```
not found: nonexistent_tool
```

## Tool Permissions

### Permission Levels

1. **allowed** - Auto-approved, no prompts
   - Set via `/tools trust`
   - Or `--trust-all-tools` flag
   - Or `--trust-tools=tool1,tool2` flag
   - Or in agent's `allowedTools` list

2. **requires-approval** - Requires confirmation
   - Default for tools not in allowedTools
   - Prompts before each use

3. **denied** - Tool is blocked from use

### MCP Tools

MCP server tools shown with `@server-name` prefix:
- `@git/git_status` - git_status tool from git server
- Format: `@server-name/tool-name`

## Troubleshooting

### Issue: Tool Not Found

**Symptom**: "not found: tool_name"  
**Cause**: Tool name invalid or not loaded  
**Solution**: Use `/tools` to see available tools. Check spelling.

### Issue: MCP Tools Not Showing

**Symptom**: MCP server tools missing  
**Cause**: MCP servers not loaded or need auth  
**Solution**: Check `/mcp` for server status. Some servers need OAuth.

### Issue: Trust Not Persisting

**Symptom**: Trust resets after session  
**Cause**: `/tools trust` is session-only  
**Solution**: Add tools to agent's `allowedTools` for permanent trust.

### Issue: Can't Untrust Tool

**Symptom**: Tool still trusted after untrust  
**Cause**: Tool in agent's allowedTools  
**Solution**: Remove from agent configuration to require prompts.

## Related Features

- [Agent Configuration](../features/agent-configuration.md) - Permanent tool permissions
- [kiro-cli chat --trust-all-tools](../commands/chat.md) - Trust all at startup
- [kiro-cli chat --trust-tools](../commands/chat.md) - Trust specific at startup
- [/mcp](mcp.md) - Manage MCP servers

## Limitations

- Trust changes are session-only (not saved)
- Can't modify agent's allowedTools from chat
- MCP tools require server to be loaded
- Tool names case-sensitive

## Technical Details

**Native Tools**: Built-in tools always available.

**MCP Tools**: Loaded from MCP servers defined in agent configuration.

**Permission Precedence**:
1. `/tools trust-all` - Trusts everything
2. `/tools trust <tool>` - Session trust
3. Agent's `allowedTools` - Permanent trust
4. Default - Requires confirmation

**Tool Names**: Use full tool names. MCP tools use full name with server prefix.
