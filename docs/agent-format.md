# Agent Format

The agent configuration file for each agent is a JSON file. The filename (without the `.json` extension) becomes the agent's name. It contains configuration needed to instantiate and run the agent.

> [!TIP]
> We recommend using the `/agent generate` slash command within your active Q session to intelligently generate your agent configuration with the help of Q.

Every agent configuration file can include the following sections:

- [`name`](#name-field) — The name of the agent (optional, derived from filename if not specified).
- [`description`](#description-field) — A description of the agent.
- [`prompt`](#prompt-field) — High-level context for the agent.
- [`mcpServers`](#mcpservers-field) — The MCP servers the agent has access to.
- [`tools`](#tools-field) — The tools available to the agent.
- [`toolAliases`](#toolaliases-field) — Tool name remapping for handling naming collisions.
- [`allowedTools`](#allowedtools-field) — Tools that can be used without prompting.
- [`toolsSettings`](#toolssettings-field) — Configuration for specific tools.
- [`resources`](#resources-field) — Resources available to the agent.
- [`hooks`](#hooks-field) — Commands run at specific trigger points.
- [`useLegacyMcpJson`](#uselegacymcpjson-field) — Whether to include legacy MCP configuration.
- [`model`](#model-field) — The model ID to use for this agent.

## Name Field

The `name` field specifies the name of the agent. This is used for identification and display purposes. 

```json
{
  "name": "aws-expert"
}
```

## Description Field

The `description` field provides a description of what the agent does. This is primarily for human readability and helps users distinguish between different agents.

```json
{
  "description": "An agent specialized for AWS infrastructure tasks"
}
```

## Prompt Field

The `prompt` field is intended to provide high-level context to the agent, similar to a system prompt. It supports both inline text and file:// URIs to reference external files.

### Inline Prompt

```json
{
  "prompt": "You are an expert AWS infrastructure specialist"
}
```

### File URI Prompt

You can reference external files using `file://` URIs. This allows you to maintain long, complex prompts in separate files for better organization and version control, while keeping your agent configuration clean and readable.

```json
{
  "prompt": "file://./my-agent-prompt.md"
}
```

#### File URI Path Resolution

- **Relative paths**: Resolved relative to the agent configuration file's directory
  - `"file://./prompt.md"` → `prompt.md` in the same directory as the agent config
  - `"file://../shared/prompt.md"` → `prompt.md` in a parent directory
- **Absolute paths**: Used as-is
  - `"file:///home/user/prompts/agent.md"` → Absolute path to the file

#### File URI Examples

```json
{
  "prompt": "file://./prompts/aws-expert.md"
}
```

```json
{
  "prompt": "file:///Users/developer/shared-prompts/rust-specialist.md"
}
```

## McpServers Field

The `mcpServers` field specifies which Model Context Protocol (MCP) servers the agent has access to. Each server is defined with a command and optional arguments.

```json
{
  "mcpServers": {
    "fetch": {
      "command": "fetch3.1",
      "args": []
    },
    "git": {
      "command": "git-mcp",
      "args": [],
      "env": {
        "GIT_CONFIG_GLOBAL": "/dev/null"
      },
      "timeout": 120000
    }
  }
}
```

Each MCP server configuration can include:
- `command` (required): The command to execute to start the MCP server
- `args` (optional): Arguments to pass to the command
- `env` (optional): Environment variables to set for the server
- `timeout` (optional): Timeout for each MCP request in milliseconds (default: 120000)

## Tools Field

The `tools` field lists all tools that the agent can potentially use. Tools include built-in tools and tools from MCP servers.

- Built-in tools are specified by their name (e.g., `fs_read`, `execute_bash`)
- MCP server tools are prefixed with `@` followed by the server name (e.g., `@git`)
- To specify a specific tool from an MCP server, use `@server_name/tool_name`
- Use `*` as a special wildcard to include all available tools (both built-in and from MCP servers)
- Use `@builtin` to include all built-in tools
- Use `@server_name` to include all tools from a specific MCP server

```json
{
  "tools": [
    "fs_read",
    "fs_write",
    "execute_bash",
    "@git",
    "@rust-analyzer/check_code"
  ]
}
```

To include all available tools, you can simply use:

```json
{
  "tools": ["*"]
}
```

## ToolAliases Field

The `toolAliases` field is an advanced feature that allows you to remap tool names. This is primarily used to resolve naming collisions between tools from different MCP servers, or to create more intuitive names for specific tools.

For example, if both `@github-mcp` and `@gitlab-mcp` servers provide a tool called `get_issues`, you would have a naming collision. You can use `toolAliases` to disambiguate them:

```json
{
  "toolAliases": {
    "@github-mcp/get_issues": "github_issues",
    "@gitlab-mcp/get_issues": "gitlab_issues"
  }
}
```

With this configuration, the tools will be available to the agent as `github_issues` and `gitlab_issues` instead of having a collision on `get_issues`.

You can also use aliases to create shorter or more intuitive names for frequently used tools:

```json
{
  "toolAliases": {
    "@aws-cloud-formation/deploy_stack_with_parameters": "deploy_cf",
    "@kubernetes-tools/get_pod_logs_with_namespace": "pod_logs"
  }
}
```

The key is the original tool name (including server prefix for MCP tools), and the value is the new name to use.

## AllowedTools Field

The `allowedTools` field specifies which tools can be used without prompting the user for permission. This is a security feature that helps prevent unauthorized tool usage.

```json
{
  "allowedTools": [
    "fs_read",
    "fs_*",
    "@git/git_status",
    "@server/read_*",
    "@fetch"
  ]
}
```

You can allow tools using several patterns:

### Exact Matches
- **Built-in tools**: `"fs_read"`, `"execute_bash"`, `"knowledge"`
- **Specific MCP tools**: `"@server_name/tool_name"` (e.g., `"@git/git_status"`)
- **All tools from MCP server**: `"@server_name"` (e.g., `"@fetch"`)

### Wildcard Patterns
The `allowedTools` field supports glob-style wildcard patterns using `*` and `?`:

#### Native Tool Patterns
- **Prefix wildcard**: `"fs_*"` → matches `fs_read`, `fs_write`, `fs_anything`
- **Suffix wildcard**: `"*_bash"` → matches `execute_bash`, `run_bash`
- **Middle wildcard**: `"fs_*_tool"` → matches `fs_read_tool`, `fs_write_tool`
- **Single character**: `"fs_?ead"` → matches `fs_read`, `fs_head` (but not `fs_write`)

#### MCP Tool Patterns
- **Tool prefix**: `"@server/read_*"` → matches `@server/read_file`, `@server/read_config`
- **Tool suffix**: `"@server/*_get"` → matches `@server/issue_get`, `@server/data_get`
- **Server pattern**: `"@*-mcp/read_*"` → matches `@git-mcp/read_file`, `@db-mcp/read_data`
- **Any tool from pattern servers**: `"@git-*/*"` → matches any tool from servers matching `git-*`

Optionally, you can also prefix native tools with the namespace `@builtin`.

### Examples

```json
{
  "allowedTools": [
    // Exact matches
    "fs_read",
    "knowledge",
    "@server/specific_tool",
    
    // Native tool wildcards
    "fs_*",                    // All filesystem tools
    "execute_*",               // All execute tools
    "*_test",                  // Any tool ending in _test
    @builtin,                // All native tools
    
    // MCP tool wildcards
    "@server/api_*",           // All API tools from server
    "@server/read_*",          // All read tools from server
    "@git-server/get_*_info",  // Tools like get_user_info, get_repo_info
    "@*/status",               // Status tool from any server
    
    // Server-level permissions
    "@fetch",                  // All tools from fetch server
    "@git-*"                   // All tools from any git-* server
  ]
}
```

### Pattern Matching Rules
- **`*`** matches any sequence of characters (including none)
- **`?`** matches exactly one character
- **Exact matches** take precedence over patterns
- **Server-level permissions** (`@server_name`) allow all tools from that server
- **Case-sensitive** matching

Unlike the `tools` field, the `allowedTools` field does not support the `"*"` wildcard for allowing all tools. To allow tools, you must use specific patterns or server-level permissions.

## ToolsSettings Field

The `toolsSettings` field provides configuration for specific tools. Each tool can have its own unique configuration options.
Note that specifications that configure allowable patterns will be overridden if the tool is also included in `allowedTools`.

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/**"]
    },
    "@git/git_status": {
      "git_user": "$GIT_USER"
    }
  }
}
```

For built-in tool configuration options, please refer to the [built-in tools documentation](./built-in-tools.md).

### Path Resolution in toolsSettings

When specifying paths in `toolsSettings` (such as `allowedPaths` and `deniedPaths` for filesystem tools), the path resolution behavior depends on whether the path is absolute or relative:

#### Absolute Paths
Absolute paths are used as-is, regardless of where the agent configuration file is located. This makes them ideal for cross-directory scenarios where you need to grant access to specific locations on the filesystem.

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": [
        "/Users/developer/projects/my-app/**",
        "/tmp/build-output/**"
      ]
    }
  }
}
```

#### Relative Paths
Relative paths in `allowedPaths` and `deniedPaths` are resolved relative to the agent configuration file's directory, not the current working directory (CWD). This makes agent configurations portable and predictable.

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": [
        "./src/**",
        "../shared-config/**"
      ]
    },
    "fs_read": {
      "deniedPaths": [
        "./secrets/**"
      ]
    }
  }
}
```

If the agent configuration is located at `/workspace/tools/.amazonq/cli-agents/my-agent.json`, then:
- `"./src/**"` resolves to `/workspace/tools/src/**`
- `"../shared-config/**"` resolves to `/workspace/shared-config/**`
- `"./secrets/**"` resolves to `/workspace/tools/secrets/**`

#### Cross-Directory Agent Usage

This path resolution behavior is particularly useful when using package-level agents that need to operate on files in different directories. For example, if you have an agent in one package that needs to modify files in another package:

```json
{
  "name": "migration-tool",
  "description": "Migrates code across multiple packages",
  "tools": ["fs_read", "fs_write"],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": [
        "/workspace/package-a/**",
        "/workspace/package-b/**",
        "/workspace/shared/**"
      ]
    },
    "fs_read": {
      "allowedPaths": [
        "/workspace/**"
      ]
    }
  }
}
```

In this example, the agent can be located anywhere (e.g., `/workspace/tools/.amazonq/cli-agents/migration-tool.json`) and will still have access to the specified absolute paths.

#### Backward Compatibility

For backward compatibility, when an agent configuration does not have a file path set (such as when using the default agent or in certain programmatic scenarios), relative paths fall back to being resolved from the current working directory (CWD). However, it's recommended to use absolute paths for cross-directory scenarios to ensure consistent behavior.

#### Best Practices

- Use absolute paths when your agent needs to access files outside its own directory tree
- Use relative paths when your agent only needs to access files within its own directory structure
- For cross-directory scenarios in non-interactive mode, absolute paths ensure permissions are correctly evaluated regardless of where the command is executed from

## Resources Field

The `resources` field gives an agent access to local resources. Currently, only file resources are supported, and all resource paths must start with `file://`.

```json
{
  "resources": [
    "file://AmazonQ.md",
    "file://README.md",
    "file://.amazonq/rules/**/*.md"
  ]
}
```

Resources can include:
- Specific files
- Glob patterns for multiple files
- Absolute or relative paths

## Hooks Field

The `hooks` field defines commands to run at specific trigger points during agent lifecycle and tool execution.

For detailed information about hook behavior, input/output formats, and examples, see the [Hooks documentation](hooks.md).

```json
{
  "hooks": {
    "agentSpawn": [
      {
        "command": "git status"
      }
    ],
    "userPromptSubmit": [
      {
        "command": "ls -la"
      }
    ],
    "preToolUse": [
      {
        "matcher": "execute_bash",
        "command": "{ echo \"$(date) - Bash command:\"; cat; echo; } >> /tmp/bash_audit_log"
      },
      {
        "matcher": "use_aws",
        "command": "{ echo \"$(date) - AWS CLI call:\"; cat; echo; } >> /tmp/aws_audit_log"
      }
    ],
    "postToolUse": [
      {
        "matcher": "fs_write",
        "command": "cargo fmt --all"
      }
    ]
  }
}
```

Each hook is defined with:
- `command` (required): The command to execute
- `matcher` (optional): Pattern to match tool names for `preToolUse` and `postToolUse` hooks. See [built-in tools documentation](./built-in-tools.md) for available tool names.

Available hook triggers:
- `agentSpawn`: Triggered when the agent is initialized.
- `userPromptSubmit`: Triggered when the user submits a message.
- `preToolUse`: Triggered before a tool is executed. Can block the tool use.
- `postToolUse`: Triggered after a tool is executed.
- `stop`: Triggered when the assistant finishes responding.

## UseLegacyMcpJson Field

The `useLegacyMcpJson` field determines whether to include MCP servers defined in the legacy MCP configuration files (`~/.aws/amazonq/mcp.json` for global and `cwd/.amazonq/mcp.json` for workspace).

```json
{
  "useLegacyMcpJson": true
}
```

When set to `true`, the agent will have access to all MCP servers defined in the global and local configurations in addition to those defined in the agent's `mcpServers` field.

## Model Field

The `model` field specifies the model ID to use for this agent. If not specified, the agent will use the default model.

```json
{
  "model": "claude-sonnet-4"
}
```

The model ID must match one of the available models returned by the Q CLI's model service. You can see available models by using the `/model` command in an active chat session.

If the specified model is not available, the agent will fall back to the default model and display a warning.

## Complete Example

Here's a complete example of an agent configuration file:

```json
{
  "name": "aws-rust-agent",
  "description": "A specialized agent for AWS and Rust development tasks",
  "mcpServers": {
    "fetch": {
      "command": "fetch3.1",
      "args": []
    },
    "git": {
      "command": "git-mcp",
      "args": []
    }
  },
  "tools": [
    "fs_read",
    "fs_write",
    "execute_bash",
    "use_aws",
    "@git",
    "@fetch/fetch_url"
  ],
  "toolAliases": {
    "@git/git_status": "status",
    "@fetch/fetch_url": "get"
  },
  "allowedTools": [
    "fs_read",
    "@git/git_status"
  ],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["src/**", "tests/**", "Cargo.toml"]
    },
    "use_aws": {
      "allowedServices": ["s3", "lambda"]
    }
  },
  "resources": [
    "file://README.md",
    "file://docs/**/*.md"
  ],
  "hooks": {
    "agentSpawn": [
      {
        "command": "git status"
      }
    ],
    "userPromptSubmit": [
      {
        "command": "ls -la"
      }
    ]
  },
  "useLegacyMcpJson": true,
  "model": "claude-sonnet-4"
}
```
