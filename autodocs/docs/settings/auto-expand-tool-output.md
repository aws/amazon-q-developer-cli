---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: chat.autoExpandToolOutput
  description: Always show full tool output inline without truncation
  keywords: [setting, tool, output, expand, truncate, verbose]
  related: [fs-read, execute-bash, grep]
---

# chat.autoExpandToolOutput

Always show full tool output inline without truncation.

> **Note**: This setting is registered in V1 but has no runtime effect in the V1 CLI. It is only consumed by the V2 TUI. The setting will be parsed without error but will not change tool output display behavior in V1.

## Overview

The `chat.autoExpandToolOutput` setting controls whether tool outputs (file reads, command results, search results) are shown in full or truncated. When enabled, all tool output displays inline without truncation.

## Usage

### Enable Auto-Expand

```bash
kiro-cli settings chat.autoExpandToolOutput true
```

### Disable Auto-Expand

```bash
kiro-cli settings chat.autoExpandToolOutput false
```

### Check Status

```bash
kiro-cli settings chat.autoExpandToolOutput
```

## Value

**Type**: Boolean  
**Default**: `false` (tool output truncated with expand hint)  
**Values**: `true` or `false`

## Behavior

When **disabled** (default):
- Long tool outputs are truncated
- Shows a truncation hint with line count

When **enabled**:
- All tool output shown in full
- No truncation or expand hints

## Examples

### Example 1: Enable Full Output

```bash
kiro-cli settings chat.autoExpandToolOutput true
```

All tool outputs now display completely inline.

### Example 2: Check Current Setting

```bash
kiro-cli settings chat.autoExpandToolOutput
```

**Output**: `true` or `false`

### Example 3: Disable (Return to Default)

```bash
kiro-cli settings chat.autoExpandToolOutput false
```

Tool outputs return to truncated view with expand option.

## Related

- [fs_read](../tools/fs-read.md) - File reading tool
- [execute_bash](../tools/execute-bash.md) - Command execution tool
- [grep](../tools/grep.md) - Search tool

## Troubleshooting

### Issue: Setting Not Taking Effect

**Symptom**: Tool output still truncated after enabling  
**Cause**: This setting has no runtime effect in V1 CLI  
**Solution**: This setting only applies to V2 TUI

### Issue: Too Much Output

**Symptom**: Screen filled with verbose tool output  
**Cause**: Auto-expand shows everything  
**Solution**: Disable with `kiro-cli settings chat.autoExpandToolOutput false`
