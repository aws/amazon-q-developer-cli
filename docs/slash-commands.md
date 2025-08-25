# Slash Commands

Amazon Q CLI provides several slash commands that allow you to interact with the system and manage your workflow without sending messages to the AI model. These commands are prefixed with `/` and provide direct access to various features.

## Available Commands

### `/todos` - To-Do List Management

The `/todos` command provides comprehensive management of to-do lists created by Amazon Q. When Q breaks down complex tasks into manageable steps, you can use these commands to view, resume, and manage your to-do lists.

#### `/todos resume`

Resume working on a selected to-do list. This command presents a fuzzy-searchable list of all incomplete to-do lists and allows you to select one to continue working on.

**Usage:**
```
/todos resume
```

**Behavior:**
- Displays all incomplete to-do lists with progress indicators
- Allows fuzzy search to quickly find specific lists
- Automatically loads the selected to-do list and prompts Q to continue working
- Shows completion status (e.g., "3/7 tasks completed")

#### `/todos view`

View the details of a selected to-do list without resuming work on it.

**Usage:**
```
/todos view
```

**Features:**
- Browse all to-do lists (both complete and incomplete)
- View full task lists with completion status
- See task descriptions and context
- Non-destructive viewing (doesn't change current session state)

#### `/todos delete`

Delete one or more to-do lists.

**Usage:**
```
/todos delete           # Delete a single selected list
/todos delete --all     # Delete all to-do lists
```

**Options:**
- Without `--all`: Presents a selection interface to choose which list to delete
- With `--all`: Deletes all to-do lists after confirmation
- Deletion is permanent and cannot be undone

#### `/todos clear-finished`

Remove all completed to-do lists while preserving incomplete ones.

**Usage:**
```
/todos clear-finished
```

**Behavior:**
- Automatically identifies to-do lists where all tasks are marked complete
- Removes only fully completed lists
- Preserves any lists with remaining tasks
- Provides feedback on how many lists were cleared

### `/knowledge` - Knowledge Base Management

The `/knowledge` command provides persistent knowledge base functionality with semantic search capabilities. For complete documentation, see the [Knowledge Management guide](./knowledge-management.md).

#### Quick Reference

| Command | Purpose |
|---------|---------|
| `/knowledge show` | Display all knowledge base entries |
| `/knowledge add <name> <path>` | Add files/directories to knowledge base |
| `/knowledge remove <identifier>` | Remove entries by name, path, or ID |
| `/knowledge update <path>` | Update existing entry with new content |
| `/knowledge clear` | Remove all entries (with confirmation) |
| `/knowledge status` | View indexing operation status |
| `/knowledge cancel [id]` | Cancel background operations |

**Example Usage:**
```
/knowledge add "project-docs" ./docs --include "**/*.md"
/knowledge show
/knowledge remove "old-project"
```

### Other Slash Commands

Additional slash commands are available for various system functions:

- `/save` - Save current conversation
- `/load` - Load a saved conversation  
- `/subscribe` - Manage subscription settings

## Command Completion

The CLI provides tab completion for slash commands, including:

- `/todos` - Base command
- `/todos resume` - Resume incomplete to-do lists
- `/todos clear-finished` - Clear completed to-do lists  
- `/todos view` - View to-do list details
- `/todos delete` - Delete to-do lists

## To-Do List Display Format

When viewing to-do lists, they are displayed with clear visual indicators:

- **Incomplete tasks**: `☐ Task description`
- **Completed tasks**: `■ Task description` (green, italicized)
- **Progress indicators**: `(3/7)` showing completed vs total tasks
- **Status symbols**: 
  - `✗` (red) for incomplete lists
  - `✓` (green) for completed lists

## Integration with Chat Sessions

### Automatic Creation
When you give Amazon Q a complex, multi-step task, it will automatically:
1. Create a to-do list using the `todo_list` tool
2. Display the list to you
3. Begin working through tasks sequentially
4. Mark tasks as complete as it finishes them

### Session Persistence
To-do lists persist across chat sessions:
- Lists are saved locally in `.amazonq/cli-todo-lists/`
- You can resume work on incomplete lists in new sessions
- Context and progress are preserved between sessions
- Conversation summaries automatically include active to-do list IDs for continuity

### Resume Workflow
When resuming a to-do list:
1. Use `/todos resume` to select a list
2. Q automatically loads the list context
3. Q reviews completed tasks and remaining work
4. Q continues from where it left off

## Best Practices

### For Users
- **Use descriptive task requests**: Clear, detailed requests help Q create better to-do lists
- **Let Q manage the lists**: Avoid manually editing to-do list files
- **Regular cleanup**: Use `/todos clear-finished` to remove completed lists
- **Resume incomplete work**: Check for incomplete lists when starting new sessions

### For Complex Projects
- **Break down large tasks**: Give Q specific, focused objectives for better to-do list creation
- **Provide context**: Include relevant background information when requesting complex tasks
- **Review progress**: Use `/todos view` to check progress without disrupting current work
- **Organize by project**: Consider using separate chat sessions for different projects

## Storage and File Management

### Local Storage
- **Location**: `.amazonq/cli-todo-lists/` in your current working directory
- **Format**: JSON files with timestamp-based IDs
- **Automatic creation**: Directory is created automatically when needed

### File Structure
Each to-do list contains:
- Task descriptions and completion status
- Context updates from completed tasks
- List of modified files
- Unique identifier and creation metadata

### Cleanup
- Use slash commands rather than manually deleting files
- The system handles file management automatically
- Backup important project directories if needed

## Troubleshooting

### Common Issues

**"No to-do lists to resume"**
- This means no incomplete to-do lists exist
- Create new tasks by giving Q complex, multi-step requests
- Check if lists were accidentally deleted

**Lists not appearing**
- Ensure you're in the correct working directory
- To-do lists are stored relative to where they were created
- Check `.amazonq/cli-todo-lists/` exists and contains files

**Cannot resume a list**
- The to-do list file may be corrupted
- Try `/todos view` to see if the list displays correctly
- Consider deleting corrupted lists and recreating tasks

**Performance with many lists**
- Use `/todos clear-finished` regularly to remove completed lists
- Consider organizing work into separate project directories
- Delete old, irrelevant lists to improve performance

### Getting Help

If you encounter issues with to-do list functionality:
1. Check that the `.amazonq/cli-todo-lists/` directory exists and is writable
2. Verify you're in the correct working directory
3. Try creating a simple test to-do list to verify functionality
4. Use `/todos view` to inspect existing lists for corruption
