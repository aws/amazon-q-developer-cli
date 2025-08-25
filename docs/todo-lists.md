# To-Do List Management

Amazon Q CLI includes comprehensive to-do list functionality that helps you track progress on complex, multi-step tasks. The system automatically creates to-do lists when you give Amazon Q tasks that require multiple steps, and provides tools to manage and resume these lists across chat sessions.

## Overview

The to-do list system consists of two main components:

1. **`todo_list` tool**: Used by Amazon Q to automatically create and manage to-do lists during task execution
2. **`/todos` slash command**: Allows you to manually view, manage, and resume existing to-do lists

## How It Works

### Automatic To-Do List Creation

When you give Amazon Q a complex task that requires multiple steps, it will automatically:

1. Create a to-do list with all necessary steps
2. Display the list to show what work will be done
3. Mark off tasks as they are completed
4. Track important context and file modifications
5. Save progress that persists across chat sessions

### Example Workflow

```
User: "Help me set up a new React project with TypeScript, ESLint, and deploy it to AWS"

Q creates a to-do list:
TODO:
 ☐ Initialize new React project with TypeScript
 ☐ Configure ESLint with TypeScript rules
 ☐ Set up build configuration
 ☐ Create AWS deployment configuration
 ☐ Deploy to AWS and verify

Q then works through each task, marking them complete:
 ■ Initialize new React project with TypeScript
 ☐ Configure ESLint with TypeScript rules
 ...
```

## Managing To-Do Lists

### The `/todos` Slash Command

Use `/todos` followed by a subcommand to manage your to-do lists:

#### `/todos view`
View an existing to-do list. Opens a selection menu to choose from available lists.

```bash
/todos view
```

#### `/todos resume`
Resume working on a selected to-do list. Amazon Q will load the list and continue from where it left off.

```bash
/todos resume
```

#### `/todos delete`
Delete a specific to-do list or all lists.

```bash
/todos delete          # Delete a selected list
/todos delete --all    # Delete all lists
```

#### `/todos clear-finished`
Remove all completed to-do lists to clean up your workspace.

```bash
/todos clear-finished
```

## To-Do List Storage

### Local Storage
To-do lists are stored locally in your current working directory under:
```
.amazonq/cli-todo-lists/
```

Each to-do list is saved as a JSON file with a unique timestamp-based ID.

### Persistence
- To-do lists persist across chat sessions
- You can resume work on any incomplete list
- Lists are automatically saved when tasks are completed or modified
- Context and file modifications are tracked for each completed task

## To-Do List States

### Task Status
- **☐** Incomplete task (displayed in normal text)
- **■** Completed task (displayed in green italic text)

### List Status
- **In Progress**: Has incomplete tasks (displayed with ✗ and progress count)
- **Completed**: All tasks finished (displayed with ✓)

### Display Format
```
✗ Set up React project (2/5)  # In progress
✓ Deploy website             # Completed
```

## Best Practices

### For Users
1. **Let Amazon Q create lists**: Don't manually create to-do lists - let Amazon Q generate them based on your requests
2. **Use descriptive requests**: Provide clear, detailed descriptions of what you want to accomplish
3. **Resume incomplete work**: Use `/todos resume` to continue work on unfinished tasks
4. **Clean up regularly**: Use `/todos clear-finished` to remove completed lists

### For Complex Tasks
1. **Break down large requests**: If you have a very complex project, consider breaking it into smaller, focused requests
2. **Provide context**: Give Amazon Q relevant information about your project, preferences, and constraints
3. **Review progress**: Use `/todos view` to check the status of ongoing work

## Integration with Chat Sessions

### Conversation Summaries
When you save a conversation that includes to-do list work, the summary will include:
- The ID of any currently loaded to-do list
- Progress made on tasks
- Important context and insights

### Resuming Work
When you resume a conversation or load a to-do list:
- Amazon Q automatically loads the list state
- Previous context and file modifications are available
- Work continues from the last completed task

## Troubleshooting

### Common Issues

**To-do lists not appearing**
- Ensure you're in the same directory where the lists were created
- Check that `.amazonq/cli-todo-lists/` exists in your current directory

**Cannot resume a list**
- Verify the list still exists with `/todos view`
- Check that the list file hasn't been corrupted or manually modified

**Lists not saving progress**
- Ensure you have write permissions in the current directory
- Check that there's sufficient disk space

### Error Messages

**"No to-do lists to [action]"**
- No lists exist in the current directory
- Create a new task that requires multiple steps to generate a list

**"Could not load to-do list"**
- The list file may be corrupted or manually modified
- Try deleting the problematic list and creating a new one

## Technical Details

### File Format
To-do lists are stored as JSON files containing:
- Task descriptions and completion status
- List description and metadata
- Context updates from completed tasks
- Modified file paths
- Unique identifier

### Security
- Lists are stored locally and never transmitted
- No sensitive information is automatically captured
- File paths and context are only what you explicitly work with

### Performance
- Lists are loaded on-demand
- Minimal impact on chat session performance
- Automatic cleanup of temporary files
