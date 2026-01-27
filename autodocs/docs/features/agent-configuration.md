---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: true
  category: feature
  title: Agent Configuration
  description: Complete guide to agent configuration format including tools, settings, resources, hooks, and MCP servers
  keywords: [agent, configuration, json, tools, settings, resources, hooks, mcp, keyboardShortcut, welcomeMessage, skill]
  related: [cmd-agent, slash-agent, slash-agent-generate]
---

# Agent Configuration

Complete guide to agent configuration format including tools, settings, resources, hooks, and MCP servers.

## Overview

Agent configurations are JSON files that define agent behavior, available tools, context files, and integrations. Stored in `.kiro/agents/` (local) or `~/.kiro/agents/` (global). Filename (without `.json`) becomes agent name.

## File Location

**Local (workspace)**: `.kiro/agents/<name>.json`  
**Global (user-wide)**: `~/.kiro/agents/<name>.json`

Local agents take precedence over global with same name.

## Basic Structure

```json
{
  "name": "my-agent",
  "description": "Agent description",
  "prompt": "System prompt or file:///path/to/prompt.txt",
  "tools": ["fs_read", "fs_write", "execute_bash"],
  "allowedTools": ["fs_read", "grep"],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/projects/**"]
    }
  },
  "resources": ["src/**/*.rs", "Cargo.toml"],
  "hooks": {
    "onStart": {
      "command": "git status",
      "description": "Show git status"
    }
  },
  "mcpServers": {
    "git": {
      "command": "mcp-server-git",
      "args": ["--stdio"]
    }
  }
}
```

## Fields

### name

Agent name for identification.

```json
{
  "name": "rust-expert"
}
```

### description

Human-readable description.

```json
{
  "description": "Rust development expert with cargo and testing tools"
}
```

### prompt

System prompt that defines agent behavior and expertise (inline text or file URI).

```json
{
  "prompt": "You are a Rust expert focused on safety, performance, and idiomatic code. Help with debugging, optimization, and best practices."
}
```

**Use cases**:
- Define agent personality and expertise area
- Set specific instructions for code style or approach  
- Reference external prompt files for complex instructions
- Customize response tone and format preferences

Or reference file:

```json
{
  "prompt": "file:///home/user/.kiro/prompts/rust-expert.txt"
}
```

### tools

Available tools for agent.

```json
{
  "tools": ["fs_read", "fs_write", "execute_bash", "grep", "code"]
}
```

### allowedTools

Tools auto-approved without prompts. Supports exact matches and wildcard patterns.

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

**Exact Matches**:
- Built-in tools: `"fs_read"`, `"execute_bash"`
- Specific MCP tools: `"@server_name/tool_name"`
- All tools from server: `"@server_name"`

**Wildcard Patterns** (using `*` and `?`):
- Prefix: `"fs_*"` → matches `fs_read`, `fs_write`
- Suffix: `"*_bash"` → matches `execute_bash`
- Middle: `"fs_*_tool"` → matches `fs_read_tool`
- Single char: `"fs_?ead"` → matches `fs_read`, `fs_head`
- MCP tool: `"@server/read_*"` → matches `@server/read_file`, `@server/read_config`
- MCP server: `"@git-*/*"` → matches any tool from servers matching `git-*`

**Special**:
- `"@builtin"` → All built-in tools
- `"*"` → NOT supported in allowedTools (use in tools field only)

### toolsSettings

Tool-specific configuration.

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/projects/**"],
      "deniedPaths": ["/etc/**"]
    },
    "execute_bash": {
      "allowedCommands": ["git status", "cargo check"],
      "autoAllowReadonly": true
    }
  }
}
```

### resources

Context files loaded into agent context. Supports `file://` and `skill://` URI schemes.

```json
{
  "resources": [
    "file://README.md",
    "file://src/**/*.rs",
    "file://Cargo.toml",
    "skill://.kiro/skills/**/SKILL.md"
  ]
}
```

**URI Schemes**:
- `file://` - Files always loaded into context
- `skill://` - Skills progressively loaded on demand

**Both support**:
- Specific paths: `file://README.md` or `skill://my-skill.md`
- Glob patterns: `file://src/**/*.rs` or `skill://.kiro/skills/**/SKILL.md`
- Absolute or relative paths

**Skills**: A skill is a resource whose metadata (name, description, path) is loaded at startup, with full content loaded on demand. Skill files must begin with YAML frontmatter:

