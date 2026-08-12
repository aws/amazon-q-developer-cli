---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: toolSearch.minPct
  description: Minimum context window percentage of MCP tool specs to activate Tool Search
  keywords: [setting, tool, search, threshold, percentage, context, window]
  related: [tool-search-enabled]
---

# toolSearch.minPct

Minimum context window percentage of MCP tool specs to activate Tool Search.

## Overview

Sets the context window percentage threshold for Tool Search activation. When `toolSearch.enabled` is true, Tool Search only activates if MCP tool specs exceed this percentage of the model's context window OR exceed `toolSearch.minTokens` (OR logic).

Set to 0 to always activate Tool Search when any MCP tools are present.

## Usage

```bash
kiro-cli settings toolSearch.minPct 5
```

**Type**: Number  
**Default**: `5` (5% of context window)

## Related

- [toolSearch.enabled](./tool-search-enabled.md) - Master toggle

## Examples

### Example 1: Set to 10%

```bash
kiro-cli settings toolSearch.minPct 10
```

Tool Search activates when MCP tool specs exceed 10% of context window.

### Example 2: Always Activate

```bash
kiro-cli settings toolSearch.minPct 0
```

Combined with `toolSearch.minTokens 0`, Tool Search activates whenever any MCP tools are present.

### Example 3: Check Current Value

```bash
kiro-cli settings toolSearch.minPct
```

### Example 4: Reset to Default

```bash
kiro-cli settings toolSearch.minPct 5
```
