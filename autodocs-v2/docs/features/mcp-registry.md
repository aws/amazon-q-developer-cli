---
doc_meta:
  validated: 2026-05-02
  commit: a433349d
  status: validated
  testable_headless: false
  category: feature
  title: MCP Registry
  description: Enterprise MCP server security allowing administrators to control which servers users can access
  keywords: [mcp, registry, security, enterprise, admin, pro, governance]
  related: [cmd-mcp, slash-mcp, agent-configuration]
---

# MCP Registry

Enterprise MCP server security allowing administrators to control which servers users can access.

## Overview

Pro-tier customers using IAM Identity Center can have MCP server access controlled through an MCP registry. When configured by administrators, users can only use MCP servers explicitly allowed in the registry. Provides centralized security for enterprise deployments.

## MCP Governance Toggle

Administrators can completely disable MCP functionality via the Kiro console. When the MCP toggle is off:

- All MCP servers are suppressed (user-configured, legacy, registry, and session-injected)
- The `/mcp` panel shows "MCP has been disabled by your administrator"
- No MCP tools are available to the agent

This applies to enterprise users (IAM Identity Center) and API key users. Builder ID and social auth users are not subject to MCP governance.

## How It Works

**Without Registry** (default):
- Users can add any MCP server
- Servers configured in agent files or via CLI
- No central control

**With Registry** (enterprise):
- Administrator configures allowed servers
- Users select from registry list
- Cannot add custom servers
- Centralized security and security

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

### Example 4: Registry Server in Agent Config

In agent configurations, you can explicitly reference registry servers:

```json
{
  "mcpServers": {
    "github": {
      "type": "registry"
    }
  }
}
```

The server is resolved from the registry before the agent launches. This is useful when you want to reference a registry server without specifying full connection details.

## Troubleshooting

### Issue: "MCP has been disabled by your administrator"

**Symptom**: Cannot use MCP at all, `/mcp` panel shows warning  
**Cause**: Administrator turned off the MCP toggle in the Kiro console  
**Solution**: Contact your administrator to request MCP access. This is an organization-level setting that only admins can change.

### Issue: "Failed to retrieve MCP settings — MCP disabled"

**Symptom**: MCP disabled with API failure message  
**Cause**: Could not reach the governance API to check MCP settings. For security, MCP is disabled when settings cannot be verified (fail-closed).  
**Solution**: This is usually temporary. Retry later. If persistent, check network connectivity or contact your administrator.

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
- Global: `~/.kiro/settings/mcp.json`
- Workspace: `.kiro/settings/mcp.json`
- Agent-specific: In agent configuration

**Customization**: Environment variables and HTTP headers can be customized even in registry mode

**Fallback**: If registry unavailable, MCP functionality disabled

**Documentation**: For administrators, see [MCP Governance Documentation](https://kiro.dev/docs/cli/mcp/security/)
