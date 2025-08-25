# Built-in Tools

Amazon Q CLI includes several built-in tools that agents can use. This document describes each tool and its configuration options.

- [`execute_bash`](#execute_bash-tool) — Execute a shell command.
- [`fs_read`](#fs_read-tool) — Read files, directories, and images.
- [`fs_write`](#fs_write-tool) — Create and edit files.
- [`report_issue`](#report_issue-tool) — Open a GitHub issue template.
- [`knowledge`](#knowledge-tool) — Store and retrieve information in a knowledge base.
- [`thinking`](#thinking-tool) — Internal reasoning mechanism.
- [`use_aws`](#use_aws-tool) — Make AWS CLI API calls.
- [`todo_list`](#todo_list-tool) — Create and manage to-do lists.

## Execute_bash Tool

Execute the specified bash command.

### Configuration

```json
{
  "toolsSettings": {
    "execute_bash": {
      "allowedCommands": ["git status", "git fetch"],
      "deniedCommands": ["git commit .*", "git push .*"],
      "allowReadOnly": true
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description                                                                              |
|--------|------|---------|------------------------------------------------------------------------------------------|
| `allowedCommands` | array of strings | `[]` | List of specific commands that are allowed without prompting. Supports regex formatting. Note that regex entered are anchored with \A and \z |
| `deniedCommands` | array of strings | `[]` | List of specific commands that are denied. Supports regex formatting. Note that regex entered are anchored with \A and \z. Deny rules are evaluated before allow rules |
| `allowReadOnly` | boolean | `true` | Whether to allow read-only commands without prompting                                    |

## Fs_read Tool

Tool for reading files, directories, and images.

### Configuration

```json
{
  "toolsSettings": {
    "fs_read": {
      "allowedPaths": ["~/projects", "./src/**"],
      "deniedPaths": ["/some/denied/path/", "/another/denied/path/**/file.txt"]
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedPaths` | array of strings | `[]` | List of paths that can be read without prompting. Supports glob patterns. Glob patterns have the same behavior as gitignore. For example, `~/temp` would match `~/temp/child` and `~/temp/child/grandchild` |
| `deniedPaths` | array of strings | `[]` | List of paths that are denied. Supports glob patterns. Deny rules are evaluated before allow rules. Glob patterns have the same behavior as gitignore. For example, `~/temp` would match `~/temp/child` and `~/temp/child/grandchild`  |

## Fs_write Tool

Tool for creating and editing files.

### Configuration

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/projects/output.txt", "./src/**"],
      "deniedPaths": ["/some/denied/path/", "/another/denied/path/**/file.txt"]
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedPaths` | array of strings | `[]` | List of paths that can be written to without prompting. Supports glob patterns. Glob patterns have the same behavior as gitignore.For example, `~/temp` would match `~/temp/child` and `~/temp/child/grandchild` |
| `deniedPaths` | array of strings | `[]` | List of paths that are denied. Supports glob patterns. Deny rules are evaluated before allow rules. Glob patterns have the same behavior as gitignore.For example, `~/temp` would match `~/temp/child` and `~/temp/child/grandchild` |

## Report_issue Tool

Opens the browser to a pre-filled GitHub issue template to report chat issues, bugs, or feature requests.

This tool has no configuration options.

## Knowledge Tool

Store and retrieve information in a knowledge base across chat sessions. Provides semantic search capabilities for files, directories, and text content.

This tool has no configuration options.

## Thinking Tool

An internal reasoning mechanism that improves the quality of complex tasks by breaking them down into atomic actions.

This tool has no configuration options.

## Use_aws Tool

Make AWS CLI API calls with the specified service, operation, and parameters.

### Configuration

```json
{
  "toolsSettings": {
    "use_aws": {
      "allowedServices": ["s3", "lambda", "ec2"],
      "deniedServices": ["eks", "rds"]
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedServices` | array of strings | `[]` | List of AWS services that can be accessed without prompting |
| `deniedServices` | array of strings | `[]` | List of AWS services to deny. Deny rules are evaluated before allow rules |

## Todo_list Tool

Create and manage to-do lists that persist across chat sessions. This tool allows Amazon Q to break down complex tasks into manageable steps and track progress as tasks are completed.

### Key Features

- **Automatic Task Creation**: Q automatically creates to-do lists when given multi-step tasks
- **Progress Tracking**: Tasks are marked as completed as Q works through them
- **Persistent Storage**: To-do lists are saved locally and persist across sessions
- **Context Tracking**: Important information and modified files are tracked with each task
- **Resume Functionality**: Users can resume incomplete to-do lists from previous sessions using `/todos resume`

### Commands

#### `create`
Creates a new to-do list with specified tasks and description.

**Parameters:**
- `tasks` (required): Array of distinct task descriptions
- `todo_list_description` (required): Brief summary of the to-do list

#### `complete`
Marks tasks as completed and updates context information.

**Parameters:**
- `completed_indices` (required): Array of 0-indexed task numbers to mark as complete
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

### Storage Location

To-do lists are stored locally in the current working directory under:
```
.amazonq/cli-todo-lists/
```

Each to-do list is saved as a JSON file with a timestamp-based ID. The storage directory is automatically created when the chat session starts if it doesn't exist.

### Usage Patterns

**Automatic Creation**: When you give Q a complex task, it will automatically create a to-do list before starting work:
```
User: "Set up a new React project with TypeScript and testing"
Q: [Creates to-do list with steps like "Initialize project", "Configure TypeScript", "Set up testing framework", etc.]
```

**Progress Tracking**: Q marks tasks as completed immediately after finishing them, providing visual feedback on progress.

**Context Preservation**: Each completed task includes context about what was accomplished and which files were modified, helping maintain continuity across sessions. When resuming a to-do list, Q automatically loads the list using the `load` command and continues from where it left off.

This tool has no configuration options and is trusted by default. The `todo_list` tool appears as "trusted" in agent tool listings.

## Using Tool Settings in Agent Configuration

Tool settings are specified in the `toolsSettings` section of the agent configuration file. Each tool's settings are specified using the tool's name as the key.

For MCP server tools, use the format `@server_name/tool_name` as the key:

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/projects"]
    },
    "@git/git_status": {
      "git_user": "$GIT_USER"
    }
  }
}
```

## Tool Permissions

Tools can be explicitly allowed in the `allowedTools` section of the agent configuration:

```json
{
  "allowedTools": [
    "fs_read",
    "knowledge",
    "@git/git_status"
  ]
}
```

If a tool is not in the `allowedTools` list, the user will be prompted for permission when the tool is used unless an allowed `toolSettings` configuration is set.

Some tools have default permission behaviors:
- `fs_read`, `report_issue`, and `todo_list` are trusted by default
- `execute_bash`, `fs_write`, and `use_aws` prompt for permission by default, but can be configured to allow specific commands/paths/services
