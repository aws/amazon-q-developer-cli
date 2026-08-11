---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: true
  category: settings-group
  title: MCP Settings
  description: Settings for Model Context Protocol (MCP) configuration
  keywords: [settings, mcp, model, context, protocol, timeout]
---

# MCP Settings

Configure Model Context Protocol (MCP) server settings and timeouts.

## mcp.initTimeout

MCP server initialization timeout.

### Overview

Sets the timeout duration for Model Context Protocol (MCP) server initialization. Controls how long to wait for MCP servers to start up and become ready.

Applies to interactive sessions. One-shot non-interactive runs use
[`mcp.noInteractiveTimeout`](#mcpnointeractivetimeout) instead.

`0` stops the wait immediately, so servers that have not finished starting are
not awaited. A negative value is ignored and the default applies.

### Usage

```bash
kiro-cli settings mcp.initTimeout 10000
```

**Type**: Number  
**Default**: `5000`  
**Unit**: Milliseconds

### Examples

```bash
# Increase for slow servers (30 seconds)
kiro-cli settings mcp.initTimeout 30000

# Decrease for fast servers (2 seconds)
kiro-cli settings mcp.initTimeout 2000

# Check current timeout
kiro-cli settings mcp.initTimeout
```

---

## mcp.noInteractiveTimeout

Non-interactive MCP timeout.

### Overview

Sets the timeout duration for MCP operations in non-interactive mode (batch processing, automated scripts). Defaults higher than the interactive timeout, since an automated run has no user to retry a turn whose servers had not finished loading.

### Usage

```bash
kiro-cli settings mcp.noInteractiveTimeout 30000
```

**Type**: Number  
**Default**: `30000`  
**Unit**: Milliseconds

### Use Cases

- Automated scripts
- CI/CD pipelines
- Batch processing
- Background operations

---

## mcp.loadedBefore

Track previously loaded MCP servers.

### Overview

Controls whether to track which MCP servers have been loaded before. Used for optimization and caching of server configurations.

### Usage

```bash
kiro-cli settings mcp.loadedBefore true
```

**Type**: Boolean  
**Default**: `false`

### Benefits

**Enabled**:
- Faster server loading
- Better caching
- Improved performance

**Disabled**:
- Always fresh server state
- No tracking overhead
- Simpler debugging