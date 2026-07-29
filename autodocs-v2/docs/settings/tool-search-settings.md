---
doc_meta:
  validated: 2026-07-29
  commit: 07ecd6a76
  status: validated
  testable_headless: true
  category: setting
  title: toolSearch
  description: Configure tool search for on-demand MCP tool discovery
  keywords: [setting, tool, search, mcp, discovery, threshold, enabled, minPct, minTokens]
  related: [tool-search, mcp]
---

# toolSearch

Configure tool search for on-demand MCP tool discovery.

## Overview

The `toolSearch` settings control when and how MCP tool schemas are deferred from the context window. When Tool Search is enabled and activation thresholds are met, MCP tool schemas are replaced with a compact list, reducing context usage. The assistant then loads specific tools on demand via the `tool_search` tool.

Tool Search is disabled by default. Enable it when you have many MCP tools configured and want to reduce context window usage.

## Settings

### toolSearch.enabled

Enable or disable tool search.

**Type**: Boolean  
**Default**: `false`

```bash
kiro-cli settings toolSearch.enabled true
```

### toolSearch.minPct

Minimum percentage of the context window that MCP tool specs must occupy before tool search activates.

**Type**: Number  
**Default**: `5` (5% of context window)

```bash
kiro-cli settings toolSearch.minPct 3
```

### toolSearch.minTokens

Minimum token count of MCP tool specs before tool search activates.

**Type**: Number  
**Default**: `50000`

```bash
kiro-cli settings toolSearch.minTokens 20000
```

## Activation Logic

Tool Search activates when **both** conditions are true:
1. `toolSearch.enabled` is `true`
2. MCP tool specs exceed **either** threshold: `minPct` of context window **or** `minTokens` tokens

To force activation whenever any MCP tools are present:

```bash
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

## Examples

### Example 1: Enable Tool Search

```bash
kiro-cli settings toolSearch.enabled true
```

### Example 2: Lower Activation Thresholds

```bash
kiro-cli settings toolSearch.minPct 2
kiro-cli settings toolSearch.minTokens 5000
```

### Example 3: Always Activate When MCP Tools Present

```bash
kiro-cli settings toolSearch.enabled true
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

### Example 4: Check Current Values

```bash
kiro-cli settings toolSearch.enabled
kiro-cli settings toolSearch.minPct
kiro-cli settings toolSearch.minTokens
```

### Example 5: Reset to Defaults

```bash
kiro-cli settings --delete toolSearch.enabled
kiro-cli settings --delete toolSearch.minPct
kiro-cli settings --delete toolSearch.minTokens
```

## Troubleshooting

### Issue: Tool Search Not Activating

**Symptom**: MCP tools still appear as full schemas in context  
**Cause**: `toolSearch.enabled` is `false` (default) or thresholds not met  
**Solution**: Enable with `kiro-cli settings toolSearch.enabled true`. If enabled but not activating, lower the thresholds.

### Issue: Tools Not Discoverable

**Symptom**: Assistant cannot find MCP tools  
**Cause**: Tool search activated but assistant not using `tool_search`  
**Solution**: The assistant automatically uses `tool_search` when tool schemas are deferred. If tools are missing, check MCP server connectivity with `/mcp`.

### Issue: Context Still High After Enabling

**Symptom**: Context usage doesn't decrease after enabling tool search  
**Cause**: MCP tool specs may be below thresholds, or built-in tools are not affected  
**Solution**: Lower thresholds or check `/context` to see actual tool token usage. Tool search only defers MCP tools, not built-in tools.

## Related

- [tool_search](../tools/tool-search.md) - The tool used for on-demand discovery
- [/mcp](../slash-commands/mcp.md) - Manage MCP servers
- [/context](../slash-commands/context.md) - View context usage breakdown
