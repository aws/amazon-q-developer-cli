---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableKnowledge
  description: Enable knowledge base functionality for persistent context storage
  keywords: [setting, knowledge, base, persistent, experimental]
  related: [knowledge-tool, slash-knowledge]
---

# chat.enableKnowledge

Enable knowledge base functionality for persistent context storage.

## Overview

Controls whether to enable the knowledge management system. When enabled, provides the `knowledge` tool for storing, retrieving, and managing persistent context across chat sessions. Useful for maintaining project-specific information and documentation.

## Usage

```bash
kiro-cli settings chat.enableKnowledge true
```

**Type**: Boolean  
**Default**: `false`

## Related

- [knowledge](../tools/knowledge.md) - Knowledge tool
- [/knowledge](../slash-commands/knowledge.md) - Knowledge commands

## Examples

### Example 1: Enable Knowledge

```bash
kiro-cli settings chat.enableKnowledge true
```

Enables knowledge base and `/knowledge` commands.

### Example 2: Check Status

```bash
kiro-cli settings chat.enableKnowledge
```

### Example 3: Disable

```bash
kiro-cli settings chat.enableKnowledge false
```
