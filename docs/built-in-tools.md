# Built-in Tools

Amazon Q CLI includes several built-in tools that agents can use. This document describes each tool and its configuration options.

- [`execute_bash`](#execute_bash-tool) — Execute a shell command.
- [`fs_read`](#fs_read-tool) — Read files, directories, and images.
- [`fs_write`](#fs_write-tool) — Create and edit files.
- [`introspect`](#introspect-tool) — Provide information about Q CLI capabilities and documentation.
- [`report_issue`](#report_issue-tool) — Open a GitHub issue template.
- [`knowledge`](#knowledge-tool) — Store and retrieve information in a knowledge base.
- [`commands`](#commands-tool) — Manage custom commands within the chat session.
- [`thinking`](#thinking-tool) — Internal reasoning mechanism.
- [`todo_list`](#todo_list-tool) — Create and manage TODO lists for tracking multi-step tasks.
- [`use_aws`](#use_aws-tool) — Make AWS CLI API calls.

## Execute_bash Tool

Execute the specified bash command on Unix-like systems (Linux, macOS).

> **Note**: On Windows systems, the `execute_cmd` tool is used instead for executing command prompt commands.

### Configuration

```json
{
  "toolsSettings": {
    "execute_bash": {
      "allowedCommands": ["git status", "git fetch"],
      "deniedCommands": ["git commit .*", "git push .*"],
      "autoAllowReadonly": true
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description                                                                              |
|--------|------|---------|------------------------------------------------------------------------------------------|
| `allowedCommands` | array of strings | `[]` | List of specific commands that are allowed without prompting. Supports regex formatting. Note that regex entered are anchored with \A and \z |
| `deniedCommands` | array of strings | `[]` | List of specific commands that are denied. Supports regex formatting. Note that regex entered are anchored with \A and \z. Deny rules are evaluated before allow rules |
| `autoAllowReadonly` | boolean | `false` | Whether to allow read-only commands without prompting                                    |

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

## Introspect Tool

Provide information about Q CLI capabilities, features, commands, and documentation. This tool accesses Q CLI's built-in documentation and help content to answer questions about the CLI's functionality.

### Usage

The introspect tool is automatically used when you ask questions about Q CLI itself, such as:
- "What can you do?"
- "How do I save conversations?"
- "What commands are available?"
- "Do you have feature X?"

### Behavior

- Tries to provide the information that is explicitly documented
- Accesses README, built-in tools documentation, experiments, and settings information
- Automatically enters tangent mode when configured to do so and if we set the setting introspect.tangentMode = true.

## Report_issue Tool

Opens the browser to a pre-filled GitHub issue template to report chat issues, bugs, or feature requests.

This tool has no configuration options.

## Knowledge Tool (experimental)

Store and retrieve information in a knowledge base across chat sessions. Provides semantic search capabilities for files, directories, and text content.

This tool has no configuration options.

## Commands Tool (experimental)

Manage custom commands within the chat session. This tool provides the underlying functionality for the `/commands` command system, allowing creation, modification, and execution of custom command templates.

### Configuration

The Commands tool itself has no direct configuration options, but its availability is controlled by a global setting:

```bash
q settings chat.enableCommands true
```

### Usage

The Commands tool is used internally by the `/commands` command system:

- `/commands add <name>` - Create new custom commands
- `/commands show` - List available commands  
- `/commands remove <name>` - Delete commands
- `/commands update <name>` - Edit existing commands
- `/commands clear` - Remove multiple commands

Custom commands can then be executed using:
- `/project:<name>` - Execute project-scoped commands
- `/user:<name>` - Execute user-scoped commands

See the [Custom Commands documentation](./custom-commands.md) for detailed usage information.

## Thinking Tool (experimental)

An internal reasoning mechanism that improves the quality of complex tasks by breaking them down into atomic actions.

This tool has no configuration options.

## TODO List Tool (experimental)

Create and manage TODO lists for tracking multi-step tasks. Lists are stored locally in `.amazonq/cli-todo-lists/`.

This tool has no configuration options.

## Use_aws Tool

Make AWS CLI API calls with the specified service, operation, and parameters.

### Configuration

```json
{
  "toolsSettings": {
    "use_aws": {
      "allowedServices": ["s3", "lambda", "ec2"],
      "deniedServices": ["eks", "rds"],
      "autoAllowReadonly": true
    }
  }
}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedServices` | array of strings | `[]` | List of AWS services that can be accessed without prompting |
| `deniedServices` | array of strings | `[]` | List of AWS services to deny. Deny rules are evaluated before allow rules |
| `autoAllowReadonly` | boolean | `false` | Whether to automatically allow read-only operations (get, describe, list, ls, search, batch_get) without prompting |

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

## Default Tool Permissions

Built-in tools have different default permission levels:

### Always Trusted (No Prompt Required)
- **`fs_read`** - Reading files and directories is considered safe
- **`report_issue`** - Opening GitHub issue templates is considered safe  
- **`thinking`** - Internal reasoning mechanism (prerelease feature)

### Configurable Trust
These tools prompt for permission by default but can be configured to allow specific operations without prompting:

- **`execute_bash`/`execute_cmd`** - Can be configured to allow specific commands or read-only operations
- **`fs_write`** - Can be configured to allow writing to specific paths
- **`use_aws`** - Can be configured to allow specific AWS services

### Session-Based Trust
- **`knowledge`** - Prompts for permission but can be trusted for the session
- **`commands`** - Prompts for permission but can be trusted for the session

Use the `/tools trust <tool_name>` command to trust tools for the current session, or configure them in your agent file for persistent trust.