```markdown
---
name: dynamodb-data-modeling
description: Guide for DynamoDB data modeling best practices. Use when designing or analyzing DynamoDB schema.
---
```
```

### hooks

Commands executed at trigger points.

```json
{
  "hooks": {
    "agentSpawn": [
      {
        "command": "git status",
        "description": "Show repository status"
      }
    ],
    "userPromptSubmit": [
      {
        "command": "date",
        "description": "Current timestamp"
      }
    ],
    "preToolUse": [
      {
        "matcher": "fs_write",
        "command": "git diff",
        "description": "Show changes before write"
      }
    ],
    "postToolUse": [
      {
        "matcher": "execute_bash",
        "command": "echo 'Command executed'",
        "description": "Log after execution"
      }
    ],
    "stop": [
      {
        "command": "echo 'Response complete'",
        "description": "Log completion"
      }
    ]
  }
}
```

**Hook Triggers**:
- `agentSpawn`: When agent initializes
- `userPromptSubmit`: When user submits message
- `preToolUse`: Before tool execution (can block)
- `postToolUse`: After tool execution
- `stop`: When assistant finishes responding

**Hook Fields**:
- `command` (required): Command to execute
- `matcher` (optional): Pattern for preToolUse/postToolUse
- `description` (optional): Human-readable description

### toolAliases

Remap tool names to resolve collisions.

```json
{
  "toolAliases": {
    "@github-mcp/get_issues": "github_issues",
    "@gitlab-mcp/get_issues": "gitlab_issues"
  }
}
```

Useful when multiple MCP servers provide tools with same name.

### useLegacyMcpJson

Include MCP servers from legacy config files.

```json
{
  "useLegacyMcpJson": true
}
```

Loads servers from `~/.aws/amazonq/mcp.json` (global) and `.amazonq/mcp.json` (workspace).

### model

Specify model ID for agent.

```json
{
  "model": "<model-id>"
}
```

If not specified, uses default model. Falls back to default if model unavailable.

### mcpServers

MCP server configurations.

```json
{
  "mcpServers": {
    "git": {
      "command": "mcp-server-git",
      "args": ["--stdio"],
      "env": {
        "GIT_DIR": "/path/to/repo"
      },
      "timeout": 120000
    }
  }
}
```

**MCP Server Fields**:
- `command` (required): Command to start server
- `args` (optional): Command arguments
- `env` (optional): Environment variables
- `timeout` (optional): Request timeout in milliseconds (default: 120000)

### toolAliases

Remap tool names to resolve naming collisions.

```json
{
  "toolAliases": {
    "@github-mcp/get_issues": "github_issues",
    "@gitlab-mcp/get_issues": "gitlab_issues",
    "@aws-tools/deploy_stack": "deploy"
  }
}
```

Use when multiple MCP servers provide tools with same name, or to create shorter names.

### useLegacyMcpJson

Include MCP servers from legacy configuration files.

```json
{
  "useLegacyMcpJson": true
}
```

**Alias**: `includeMcpJson`

When `true`, loads MCP servers from:
- Global: `~/.aws/amazonq/mcp.json`
- Workspace: `.amazonq/mcp.json`

Tools from legacy servers can be referenced same as servers in `mcpServers` field.

### model

Specify model ID for this agent.

```json
{
  "model": "<model-id>"
}
```

If not specified, uses default model. Falls back to default if specified model unavailable.

### keyboardShortcut

Keyboard shortcut for quickly switching to this agent during a chat session.

```json
{
  "keyboardShortcut": "ctrl+shift+a"
}
```

**Format**: Modifiers and key separated by `+`

**Modifiers** (optional, can combine):
- `ctrl` - Control key
- `shift` - Shift key
- `alt` - Alt key (Option on Mac)

**Keys**:
- Letters: `a-z` (case insensitive)
- Digits: `0-9`
- Function keys: `f1-f12`
- Special: `tab`

**Examples**:
```json
"keyboardShortcut": "ctrl+a"           // Control + A
"keyboardShortcut": "ctrl+shift+b"     // Control + Shift + B
"keyboardShortcut": "alt+f1"           // Alt + F1
"keyboardShortcut": "shift+tab"        // Shift + Tab
```

**Toggle Behavior**: Pressing shortcut while already on this agent switches back to previous agent.

**Conflicts**: If multiple agents have same shortcut, warning is logged and shortcut disabled.

### welcomeMessage

Message displayed when switching to this agent.

```json
{
  "welcomeMessage": "What would you like to build today?"
}
```

Appears after agent switch confirmation to orient users to agent's purpose.

## Complete Example

```json
{
  "name": "rust-dev",
  "description": "Rust development agent with full toolset",
  "prompt": "You are an expert Rust developer. Focus on safety, performance, and idiomatic code.",
  "tools": [
    "fs_read",
    "fs_write",
    "execute_bash",
    "grep",
    "glob",
    "code"
  ],
  "allowedTools": [
    "fs_read",
    "grep",
    "glob"
  ],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/rust-projects/**"],
      "deniedPaths": ["~/.cargo/**"]
    },
    "execute_bash": {
      "allowedCommands": [
        "cargo check",
        "cargo test",
        "cargo build"
      ],
      "autoAllowReadonly": true
    }
  },
  "resources": [
    "file://src/**/*.rs",
    "file://Cargo.toml",
    "skill://.kiro/skills/**/SKILL.md"
  ],
  "hooks": {
    "agentSpawn": [
      {
        "command": "cargo --version && rustc --version",
        "description": "Show Rust toolchain versions"
      }
    ],
    "stop": [
      {
        "command": "echo 'Response complete'",
        "description": "Log completion"
      }
    ]
  },
  "mcpServers": {
    "git": {
      "command": "mcp-server-git",
      "args": ["--stdio"]
    }
  },
  "keyboardShortcut": "ctrl+shift+r",
  "welcomeMessage": "Ready to help with Rust development!"
}
```

## Creating Agents

### Method 1: AI Generation

```
/agent generate
```

Interactive process with AI assistance.

### Method 2: CLI Command

```bash
kiro-cli agent create --name my-agent
```

Creates example configuration.

### Method 3: Manual Creation

Create `.kiro/agents/my-agent.json` with desired configuration.

## Validation

```bash
kiro-cli agent validate --path ~/.kiro/agents/my-agent.json
```

Checks JSON syntax and schema compliance.

## Examples

### Example 1: Simple File Reader Agent

```json
{
  "name": "reader",
  "description": "Read-only agent for browsing code",
  "tools": ["fs_read", "grep", "glob"],
  "allowedTools": ["fs_read", "grep", "glob"],
  "resources": ["src/**/*", "README.md"]
}
```

### Example 2: Rust Development Agent

```json
{
  "name": "rust-dev",
  "description": "Rust development with testing",
  "prompt": "You are a Rust expert. Focus on safety and performance.",
  "tools": ["fs_read", "fs_write", "execute_bash", "code"],
  "allowedTools": ["fs_read", "code"],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["src/**", "tests/**"],
      "deniedPaths": ["target/**"]
    },
    "execute_bash": {
      "allowedCommands": ["cargo check", "cargo test", "cargo build"],
      "autoAllowReadonly": true
    }
  },
  "resources": ["src/**/*.rs", "Cargo.toml"],
  "hooks": {
    "onStart": {
      "command": "cargo --version",
      "description": "Show Rust version"
    }
  }
}
```

### Example 3: AWS Operations Agent

```json
{
  "name": "aws-ops",
  "description": "AWS operations and management",
  "tools": ["use_aws", "fs_read", "execute_bash"],
  "toolsSettings": {
    "use_aws": {
      "allowedServices": ["s3", "lambda", "ec2"],
      "autoAllowReadonly": true
    }
  }
}
```

### Example 4: Agent with MCP Servers

```json
{
  "name": "full-stack",
  "description": "Full-stack development with git integration",
  "tools": ["fs_read", "fs_write", "execute_bash", "code"],
  "mcpServers": {
    "git": {
      "command": "mcp-server-git",
      "args": ["--stdio"]
    },
    "github": {
      "command": "mcp-server-github",
      "args": ["--stdio"],
      "env": {
        "GITHUB_TOKEN": "$GITHUB_TOKEN"
      }
    }
  }
}
```

## Troubleshooting

### Issue: Agent Not Found

**Symptom**: "Agent not found" error  
**Cause**: Agent file doesn't exist or invalid name  
**Solution**: Check file exists in `.kiro/agents/` or `~/.kiro/agents/`. Use `kiro-cli agent list`.

### Issue: Invalid JSON

**Symptom**: "Json supplied is invalid" error  
**Cause**: Syntax error in JSON  
**Solution**: Validate JSON syntax. Use `kiro-cli agent validate <name>` for details.

### Issue: Schema Mismatch

**Symptom**: "Agent config is malformed" error  
**Cause**: JSON doesn't match agent schema  
**Solution**: Check required fields. Use `kiro-cli agent schema` to see format.

### Issue: Tool Not Available

**Symptom**: Tool in config but not working  
**Cause**: Tool name incorrect or MCP server not loaded  
**Solution**: Check tool name spelling. For MCP tools, ensure server configured.

### Issue: Glob Pattern Not Matching

**Symptom**: Resources not loading files  
**Cause**: Invalid glob pattern  
**Solution**: Test pattern. Use `**` for recursive, `*` for single level.

### Issue: Hook Not Executing

**Symptom**: Hook command not running  
**Cause**: Invalid command or permission issue  
**Solution**: Test command in terminal first. Check command exists and is executable.

### Issue: MCP Server Won't Start

**Symptom**: MCP tools not available  
**Cause**: Server command not found or configuration error  
**Solution**: Verify server installed. Check command path and args.

### Issue: Local Agent Not Overriding Global

**Symptom**: Global agent used instead of local  
**Cause**: Names don't match exactly  
**Solution**: Ensure filenames match exactly (case-sensitive).

### Issue: Tool Settings Not Applied

**Symptom**: Tool permissions not working as configured  
**Cause**: Tool name mismatch in toolsSettings  
**Solution**: Use exact tool name. For MCP: `@server-name/tool-name`.

### Issue: Prompt File Not Found

**Symptom**: Error loading prompt from file URI  
**Cause**: Invalid file path  
**Solution**: Use absolute path with `file://` prefix. Verify file exists.

## Related

- [kiro-cli agent](../commands/agent.md) - Manage agents
- [/agent](../slash-commands/agent-switch.md) - Switch agents
- [/agent generate](../slash-commands/agent-generate.md) - Generate with AI
- [Hooks](../features/hooks.md) - Hook system details
