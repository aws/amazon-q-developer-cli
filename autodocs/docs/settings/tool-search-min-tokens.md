---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: toolSearch.minTokens
  description: Minimum MCP tool spec token count to activate Tool Search
  keywords: [setting, tool, search, threshold, tokens, context]
  related: [tool-search-enabled, tool-search-min-pct, tool-search-feature]
---

# toolSearch.minTokens

Minimum MCP tool spec token count to activate Tool Search.

## Overview

Sets the token count threshold for Tool Search activation. When `toolSearch.enabled` is true, Tool Search only activates if MCP tool specs exceed this token count OR exceed `toolSearch.minPct` percentage of context window (OR logic).

Set to 0 to always activate Tool Search when any MCP tools are present.

## Usage

```bash
kiro-cli settings toolSearch.minTokens 50000
```

**Type**: Number  
**Default**: `50000` (50k tokens)

## Related

- [toolSearch.enabled](./tool-search-enabled.md) - Master toggle
- [toolSearch.minPct](./tool-search-min-pct.md) - Percentage threshold
- [Tool Search Feature](../features/tool-search.md) - Complete feature documentation

## Examples

### Example 1: Set to 100k Tokens

```bash
kiro-cli settings toolSearch.minTokens 100000
```

Tool Search activates when MCP tool specs exceed 100k tokens.

### Example 2: Always Activate

```bash
kiro-cli settings toolSearch.minTokens 0
```

Combined with `toolSearch.minPct 0`, Tool Search activates whenever any MCP tools are present.

### Example 3: Check Current Value

```bash
kiro-cli settings toolSearch.minTokens
```

### Example 4: Reset to Default

```bash
kiro-cli settings toolSearch.minTokens 50000
```
