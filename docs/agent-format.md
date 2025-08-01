# Agent Format

The agent configuration file for each agent is a JSON file. The filename (without the `.json` extension) becomes the agent's name. It contains configuration needed to instantiate and run the agent.

Every agent configuration file can include the following sections:

- [`name`](#name-field) — The name of the agent (optional, derived from filename if not specified).
- [`description`](#description-field) — A description of the agent.
- [`prompt`](#prompt-field) — High-level context for the agent (not yet implemented).
- [`mcpServers`](#mcpservers-field) — The MCP servers the agent has access to.
- [`tools`](#tools-field) — The tools available to the agent.
- [`toolAliases`](#toolaliases-field) — Tool name remapping for handling naming collisions.
- [`allowedTools`](#allowedtools-field) — Tools that can be used without prompting.
- [`toolsSettings`](#toolssettings-field) — Configuration for specific tools.
- [`resources`](#resources-field) — Resources available to the agent.
- [`hooks`](#hooks-field) — Commands run at specific trigger points.
- [`useLegacyMcpJson`](#uselegacymcpjson-field) — Whether to include legacy MCP configuration.

## Name Field

The `name` field specifies the name of the agent. This is used for identification and display purposes. 

```json
{
  "name": "aws-expert"
}
```

Note: While this field can be included in the configuration file, it will be overridden by the filename when the agent is loaded.

## Description Field

The `description` field provides a description of what the agent does. This is primarily for human readability and helps users distinguish between different agents.

```json
{
  "description": "An agent specialized for AWS infrastructure tasks"
}
```

## Prompt Field

The `prompt` field is intended to provide high-level context to the agent, similar to a system prompt. This feature is not yet implemented.

```json
{
  "prompt": "You are an expert AWS infrastructure specialist"
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
    "@git/git_status",
    "@fetch"
  ]
}
```

You can allow:
- Specific built-in tools by name (e.g., `"fs_read"`)
- Specific MCP tools using `@server_name/tool_name` (e.g., `"@git/git_status"`)
- All tools from an MCP server using `@server_name` (e.g., `"@fetch"`)

Unlike the `tools` field, the `allowedTools` field does not support the `"*"` wildcard for allowing all tools. To allow specific tools, you must list them individually or use server-level wildcards with the `@server_name` syntax.

## ToolsSettings Field

The `toolsSettings` field provides configuration for specific tools. Each tool can have its own unique configuration options.

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

The `hooks` field defines commands to run at specific trigger points. The output of these commands is added to the agent's context.

```json
{
  "hooks": {
    "agentSpawn": [
      {
        "command": "git status",
      }
    ],
    "userPromptSubmit": [
      {
        "command": "ls -la",
      }
    ]
  }
}
```

Each hook is defined with:
- `command` (required): The command to execute

Available hook triggers:
- `agentSpawn`: Triggered when the agent is initialized
- `userPromptSubmit`: Triggered when the user submits a message

## UseLegacyMcpJson Field

The `useLegacyMcpJson` field determines whether to include MCP servers defined in the legacy global MCP configuration file (`~/.aws/amazonq/mcp.json`).

```json
{
  "useLegacyMcpJson": true
}
```

When set to `true`, the agent will have access to all MCP servers defined in the global configuration in addition to those defined in the agent's `mcpServers` field.

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
        "command": "git status",
      }
    ],
    "userPromptSubmit": [
      {
        "command": "ls -la",
      }
    ]
  },
  "useLegacyMcpJson": true
}
```
