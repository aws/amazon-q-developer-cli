---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableContextUsageIndicator
  description: Show context usage percentage in prompt
  keywords: [setting, context, usage, indicator, percentage]
  related: [context]
---

# chat.enableContextUsageIndicator

Show context usage percentage in prompt.

## Overview

Controls whether to display context window usage percentage in the chat prompt. When enabled, shows how much of the available context window is being used, helping users understand when they're approaching limits.

## Usage

```bash
kiro-cli settings chat.enableContextUsageIndicator true
```

**Type**: Boolean  
**Default**: `false`

Displays context window usage percentage in prompt.

## Related

- [/context](../slash-commands/context.md) - View context usage

## Examples

### Example 1: Enable Indicator

```bash
kiro-cli settings chat.enableContextUsageIndicator true
```

Prompt shows context usage: `[75%] > `

### Example 2: Check Status

```bash
kiro-cli settings chat.enableContextUsageIndicator
```

### Example 3: Disable

```bash
kiro-cli settings chat.enableContextUsageIndicator false
```
