# Slash Commands

Slash commands provide quick access to specific Q CLI functionality during chat sessions. Commands are prefixed with `/` and can be used at any time during a conversation.

## New: To-Do List Management (`/todos`)

The `/todos` command allows you to view, manage, and resume to-do lists created by Q during multi-step tasks.

### `/todos resume`
Select and resume work on an existing to-do list. Q will load the selected list and continue working on incomplete tasks.

**Usage:**
```
/todos resume
```

Opens an interactive selector showing all available to-do lists with their completion status:
- ✓ (green) indicates completed to-do lists
- ✗ (red) indicates in-progress to-do lists with completion count (e.g., "2/5")

### `/todos view`
View the contents of a to-do list without resuming work on it.

**Usage:**
```
/todos view
```

Displays the selected to-do list with:
- Task descriptions
- Completion status (☐ for incomplete, ■ for complete)
- Overall progress

### `/todos delete`
Delete one or more to-do lists.

**Usage:**
```
/todos delete          # Delete a single selected to-do list
/todos delete --all    # Delete all to-do lists
```

The `--all` flag will delete all to-do lists without prompting for selection.

### `/todos clear-finished`
Remove all completed to-do lists (where all tasks are marked as complete).

**Usage:**
```
/todos clear-finished
```

This command automatically identifies and removes only fully completed to-do lists, leaving in-progress lists intact.

## Other Available Commands

Q CLI includes many other slash commands for various functionality:

### Basic Commands
- `/help` - Show help information
- `/clear` - Clear the conversation
- `/quit` - Exit the chat session

### Agent Management
- `/agent` - Agent-related commands
- `/agent list` - List available agents
- `/agent create` - Create a new agent
- `/agent delete` - Delete an agent

### Tool Management
- `/tools` - Show available tools
- `/tools trust` - Trust specific tools
- `/tools untrust` - Untrust specific tools

### Context Management
- `/context` - Context-related commands
- `/context show` - Show current context
- `/context add` - Add context
- `/context clear` - Clear context

### Session Management
- `/save` - Save current conversation
- `/load` - Load a saved conversation
- `/compact` - Compact conversation history

### Other Features
- `/model` - Model selection
- `/usage` - Show usage information
- `/subscribe` - Subscription management

## Interactive Selection

Most `/todos` commands use an interactive fuzzy selector that allows you to:
- Type to filter to-do lists by description
- Use arrow keys to navigate
- Press Enter to select
- Press Escape to cancel

The selector displays to-do lists with their completion status and description for easy identification.

## Integration with Q's Workflow

- Q automatically creates to-do lists when given multi-step tasks
- To-do lists persist across chat sessions
- Use `/todos resume` to continue interrupted work
- To-do list IDs are included in conversation summaries for seamless resumption

## Storage Location

To-do lists are stored locally in:
```
.amazonq/cli-todo-lists/
```

This directory is created automatically in your current working directory when needed.
