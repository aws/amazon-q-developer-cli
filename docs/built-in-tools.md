# Built-in Tools

Amazon Q CLI includes several built-in tools that agents can use. This document describes each tool and its configuration options.

- [`execute_bash`](#execute_bash-tool) — Execute a shell command.
- [`fs_read`](#fs_read-tool) — Read files, directories, and images.
- [`fs_write`](#fs_write-tool) — Create and edit files.
- [`report_issue`](#report_issue-tool) — Open a GitHub issue template.
- [`knowledge`](#knowledge-tool) — Store and retrieve information in a knowledge base.
- [`thinking`](#thinking-tool) — Internal reasoning mechanism.
- [`todo_list`](#todo_list-tool) — Create and manage to-do lists for multi-step tasks.
- [`use_aws`](#use_aws-tool) — Make AWS CLI API calls.

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

## Todo_list Tool

Create and manage to-do lists for multi-step tasks. This tool helps track progress on complex tasks by breaking them down into manageable steps and marking completion as work progresses.

The tool automatically creates to-do lists when you give Amazon Q multi-step tasks and tracks completion status. To-do lists are stored locally in the `.amazonq/cli-todo-lists/` directory and persist across chat sessions.

### Commands

#### `create`
Creates a new to-do list with specified tasks and description.

**Required parameters:**
- `tasks`: Array of distinct task descriptions
- `todo_list_description`: Brief summary of the to-do list

#### `complete`
Marks specified tasks as completed and updates context.

**Required parameters:**
- `completed_indices`: Array of 0-indexed task numbers to mark complete
- `context_update`: Important information about completed tasks
- `current_id`: ID of the currently loaded to-do list

**Optional parameters:**
- `modified_files`: Array of file paths that were modified during the task

#### `load`
Loads an existing to-do list by ID.

**Required parameters:**
- `load_id`: ID of the to-do list to load

#### `add`
Adds new tasks to the current to-do list.

**Required parameters:**
- `new_tasks`: Array of new task descriptions
- `insert_indices`: Array of 0-indexed positions where tasks should be inserted
- `current_id`: ID of the currently loaded to-do list

**Optional parameters:**
- `new_description`: Updated description if tasks significantly change the goal

#### `remove`
Removes tasks from the current to-do list.

**Required parameters:**
- `remove_indices`: Array of 0-indexed positions of tasks to remove
- `current_id`: ID of the currently loaded to-do list

**Optional parameters:**
- `new_description`: Updated description if removal significantly changes the goal

### Configuration

This tool has no configuration options and is trusted by default.

### Usage Notes

- To-do lists are automatically created when you give Amazon Q complex, multi-step tasks
- Tasks should be marked as completed immediately after finishing each step
- The tool tracks file modifications and important context for each completed task
- To-do lists persist across chat sessions and can be resumed later
- Use the `/todos` slash command to view, manage, and resume existing to-do lists

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
- `fs_read` and `report_issue` are trusted by default
- `execute_bash`, `fs_write`, and `use_aws` prompt for permission by default, but can be configured to allow specific commands/paths/services
