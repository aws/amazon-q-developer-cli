# Slash Commands Reference

Amazon Q CLI provides several slash commands that allow you to perform specific actions and manage various features directly within your chat session. Slash commands start with `/` and provide quick access to functionality without needing to exit the chat interface.

## Available Commands

### General Commands

#### `/help`
Display help information about available commands and features.

#### `/quit` or `/exit`
Exit the current chat session and return to the command line.

#### `/clear`
Clear the current conversation history while maintaining the same agent and configuration.

### Agent Management

#### `/agent list`
List all available agents in your current workspace and global directories.

#### `/agent create`
Create a new agent configuration. Opens an interactive wizard to set up agent properties.

#### `/agent switch <name>`
Switch to a different agent for the current session.

### Model Management

#### `/model list`
List all available language models that can be used with Amazon Q.

#### `/model switch <model-id>`
Switch to a different language model for the current session.

### Conversation Management

#### `/save <name>`
Save the current conversation with a given name for later retrieval.

#### `/load <name>`
Load a previously saved conversation by name.

### Subscription Management

#### `/subscribe`
Display your current Amazon Q subscription status and usage information.

### To-Do List Management

The `/todos` command provides comprehensive to-do list management functionality:

#### `/todos view`
View an existing to-do list. Opens an interactive selection menu to choose from available lists.

**Usage:**
```bash
/todos view
```

**Features:**
- Lists all available to-do lists with completion status
- Shows progress indicators (e.g., "3/5 tasks completed")
- Displays completed lists with ✓ and in-progress lists with ✗
- Interactive fuzzy search for easy selection

#### `/todos resume`
Resume working on a selected to-do list. Amazon Q will load the list and continue from where it left off.

**Usage:**
```bash
/todos resume
```

**Features:**
- Automatically loads the selected to-do list state
- Restores previous context and file modifications
- Continues execution from the last completed task
- Provides seamless continuation of interrupted work

#### `/todos delete`
Delete a specific to-do list or all lists.

**Usage:**
```bash
/todos delete          # Delete a selected list (interactive)
/todos delete --all    # Delete all lists (requires confirmation)
```

**Options:**
- `--all`: Delete all to-do lists without individual selection

#### `/todos clear-finished`
Remove all completed to-do lists to clean up your workspace.

**Usage:**
```bash
/todos clear-finished
```

**Features:**
- Only removes lists where all tasks are marked complete
- Preserves in-progress lists
- Provides confirmation of cleanup actions

### Knowledge Management

The `/knowledge` command provides persistent knowledge base functionality:

#### `/knowledge show`
Display all entries in your knowledge base with detailed information.

#### `/knowledge add <name> <path> [options]`
Add files or directories to your knowledge base.

**Usage:**
```bash
/knowledge add "project-docs" /path/to/documentation
/knowledge add "rust-code" /path/to/project --include "*.rs" --exclude "target/**"
```

**Options:**
- `--include <pattern>`: Include files matching the pattern
- `--exclude <pattern>`: Exclude files matching the pattern  
- `--index-type <Fast|Best>`: Choose indexing approach

#### `/knowledge remove <identifier>`
Remove entries from your knowledge base by name, path, or ID.

#### `/knowledge update <path>`
Update an existing knowledge base entry with new content.

#### `/knowledge clear`
Remove all entries from your knowledge base (requires confirmation).

#### `/knowledge status`
View the status of background indexing operations.

#### `/knowledge cancel [operation_id]`
Cancel background operations by ID, or all operations if no ID provided.

## Command Categories

### Interactive Commands
Commands that open selection menus or wizards:
- `/todos view`
- `/todos resume` 
- `/todos delete` (without --all)
- `/agent list`
- `/model list`

### Immediate Action Commands
Commands that perform actions directly:
- `/todos clear-finished`
- `/todos delete --all`
- `/knowledge show`
- `/clear`
- `/quit`

### Commands with Arguments
Commands that require additional parameters:
- `/knowledge add <name> <path>`
- `/save <name>`
- `/load <name>`
- `/agent switch <name>`

## Tips and Best Practices

### General Usage
- Use tab completion to discover available commands
- Most commands provide help when used incorrectly
- Commands are case-insensitive
- Use quotes around names or paths with spaces

### To-Do List Management
- Let Amazon Q create to-do lists automatically for complex tasks
- Use `/todos resume` to continue interrupted work sessions
- Regularly use `/todos clear-finished` to maintain a clean workspace
- View lists with `/todos view` to check progress without resuming

### Knowledge Management
- Use descriptive names when adding knowledge bases
- Leverage include/exclude patterns to focus on relevant files
- Monitor indexing progress with `/knowledge status`
- Use `/knowledge clear` sparingly as it removes all data

### Workflow Integration
- Save important conversations before switching agents or models
- Use `/subscribe` to monitor your usage and subscription status
- Combine slash commands with natural language requests for efficient workflows

## Error Handling

### Common Error Messages

**"No to-do lists found"**
- No to-do lists exist in the current directory
- Create complex tasks to generate new lists automatically

**"Agent not found"**
- The specified agent doesn't exist in current workspace or global directories
- Use `/agent list` to see available agents

**"Knowledge base operation failed"**
- Check file permissions and disk space
- Verify paths exist and are accessible
- Use `/knowledge status` to check for ongoing operations

### Recovery Actions
- Most commands can be safely retried after fixing underlying issues
- Use `/clear` to reset conversation state if commands behave unexpectedly
- Check the Amazon Q CLI logs for detailed error information

## Advanced Usage

### Combining Commands
You can use multiple slash commands in sequence:
```bash
/knowledge add "current-project" .
/todos resume
/save "project-work-session"
```

### Automation Integration
Slash commands work well with:
- Shell scripts that launch Q CLI with specific agents
- Workflow automation that saves/loads conversations
- CI/CD pipelines that use knowledge bases for context

### Customization
- Configure default knowledge base patterns with `q settings`
- Set default agents to avoid repeated `/agent switch` commands
- Use agent configurations to pre-configure tool permissions for smoother workflows
