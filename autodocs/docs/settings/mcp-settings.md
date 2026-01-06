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

### Usage

```bash
kiro-cli settings mcp.initTimeout 10
```

**Type**: Number  
**Default**: `10`  
**Unit**: Seconds

### Examples

```bash
# Increase for slow servers
kiro-cli settings mcp.initTimeout 30

# Decrease for fast servers
kiro-cli settings mcp.initTimeout 5

# Check current timeout
kiro-cli settings mcp.initTimeout
```

---

## mcp.noInteractiveTimeout

Non-interactive MCP timeout.

### Overview

Sets the timeout duration for MCP operations in non-interactive mode (batch processing, automated scripts). Typically shorter than interactive timeouts to prevent hanging in automated workflows.

### Usage

```bash
kiro-cli settings mcp.noInteractiveTimeout 5
```

**Type**: Number  
**Default**: `5`  
**Unit**: Seconds

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