---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.introspectTangentMode
  description: Auto-enter tangent mode for introspect questions
  keywords: [setting, introspect, tangent, auto]
  related: [introspect, enable-tangent-mode]
---

# chat.introspectTangentMode

Auto-enter tangent mode for introspect questions.

## Overview

Controls whether to automatically enter tangent mode when asking questions about Kiro CLI itself. When enabled, help and documentation queries are isolated in tangent mode, keeping the main conversation focused on your primary tasks.

## Usage

```bash
kiro-cli settings chat.introspectTangentMode true
```

**Type**: Boolean  
**Default**: `false`

Automatically enters tangent mode when asking Kiro CLI help questions, keeping help separate from main conversation.

## Related

- [introspect](../tools/introspect.md) - Introspect tool
- [chat.enableTangentMode](enable-tangent-mode.md) - Enable tangent mode

## Examples

### Example 1: Enable Auto-Tangent

```bash
kiro-cli settings chat.introspectTangentMode true
```

Kiro CLI help questions automatically enter tangent mode.

### Example 2: Check Status

```bash
kiro-cli settings chat.introspectTangentMode
```

### Example 3: Disable

```bash
kiro-cli settings chat.introspectTangentMode false
```
