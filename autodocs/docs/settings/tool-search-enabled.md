---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: toolSearch.enabled
  description: Enable on-demand MCP tool discovery to reduce context window usage
  keywords: [setting, tool, search, mcp, discovery, context, deferred]
  related: [tool-search-min-pct]
---

# toolSearch.enabled

Enable on-demand MCP tool discovery to reduce context window usage.

## Overview

Controls whether Tool Search is enabled. When enabled, MCP tool schemas are deferred until needed, reducing context window consumption. The assistant sees a compact list of available tools and loads specific tools on demand using the `tool_search` built-in tool.

## Usage

```bash
kiro-cli settings toolSearch.enabled true
```

**Type**: Boolean  
**Default**: `false`

## Related

- [toolSearch.minPct](./tool-search-min-pct.md) - Minimum context percentage threshold

## Examples

### Example 1: Enable Tool Search

```bash
kiro-cli settings toolSearch.enabled true
```

Enables Tool Search. Activation depends on threshold settings.

### Example 2: Check Status

```bash
kiro-cli settings toolSearch.enabled
```

### Example 3: Disable Tool Search

```bash
kiro-cli settings toolSearch.enabled false
```

Returns to default behavior where all MCP tool schemas are sent on every turn.

### Example 4: Force Always Active

To activate Tool Search whenever any MCP tools are present (ignoring thresholds):

```bash
kiro-cli settings toolSearch.enabled true
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```
