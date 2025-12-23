---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableThinking
  description: Enable thinking tool for complex reasoning
  keywords: [setting, thinking, reasoning, experimental]
  related: [thinking-tool]
---

# chat.enableThinking

Enable thinking tool for complex reasoning.

## Overview

Controls whether to enable the thinking tool for complex problem-solving. When enabled, provides the `thinking` tool that allows the AI to work through complex problems step-by-step in a structured way before providing final answers.

## Usage

```bash
kiro-cli settings chat.enableThinking true
```

**Type**: Boolean  
**Default**: `false`

## Related

- [thinking](../tools/thinking.md) - Thinking tool
- [/experiment](../slash-commands/experiment.md) - Alternative enable method

## Examples

### Example 1: Enable Thinking

```bash
kiro-cli settings chat.enableThinking true
```

AI can now use thinking tool for complex reasoning.

### Example 2: Check Status

```bash
kiro-cli settings chat.enableThinking
```

**Output**: `true` or `false`

### Example 3: Disable

```bash
kiro-cli settings chat.enableThinking false
```
