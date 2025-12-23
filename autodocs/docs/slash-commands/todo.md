---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /todo
  description: View, manage, and resume TODO lists with clear-finished, resume, view, and delete operations
  keywords: [todo, task, list, manage, resume]
  related: [todo-list, enable-todo-list]
---

# /todo

View, manage, and resume TODO lists with clear-finished, resume, view, and delete operations.

## Overview

The `/todo` command manages TODO lists created by the todo_list tool. View lists, resume in-progress lists, delete lists, or clear completed lists.

## Usage

```
/todo <subcommand>
```

## Subcommands

### clear-finished

Delete all completed TODO lists.

```
/todo clear-finished
```

### resume

Resume a selected TODO list.

```
/todo resume
```

Shows picker with in-progress lists.

### view

View a TODO list.

```
/todo view
```

Shows picker to select list to view.

### delete

Delete TODO list(s).

```
/todo delete
/todo delete --all
```

## Examples

### Example 1: Resume List

```
/todo resume
```

**Output**:
```
Select TODO list:
  ✗ Implement authentication (2/5)
  ✗ Refactor database (1/3)
```

### Example 2: Clear Completed

```
/todo clear-finished
```

**Output**:
```
✔ Cleared finished to-do lists!
```

### Example 3: View List

```
/todo view
```

Shows complete list with all tasks and status.

## Configuration

Enable TODO lists:

```bash
kiro-cli settings chat.enableTodoList true
```

## Related

- [todo_list](../tools/todo-list.md) - TODO list tool
- [chat.enableTodoList](../settings/enable-todo-list.md) - Enable setting

## Limitations

- Requires feature enabled
- Lists stored locally only
- Interactive picker only

## Technical Details

**Storage**: `.kiro/cli-todo-lists/` directory

**Display**: ✓ for completed, ✗ for in-progress
