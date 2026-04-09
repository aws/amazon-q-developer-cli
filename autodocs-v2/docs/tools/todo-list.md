---
doc_meta:
  title: todo_list
  description: Task list tool for tracking multi-step work with create, complete, add, remove, and list commands
  category: tool
  keywords: [todo_list, todo, task, task_list, plan, checklist, tracking]
  related: [switch-to-execution]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The task list tool creates and tracks multi-step tasks. It is used automatically when the assistant receives a task that requires multiple steps. Tasks are created before execution begins and marked off as they are completed.

## Usage

The tool uses a `command` field to select the operation.

### create

Create a new task list, replacing any existing one.

- `tasks` (array, required) — List of tasks, each with:
  - `task_description` (string, required) — The task description
  - `details` (string, optional) — Additional details
- `task_list_description` (string, required) — Brief summary of the overall task list

### complete

Mark tasks as completed and record context.

- `completed_task_ids` (array, required) — IDs of completed tasks
- `context_update` (string, required) — Important context about what was done
- `modified_files` (array, optional) — Paths of files modified during the task

### add

Add new tasks to an existing task list.

- `new_tasks` (array, required) — Tasks to add (same format as `create`)
- `new_description` (string, optional) — Updated task list description if the goal changed

### remove

Remove tasks from the list.

- `remove_task_ids` (array, required) — IDs of tasks to remove
- `new_description` (string, optional) — Updated task list description if the goal changed

### list

Show all current tasks and their status. No parameters required.

## Examples

### Create a task list

```json
{
  "command": "create",
  "task_list_description": "Add user authentication to the API",
  "tasks": [
    {"task_description": "Create user model and migration"},
    {"task_description": "Implement login endpoint"},
    {"task_description": "Add JWT token generation"},
    {"task_description": "Write tests for auth flow"}
  ]
}
```

### Mark a task complete

```json
{
  "command": "complete",
  "completed_task_ids": ["1"],
  "context_update": "Created User model with email, password_hash fields. Migration adds users table.",
  "modified_files": ["src/models/user.rs", "migrations/001_create_users.sql"]
}
```

### Add tasks mid-execution

```json
{
  "command": "add",
  "new_tasks": [
    {"task_description": "Add rate limiting to login endpoint"}
  ],
  "new_description": "Add user authentication with rate limiting to the API"
}
```

## Troubleshooting

### "No tasks were provided"

The `tasks` or `new_tasks` array is empty. Provide at least one task.

### "No task description was provided"

The `task_list_description` is empty or missing for the `create` command.

### Task ID not found

The ID in `completed_task_ids` or `remove_task_ids` doesn't match any existing task. Use the `list` command to see current task IDs.

## Related

- [switch-to-execution](switch-to-execution.md) — Signals transition from planning to execution
