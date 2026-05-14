---
doc_meta:
  title: Trust Configuration
  description: Configure tool auto-approval at session, agent, and directory levels
  category: feature
  keywords: [trust, auto-approve, allowedTools, permissions, tools, security]
  related: [agent-configuration, tools, chat]
  validated: 2026-04-30
  commit: be2c1347
  status: validated
  testable_headless: true
---

# Trust Configuration

Configure tool auto-approval at session, agent, and directory levels.

## Overview

Kiro CLI prompts for approval before executing tools that can modify your system. Trust configuration lets you auto-approve tools to reduce prompts while maintaining control over what the agent can do.

Trust can be configured at multiple levels:
1. **Session** — Temporary trust for current session only
2. **Agent** — Permanent trust defined in agent configuration
3. **CLI flags** — Trust specified at startup

## Quick Reference

| Goal | Method |
|------|--------|
| Trust all tools for one session | `kiro-cli chat --trust-all-tools` |
| Trust specific tools for one session | `kiro-cli chat --trust-tools=read,grep` |
| Trust tools mid-session | `/tools trust write` |
| Permanently trust tools | Add to agent's `allowedTools` |
| Trust all reads in a directory | Use `toolsSettings.read.allowedPaths` |
| Trust all writes in a directory | Use `toolsSettings.write.allowedPaths` |

## Session-Level Trust

### At Startup

```bash
# Trust all tools
kiro-cli chat --trust-all-tools

# Trust specific tools
kiro-cli chat --trust-tools=read,write,shell
```

### Mid-Session

```bash
# Trust a tool
/tools trust write

# Trust multiple tools
/tools trust shell grep

# Trust all tools
/tools trust-all

# Reset to agent defaults
/tools reset
```

Session trust is temporary and resets when you start a new session.

## Agent-Level Trust

Add tools to `allowedTools` in your agent configuration for permanent trust:

```json
{
  "name": "my-agent",
  "allowedTools": [
    "read",
    "grep",
    "glob",
    "code"
  ]
}
```

### Wildcard Patterns

Use patterns to trust groups of tools:

```json
{
  "allowedTools": [
    "fs_*",           
    "@git/*",         
    "@server/read_*"  
  ]
}
```

Pattern syntax:
- `*` — Matches any characters
- `?` — Matches single character
- `@server/*` — All tools from an MCP server
- `@server/prefix_*` — Tools matching prefix from server

### Special Values

- `@builtin` — Trust all built-in tools

## Directory-Level Trust

Use `toolsSettings` to trust operations within specific directories:

### Trust Reads in Directories

```json
{
  "toolsSettings": {
    "read": {
      "allowedPaths": ["~/projects/**", "./src/**"]
    }
  }
}
```

### Trust Writes in Directories

```json
{
  "toolsSettings": {
    "write": {
      "allowedPaths": ["./src/**", "./tests/**"],
      "deniedPaths": ["./src/config/**"]
    }
  }
}
```

- `allowedPaths` — Glob patterns for auto-approved paths
- `deniedPaths` — Glob patterns that always require approval (overrides allowedPaths)

### Trust Bash Commands

```json
{
  "toolsSettings": {
    "shell": {
      "allowedCommands": ["git status", "cargo test", "npm run build"],
      "autoAllowReadonly": true
    }
  }
}
```

- `allowedCommands` — Specific commands to auto-approve
- `autoAllowReadonly` — Auto-approve read-only commands (ls, cat, etc.)

### Trust AWS Operations

```json
{
  "toolsSettings": {
    "use_aws": {
      "allowedServices": ["s3", "lambda"],
      "autoAllowReadonly": true
    }
  }
}
```

## Subagent Trust

Subagents do not inherit trust from the parent agent. Configure trust separately:

### Trust Subagent Spawning

In the parent agent's config:

```json
{
  "toolsSettings": {
    "crew": {
      "trustedAgents": ["research-agent", "code-*"]
    }
  }
}
```

### Trust Tools in Subagents

In each subagent's own config file:

```json
{
  "name": "research-agent",
  "allowedTools": ["read", "grep", "web_search"]
}
```

## Headless Mode

For automation, always specify trust to avoid hanging on prompts:

```bash
# Trust all (use with caution)
kiro-cli chat --no-interactive --trust-all-tools "Run tests"

# Trust specific tools (safer)
kiro-cli chat --no-interactive --trust-tools=read,grep "Find TODOs"
```

## Examples

### Example 1: Read-Only Agent

Agent that can read anything but never write:

```json
{
  "name": "reader",
  "tools": ["read", "grep", "glob", "code"],
  "allowedTools": ["read", "grep", "glob", "code"]
}
```

### Example 2: Project-Scoped Developer

Agent trusted to modify only project files:

```json
{
  "name": "dev",
  "tools": ["read", "write", "shell", "code"],
  "allowedTools": ["read", "code"],
  "toolsSettings": {
    "write": {
      "allowedPaths": ["./src/**", "./tests/**"],
      "deniedPaths": ["./.env", "./secrets/**"]
    },
    "shell": {
      "allowedCommands": ["npm test", "npm run build"],
      "autoAllowReadonly": true
    }
  }
}
```

### Example 3: Full Trust for Automation

Agent for CI/CD with full trust:

```json
{
  "name": "ci-agent",
  "tools": ["read", "write", "shell"],
  "allowedTools": ["read", "write", "shell"]
}
```

### Example 4: MCP Server Trust

Trust all tools from specific MCP servers:

```json
{
  "allowedTools": [
    "@git/*",
    "@github/get_*",
    "@github/list_*"
  ]
}
```

## Trust Precedence

When multiple trust settings apply, they're evaluated in order:

1. `deniedPaths` in toolsSettings — Always blocks
2. `/tools trust-all` — Session-wide trust
3. `/tools trust <tool>` — Session tool trust
4. `allowedTools` in agent config — Permanent trust
5. `toolsSettings` path/command rules — Conditional trust
6. Default — Requires approval

## FAQ

### How do I skip the TUI trust confirmation on startup?

Use `--trust-all-tools` or configure `allowedTools` in your agent. There's no separate setting to skip the startup confirmation — it appears when tools need approval.

### How do I trust all writes within a directory?

Use `toolsSettings.write.allowedPaths`:

```json
{
  "toolsSettings": {
    "write": {
      "allowedPaths": ["./my-project/**"]
    }
  }
}
```

### How do I auto-approve all read tools?

Add read tools to `allowedTools`:

```json
{
  "allowedTools": ["read", "grep", "glob", "code"]
}
```

Or use the pattern `fs_*` if you want all fs tools (includes writes).

### Can I trust tools for subagents from the parent?

No. Each subagent uses its own agent configuration. Add `allowedTools` to the subagent's config file, or run with `--trust-all-tools` to trust everything.

## Troubleshooting

### Tool Still Prompts After Adding to allowedTools

**Cause**: Tool name doesn't match exactly  
**Solution**: Check spelling. Use `/tools` to see exact tool names. MCP tools need `@server/tool` format.

### Path Pattern Not Matching

**Cause**: Glob pattern syntax issue  
**Solution**: Use `**` for recursive matching, `*` for single directory level. Paths are relative to working directory.

### Trust Resets After Session

**Cause**: Using `/tools trust` which is session-only  
**Solution**: Add to agent's `allowedTools` for permanent trust.

## Related

- [Agent Configuration](agent-configuration.md) — Full agent config reference
- [/tools](../slash-commands/tools.md) — Session trust commands
- [kiro-cli chat](../commands/chat.md) — CLI trust flags
- [subagent](../tools/subagent.md) — Subagent trust configuration
