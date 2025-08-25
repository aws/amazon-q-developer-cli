# To-Do List Quick Reference

Amazon Q CLI includes powerful to-do list functionality that automatically breaks down complex tasks and tracks progress. This quick reference covers the most common use cases.

## Quick Start

### Automatic To-Do Creation
Simply give Q a complex task and it will automatically create a to-do list:

```
User: "Set up a new Python web API with FastAPI, database integration, and tests"
Q: [Automatically creates and displays a to-do list with steps like:]
   ☐ Initialize Python project structure
   ☐ Install FastAPI and dependencies  
   ☐ Set up database models
   ☐ Create API endpoints
   ☐ Write unit tests
   ☐ Configure testing framework
```

### Managing Existing Lists

| Command | Purpose | Example |
|---------|---------|---------|
| `/todos resume` | Continue working on an incomplete list | Select from fuzzy-searchable list |
| `/todos view` | View list details without resuming | Browse all lists, see progress |
| `/todos delete` | Remove a specific list | Choose from selection interface |
| `/todos delete --all` | Remove all lists | Clears everything after confirmation |
| `/todos clear-finished` | Remove only completed lists | Keeps incomplete lists intact |

## Visual Indicators

| Symbol | Meaning | Example |
|--------|---------|---------|
| `☐` | Incomplete task | `☐ Set up database connection` |
| `■` | Completed task | `■ Install dependencies` (green, italic) |
| `✗` | Incomplete list | `✗ API Setup (2/5)` (red) |
| `✓` | Completed list | `✓ Database Migration (3/3)` (green) |
| `(2/5)` | Progress indicator | 2 completed out of 5 total tasks |

## Common Workflows

### Starting a New Project
1. Describe your project goals to Q
2. Q automatically creates a to-do list
3. Q begins working through tasks sequentially
4. Tasks are marked complete as Q finishes them

### Resuming Previous Work
1. Start a new chat session
2. Run `/todos resume`
3. Select the incomplete list you want to continue
4. Q loads the context and continues where it left off

### Project Cleanup
1. Run `/todos view` to see all lists
2. Use `/todos clear-finished` to remove completed work
3. Use `/todos delete` to remove outdated or irrelevant lists

## Best Practices

### For Better To-Do Lists
- **Be specific**: "Set up authentication with JWT tokens" vs "add auth"
- **Provide context**: Include relevant technologies, constraints, or requirements
- **Break down large requests**: Focus on specific features or components

### For Project Management
- **Regular cleanup**: Use `/todos clear-finished` to maintain organization
- **Separate concerns**: Use different chat sessions for different projects
- **Review progress**: Check `/todos view` before starting new work

### For Team Collaboration
- **Document context**: Include relevant background in your requests
- **Use descriptive names**: Q creates meaningful descriptions for lists
- **Share approaches**: Team members can learn from viewing completed lists

## Storage and Persistence

- **Location**: `.amazonq/cli-todo-lists/` in your working directory
- **Format**: JSON files with timestamp-based IDs
- **Persistence**: Lists survive across chat sessions and CLI restarts
- **Context**: Each task completion includes context and modified files

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "No to-do lists found" | Create new tasks by giving Q complex requests |
| Lists not appearing | Check you're in the correct working directory |
| Cannot resume list | Try `/todos view` to check if list is corrupted |
| Too many old lists | Use `/todos clear-finished` or `/todos delete --all` |

## Integration Points

### With Chat Sessions
- Lists are automatically created for multi-step tasks
- Progress is tracked in conversation summaries
- Context is preserved between sessions

### With File Operations
- Modified files are tracked with each completed task
- File changes are included in task context
- Project state is maintained across work sessions

### With Agent Configuration
- The `todo_list` tool is trusted by default
- No special configuration required
- Works with all agent types and configurations

## Advanced Usage

### Custom Task Management
While Q manages to-do lists automatically, you can:
- Request specific task breakdowns: "Create a to-do list for database migration"
- Ask for task modifications: "Add error handling tasks to the current list"
- Request progress reviews: "Show me what we've completed so far"

### Project Organization
- Use separate directories for different projects
- Each directory maintains its own `.amazonq/cli-todo-lists/`
- Lists are scoped to the working directory where they were created

### Context Preservation
- Task context includes important decisions and discoveries
- Modified file lists help track project changes
- Context is available when resuming work in new sessions
