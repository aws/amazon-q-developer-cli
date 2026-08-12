---
doc_meta:
  validated: 2026-08-12
  commit: 20bc37f97
  status: validated
  testable_headless: true
  category: setting
  title: chat.autoExpandToolOutput
  description: Always show full tool output inline without truncation
  keywords: [setting, tool, output, expand, truncate, verbose]
  related: [fs-read, execute-bash, code]
---

# chat.autoExpandToolOutput

Always show full tool output inline without truncation.

## Overview

The `chat.autoExpandToolOutput` setting controls whether tool outputs (file reads, command results, search results) are shown in full or truncated with a "ctrl+o to toggle" hint. When enabled, all tool output displays inline without truncation, eliminating the need to press Ctrl+O to expand.

This is useful when you want to see complete tool results immediately, such as when reviewing file contents or command output during debugging sessions.

## Usage

### Enable Auto-Expand

```bash
kiro-cli settings chat.autoExpandToolOutput true
```

### Disable Auto-Expand

```bash
kiro-cli settings chat.autoExpandToolOutput false
```

### Get Current Value

```bash
kiro-cli settings chat.autoExpandToolOutput
```

### Delete Setting

```bash
kiro-cli settings --delete chat.autoExpandToolOutput
```

## Value

**Type**: Boolean  
**Default**: `false` (tool output is truncated with expand hint)  
**Scope**: Workspace-overridable (can be set globally or per-workspace)

## Behavior

| Setting Value | Tool Output Display |
|---------------|---------------------|
| `false` (default) | Truncated preview with "...+N lines (ctrl+o to toggle)" hint |
| `true` | Full output shown inline, no truncation |

When auto-expand is enabled:
- All tool output displays in full immediately
- The Ctrl+O toggle hint is hidden
- No "Viewing detailed tool output" overlay appears
- Scrolling may be needed for large outputs

## Examples

### Example 1: Enable for Verbose Debugging

```bash
kiro-cli settings chat.autoExpandToolOutput true
```

Now when tools run, you see complete output:

```
Reading file.py (lines 1-150)...
[full 150 lines displayed inline]
```

### Example 2: Disable to Reduce Noise

```bash
kiro-cli settings chat.autoExpandToolOutput false
```

Tool output returns to truncated view:

```
Reading file.py (lines 1-150)...
[first 10 lines shown]
...+140 lines (ctrl+o to toggle)
```

### Example 3: Check Current Setting

```bash
kiro-cli settings chat.autoExpandToolOutput
```

**Output**: `true` or `false` (or empty if not set)

## Related

- [fs_read tool](../tools/fs-read.md) - File reading tool whose output is affected
- [execute_bash tool](../tools/execute-bash.md) - Command execution tool whose output is affected
- [code tool](../tools/code.md) - Code intelligence tool whose output is affected

## Troubleshooting

### Issue: Output Still Truncated After Enabling

**Symptom**: Tool output still shows truncation hints  
**Cause**: Setting not applied to current session  
**Solution**: Start a new chat session after changing the setting

### Issue: Too Much Output Cluttering Screen

**Symptom**: Large tool outputs make chat hard to follow  
**Cause**: Auto-expand shows everything inline  
**Solution**: Disable the setting with `kiro-cli settings chat.autoExpandToolOutput false`

### Issue: Setting Not Persisting

**Symptom**: Setting resets between sessions  
**Cause**: May be set at workspace level overriding global  
**Solution**: Check both global and workspace settings, or use `--delete` to clear workspace override
