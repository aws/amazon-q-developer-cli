---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableTodoList
  description: Enable TODO list feature for task tracking
  keywords: [setting, todo, list, task, experimental]
  related: [todo-list-tool, slash-todo]
---

# chat.enableTodoList

Enable TODO list feature for task tracking.

## Overview

Controls whether to enable the TODO list management system. When enabled, provides the `todo_list` tool for creating, managing, and tracking tasks and project milestones across chat sessions.

## Usage

```bash
kiro-cli settings chat.enableTodoList true
```

**Type**: Boolean  
**Default**: `false`

## Related

- [todo_list](../tools/todo-list.md) - TODO list tool
- [/todo](../slash-commands/todo.md) - TODO commands

## Examples

### Example 1: Enable TODO Lists

```bash
kiro-cli settings chat.enableTodoList true
```

Enables todo_list tool and `/todo` commands.

### Example 2: Check Status

```bash
kiro-cli settings chat.enableTodoList
```

### Example 3: Disable

```bash
kiro-cli settings chat.enableTodoList false
```
