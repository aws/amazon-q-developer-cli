# To-Do List Functionality

Amazon Q CLI includes built-in to-do list functionality that allows Q to create, manage, and track multi-step tasks. This feature helps organize complex workflows and provides persistence across chat sessions.

## Overview

The to-do list functionality consists of two main components:

1. **`todo_list` tool** - A built-in tool that Q uses to create and manage to-do lists
2. **`/todos` slash command** - User commands for viewing and managing existing to-do lists

## The todo_list Tool

The `todo_list` tool is automatically available to all agents and is trusted by default. Q uses this tool to:

- Create structured to-do lists for multi-step tasks
- Mark tasks as completed as work progresses
- Track context and modified files for each task
- Load and resume existing to-do lists

### Tool Commands

#### `create`
Creates a new to-do list with specified tasks and description.

**Parameters:**
- `tasks` (required): Array of distinct task descriptions
- `todo_list_description` (required): Brief summary of the to-do list

#### `complete`
Marks tasks as completed and updates context.

**Parameters:**
- `completed_indices` (required): Array of 0-indexed task numbers to mark complete
- `context_update` (required): Important context about completed tasks
- `modified_files` (optional): Array of file paths that were modified
- `current_id` (required): ID of the currently loaded to-do list

#### `load`
Loads an existing to-do list by ID.

**Parameters:**
- `load_id` (required): ID of the to-do list to load

#### `add`
Adds new tasks to an existing to-do list.

**Parameters:**
- `new_tasks` (required): Array of new task descriptions
- `insert_indices` (required): Array of 0-indexed positions to insert tasks
- `new_description` (optional): Updated description for the to-do list
- `current_id` (required): ID of the currently loaded to-do list

#### `remove`
Removes tasks from an existing to-do list.

**Parameters:**
- `remove_indices` (required): Array of 0-indexed positions of tasks to remove
- `new_description` (optional): Updated description for the to-do list
- `current_id` (required): ID of the currently loaded to-do list

### Tool Behavior

- Q automatically creates to-do lists when given multi-step tasks
- Tasks are marked as completed immediately after Q finishes each step
- The tool displays progress visually with checkboxes (☐ for incomplete, ■ for complete)
- To-do lists are stored locally in `.amazonq/cli-todo-lists/`
- Each to-do list has a unique timestamp-based ID

## User Commands (/todos)

Users can manage their to-do lists using the `/todos` slash command with various subcommands.

### `/todos resume`

Allows you to select and resume work on an existing to-do list. Q will load the selected list and continue working on incomplete tasks.

**Usage:**
```
/todos resume
```

This opens an interactive selector showing all available to-do lists with their completion status.

### `/todos view`

View the contents of a to-do list without resuming work on it.

**Usage:**
```
/todos view
```

This opens an interactive selector and displays the selected to-do list with all tasks and their completion status.

### `/todos delete`

Delete one or more to-do lists.

**Usage:**
```
/todos delete          # Delete a single selected to-do list
/todos delete --all    # Delete all to-do lists
```

### `/todos clear-finished`

Remove all completed to-do lists (where all tasks are marked as complete).

**Usage:**
```
/todos clear-finished
```

## Storage and Persistence

### Local Storage
To-do lists are stored locally in the current working directory under:
```
.amazonq/cli-todo-lists/
```

Each to-do list is saved as a JSON file named with its unique ID (e.g., `1693234567890.json`).

### Data Structure
Each to-do list contains:
- **tasks**: Array of task descriptions
- **completed**: Array of boolean values indicating completion status
- **description**: Brief summary of the to-do list
- **context**: Array of context updates from completed tasks
- **modified_files**: Array of file paths that were modified during task execution
- **id**: Unique identifier for the to-do list

### Conversation Integration
To-do list IDs are automatically included in conversation summaries, allowing Q to resume work on to-do lists when conversations are loaded from history.

## Best Practices

### For Users
- Use `/todos resume` to continue work on incomplete tasks
- Regularly use `/todos clear-finished` to clean up completed lists
- Use `/todos view` to check progress without resuming work

### For Q's Usage
- Create to-do lists for any multi-step task before beginning work
- Mark tasks as completed immediately after finishing each step
- Provide meaningful context updates when completing tasks
- Track modified files to maintain a record of changes

## Example Workflow

1. User asks Q to implement a new feature
2. Q creates a to-do list with steps like:
   - Analyze requirements
   - Create necessary files
   - Implement core functionality
   - Add tests
   - Update documentation
3. Q works through tasks, marking each as complete with context
4. User can check progress with `/todos view`
5. If interrupted, user can resume with `/todos resume`
6. When finished, user can clean up with `/todos clear-finished`

## Limitations

- To-do lists are stored locally and not synchronized across different working directories
- The interactive selectors require terminal support for user input
- Very long task descriptions may be truncated in the display
- No built-in backup or export functionality for to-do lists
