# MCP Registry

## Overview

Pro-tier customers using IAM Identity Center can have their MCP server access controlled through an MCP registry. When a registry is configured by your administrator, you can only use MCP servers that are explicitly allowed in the registry.

For details on how to configure the registry for your organization, see [MCP Governance Documentation](https://kiro.dev/docs/cli/mcp/governance/).

## Adding MCP Servers

When your organization has configured an MCP registry, you can only add servers from that registry:

### In Chat:
```
/mcp add
```
You'll see a list of servers from your organization's registry that you can select from.

### In CLI:

**Add a specific server by name:**
```bash
kiro-cli mcp add --name myserver
```
The server name must match a server in your organization's registry.

**Add to workspace scope:**
```bash
kiro-cli mcp add --scope workspace
```

**Add to a specific agent:**
```bash
kiro-cli mcp add --agent myagent
```

**Add to default location:**
```bash
kiro-cli mcp add
```
This adds to the default agent if one exists on disk, otherwise adds to workspace scope.

**Combine options:**
```bash
kiro-cli mcp add --name myserver --agent myagent
```

If you omit `--name`, an interactive menu will appear for you to choose from available registry servers.

**Note**: In registry mode, you cannot add custom servers that aren't in your organization's registry. Contact your administrator to request additional servers.

## Removing MCP Servers

### In Chat:
```
/mcp remove
```
An interactive menu will appear for you to select which server to remove.

### In CLI:
```bash
kiro-cli mcp remove --name <server-name>
```

## Viewing Available Servers

### In Chat:
```
/mcp list
```

This shows all servers from your organization's registry and which ones you have installed.

## Customizing MCP Servers

Even in registry mode, you can customize certain aspects of MCP servers:

### For Local (stdio) Servers:
- **Environment Variables**: Add environment variables specific to your setup (e.g., API keys, local paths)
- **Request Timeout**: Change timeout settings
- **Server Scope**: Set to Global, Workspace, or Agent-specific
- **Tool Trust**: Configure which tools are trusted

### For Remote (HTTP) Servers:
- **HTTP Headers**: Add headers like authentication tokens
- **Request Timeout**: Change timeout settings
- **Server Scope**: Set to Global, Workspace, or Agent-specific
- **Tool Trust**: Configure which tools are trusted

**Note**: Your custom environment variables or HTTP headers override registry defaults, allowing you to provide your own credentials or local configuration.

## Troubleshooting

### "MCP functionality has been disabled by your administrator"
Your organization has disabled MCP entirely. Contact your administrator if you need MCP access.

### "Failed to retrieve MCP settings; MCP functionality disabled"
There was an error fetching your MCP configuration from the server. This is usually temporary - try again later or contact your administrator.

### Cannot add custom servers
In registry mode, you can only use servers defined in your organization's registry. Contact your administrator to request additional servers be added to the registry.

## Related Commands

- `/mcp add` - Add a server from the registry
- `/mcp list` - View available servers
- `/mcp remove` - Remove a server (interactive)
- `/mcp` - View status of installed servers
- `kiro-cli mcp add` - CLI command to add servers
- `kiro-cli mcp remove --name <name>` - CLI command to remove servers
