---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableCodeIntelligence
  description: Enable code intelligence with LSP integration
  keywords: [setting, code, intelligence, lsp]
  related: [code-tool, slash-code]
---

# chat.enableCodeIntelligence

Enable code intelligence with LSP integration.

## Overview

Controls whether to enable Language Server Protocol (LSP) integration for semantic code understanding. When enabled, provides the `code` tool for precise symbol navigation, definitions, references, and type information across your codebase.

## Usage

```bash
kiro-cli settings chat.enableCodeIntelligence true
```

**Type**: Boolean  
**Default**: `false`

## Related

- [code](../tools/code.md) - Code intelligence tool
- [/code](../slash-commands/code.md) - Code commands

## Examples

### Example 1: Enable Code Intelligence

```bash
kiro-cli settings chat.enableCodeIntelligence true
```

Enables code tool and `/code` commands.

### Example 2: Check Status

```bash
kiro-cli settings chat.enableCodeIntelligence
```

### Example 3: Disable

```bash
kiro-cli settings chat.enableCodeIntelligence false
```
