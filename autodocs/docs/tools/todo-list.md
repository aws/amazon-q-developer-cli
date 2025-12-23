---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: todo_list
  description: Create and manage TODO lists for tracking multi-step tasks with progress and context
  keywords: [todo_list, todo, task, checklist, tracking]
  related: [slash-todo]
---

# todo_list

Create and manage TODO lists for tracking multi-step tasks with progress and context.

## Overview

The todo_list tool creates persistent TODO lists for multi-step tasks. Lists are stored in `.kiro/cli-todo-lists/` and track task completion, context, and modified files. Essential for complex tasks requiring multiple steps. Lists can be resumed across sessions.

## How It Works

Create list with tasks before starting work. Mark tasks complete as you finish them. Add context and track modified files. Lists persist across sessions. Use `/todo` slash commands to view, resume, or delete lists.

## Usage

### Basic Usage

```json
{
  "command": "create",
  "tasks": [
    {"task_description": "Read requirements"},
    {"task_description": "Implement feature"},
    {"task_description": "Write tests"}
  ],
  "todo_list_description": "Implement user authentication"
}
```

### Common Use Cases

#### Use Case 1: Create TODO List

```json
{
  "command": "create",
  "tasks": [
    {"task_description": "Analyze codebase structure"},
    {"task_description": "Identify refactoring opportunities"},
    {"task_description": "Implement changes"},
    {"task_description": "Run tests"}
  ],
  "todo_list_description": "Refactor authentication module"
}
```

**What this does**: Creates new TODO list with 4 tasks. Returns list ID for tracking.

#### Use Case 2: Mark Tasks Complete

```json
{
  "command": "complete",
  "current_id": "1234567890",
  "completed_indices": [0, 1],
  "context_update": "Analyzed codebase. Found 3 areas needing refactoring."
}
```

**What this does**: Marks tasks 0 and 1 as complete. Adds context about progress.

#### Use Case 3: Add New Tasks

```json
{
  "command": "add",
  "current_id": "1234567890",
  "new_tasks": [
    {"task_description": "Update documentation"},
    {"task_description": "Review PR"}
  ],
  "insert_indices": [3, 4]
}
```

**What this does**: Inserts new tasks at positions 3 and 4.

#### Use Case 4: Remove Tasks

```json
{
  "command": "remove",
  "current_id": "1234567890",
  "remove_indices": [2],
  "new_description": "Refactor authentication - simplified scope"
}
```

**What this does**: Removes task at index 2. Updates list description.

#### Use Case 5: Load Existing List

```json
{
  "command": "load",
  "load_id": "1234567890"
}
```

**What this does**: Loads existing TODO list by ID.

## Configuration

Enable TODO list feature:

```bash
kiro-cli settings chat.enableTodoList true
```

No agent configuration needed - todo_list is trusted by default.

## Commands

### create

Create new TODO list.

**Parameters**:
- `tasks` (array, required): List of tasks
  - `task_description` (string, required): Task description
  - `details` (string, optional): Additional task details
- `todo_list_description` (string, required): Brief summary of TODO list

**Returns**: List ID and serialized state.

### complete

Mark tasks as complete.

**Parameters**:
- `current_id` (string, required): TODO list ID
- `completed_indices` (array, required): 0-indexed task numbers to mark complete
- `context_update` (string, required): Important context about progress
- `modified_files` (array, optional): Paths of files modified

**Note**: Mark tasks IMMEDIATELY after completion. Tasks should be marked in order.

### add

Add new tasks to list.

**Parameters**:
- `current_id` (string, required): TODO list ID
- `new_tasks` (array, required): Tasks to add
- `insert_indices` (array, required): 0-indexed positions for new tasks
- `new_description` (string, optional): Updated list description

### remove

Remove tasks from list.

**Parameters**:
- `current_id` (string, required): TODO list ID
- `remove_indices` (array, required): 0-indexed positions to remove
- `new_description` (string, optional): Updated list description

### load

Load existing TODO list.

**Parameters**:
- `load_id` (string, required): TODO list ID

### lookup

List all existing TODO list IDs.

**Parameters**: None

## Examples

### Example 1: Multi-Step Feature Implementation

```json
{
  "command": "create",
  "tasks": [
    {"task_description": "Design API endpoints", "details": "RESTful design with versioning"},
    {"task_description": "Implement database schema"},
    {"task_description": "Create API handlers"},
    {"task_description": "Write unit tests"},
    {"task_description": "Write integration tests"},
    {"task_description": "Update API documentation"}
  ],
  "todo_list_description": "Implement user profile API"
}
```

### Example 2: Mark Progress

```json
{
  "command": "complete",
  "current_id": "1234567890",
  "completed_indices": [0, 1, 2],
  "context_update": "Designed API with 5 endpoints. Implemented schema with user, profile, and settings tables. Created handlers for all endpoints.",
  "modified_files": ["src/api/profile.ts", "src/db/schema.sql", "src/handlers/profile.ts"]
}
```

### Example 3: Adjust Plan

```json
{
  "command": "add",
  "current_id": "1234567890",
  "new_tasks": [
    {"task_description": "Add rate limiting"},
    {"task_description": "Add caching layer"}
  ],
  "insert_indices": [3, 4],
  "new_description": "Implement user profile API with performance optimizations"
}
```

## Troubleshooting

### Issue: "TODO list feature not enabled"

**Symptom**: Tool returns error  
**Cause**: Feature not enabled  
**Solution**: Enable with `kiro-cli settings chat.enableTodoList true`

### Issue: Can't Find TODO List

**Symptom**: Load fails with ID not found  
**Cause**: Invalid ID or list deleted  
**Solution**: Use `"command": "lookup"` to see all list IDs.

### Issue: Wrong Tasks Marked Complete

**Symptom**: Incorrect tasks shown as complete  
**Cause**: Wrong indices provided  
**Solution**: Indices are 0-based. First task is 0, second is 1, etc.

### Issue: Context Not Saved

**Symptom**: Context missing when resuming  
**Cause**: context_update not provided in complete command  
**Solution**: Always provide context_update when marking tasks complete.

## Related Features

- [/todo](../slash-commands/todo.md) - Slash commands for TODO list management
- [use_subagent](use-subagent.md) - Delegate tasks to subagents
- [delegate](delegate.md) - Background task execution

## Limitations

- Lists stored locally in `.kiro/cli-todo-lists/`
- No cloud sync or sharing
- One active list per conversation (can have multiple saved)
- Task indices are 0-based
- No task dependencies or scheduling
- No automatic task completion detection

## Technical Details

**Aliases**: `todo_list`

**Storage**: Lists stored as JSON in `.kiro/cli-todo-lists/` directory.

**State**: Each list contains:
- `tasks`: Array of task objects with description, completed status, details
- `description`: List summary
- `context`: Array of context updates from complete commands
- `modified_files`: Array of file paths modified during work
- `id`: Unique identifier (timestamp-based)

**Permissions**: Trusted by default. Requires `chat.enableTodoList` setting enabled.

**Best Practice**: Create TODO list BEFORE executing steps. Mark tasks AS YOU COMPLETE THEM. Complete tasks in order provided.

**Display**: DO NOT display your own tasks or todo list - this is done automatically by the system.
