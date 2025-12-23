---
doc_meta:
  validated: 2025-12-22
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: MCP Registry
  description: Enterprise MCP server governance allowing administrators to control which servers users can access
  keywords: [mcp, registry, governance, enterprise, admin, pro]
  related: [cmd-mcp, slash-mcp, agent-configuration]
---

# MCP Registry

Enterprise MCP server governance allowing administrators to control which servers users can access.

## Overview

Pro-tier customers using IAM Identity Center can have MCP server access controlled through an MCP registry. When configured by administrators, users can only use MCP servers explicitly allowed in the registry. Provides centralized governance for enterprise deployments.

## How It Works

**Without Registry** (default):
- Users can add any MCP server
- Servers configured in agent files or via CLI
- No central control

**With Registry** (enterprise):
- Administrator configures allowed servers
- Users select from registry list
- Cannot add custom servers
- Centralized governance and security

## Adding Servers (Registry Mode)

### In Chat

```
/mcp add
```

Shows interactive list of servers from organization's registry.

### In CLI

```bash
# Add specific server
kiro-cli mcp add --name myserver

# Add to workspace
kiro-cli mcp add --scope workspace

# Add to specific agent
kiro-cli mcp add --agent myagent

# Interactive selection
kiro-cli mcp add
```

**Note**: Server name must match registry. Cannot add custom servers.

## Removing Servers

### In Chat

```
/mcp remove
```

Interactive menu to select server to remove.

### In CLI

```bash
kiro-cli mcp remove --name <server-name>
```

## Viewing Available Servers

### In Chat

```
/mcp list
```

Shows:
- All locally configured MCP servers
- Server status and configuration
- Available tools from each server

## Customization (Registry Mode)

Even with registry, you can customize:

### Local (stdio) Servers
- Environment variables (API keys, paths)
- Request timeout
- Server scope (Global/Workspace/Agent)
- Tool trust settings

### Remote (HTTP) Servers
- HTTP headers (authentication tokens)
- Request timeout
- Server scope
- Tool trust settings

**Custom values override registry defaults**, allowing personal credentials and configuration.

## Examples

### Example 1: Add Registry Server

```
/mcp add
```

**Output**:
```
Select MCP server from registry:
  git-server (Git operations)
  github-server (GitHub integration)
  aws-tools (AWS operations)
```

### Example 2: Add to Specific Agent

```bash
kiro-cli mcp add --name git-server --agent rust-dev
```

Adds git-server to rust-dev agent configuration.

### Example 3: Customize with Environment Variables

```json
{
  "mcpServers": {
    "github": {
      "command": "mcp-server-github",
      "args": ["--stdio"],
      "env": {
        "GITHUB_TOKEN": "$GITHUB_TOKEN"
      }
    }
  }
}
```

Your token overrides registry defaults.

## Troubleshooting

### Issue: "MCP functionality has been disabled by your administrator"

**Symptom**: Cannot use MCP at all  
**Cause**: Organization disabled MCP entirely  
**Solution**: Contact administrator for MCP access

### Issue: "Failed to retrieve MCP settings"

**Symptom**: Error fetching MCP configuration  
**Cause**: Network issue or server error  
**Solution**: Temporary issue - retry later or contact administrator

### Issue: Cannot Add Custom Server

**Symptom**: Server not in list  
**Cause**: Registry mode only allows registry servers  
**Solution**: Request administrator add server to registry

### Issue: Server Not in Registry

**Symptom**: Needed server not available  
**Cause**: Not added to organization's registry  
**Solution**: Contact administrator to request server addition

## Related Features

- [kiro-cli mcp](../commands/mcp.md) - MCP management commands
- [/mcp](../slash-commands/mcp.md) - View MCP status
- [Agent Configuration](agent-configuration.md) - Configure MCP servers in agents

## Limitations

- Registry mode only for Pro-tier with IAM Identity Center
- Cannot add servers not in registry
- Administrator controls available servers
- Custom servers not allowed in registry mode

## Technical Details

**Registry Source**: Configured by administrator at organization level

**Scope Options**:
- Global: `~/.kiro/mcp.json`
- Workspace: `.kiro/mcp.json`
- Agent-specific: In agent configuration

**Customization**: Environment variables and HTTP headers can be customized even in registry mode

**Fallback**: If registry unavailable, MCP functionality disabled

**Documentation**: For administrators, see [MCP Governance Documentation](https://kiro.dev/docs/cli/mcp/governance/)
