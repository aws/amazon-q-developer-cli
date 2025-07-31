# Migrating Profiles to Agents

All global profiles (created under `~/.aws/amazonq/profiles/`) support automatic migration to the global agents directory under `~/.aws/amazonq/cli-agents/` on initial startup.

If you have local MCP configuration defined under `.amazonq/mcp.json`, then you can optionally add this configuration to a global agent, or create a new workspace agent.

## Creating a New Workspace Agent

Workspace agents are managed under the current working directory inside `.amazonq/cli-agents/`.

You can create a new workspace agent with `q agent create --name my-agent -d .`.

## Global Context

Global context previously configured under `~/.aws/amazonq/global_context.json` is no longer supported. Global context will need to be manually added to agents (see the below section).

## MCP Servers

The agent configuration supports the same MCP format as previously configured.

See [the agent format documentation for more details](./agent-format.md#mcpservers-field).

## Context Files

Context files are now [file URI's](https://en.wikipedia.org/wiki/File_URI_scheme) and configured under the `"resources"` field.

Example from profiles:
```json
{
    "paths": [
        "~/my-files/**/*.txt"
    ]
}
```

Same example for agents:
```json
{
    "resources": [
        "file://~/my-files/**/*.txt"
    ]
}
```

## Hooks

Hook triggers have been updated:
- Hook name is no longer required
- `conversation_start` is now `agentSpawn`
- `per_prompt` is now `userPromptSubmit`

See [the agent format documentation for more details](./agent-format.md#hooks-field).

Example from profiles:
```json
{
    "hooks": {
        "sleep_conv_start": {
            "trigger": "conversation_start",
            "type": "inline",
            "disabled": false,
            "timeout_ms": 30000,
            "max_output_size": 10240,
            "cache_ttl_seconds": 0,
            "command": "echo Conversation start hook"
        },
        "hello_world": {
            "trigger": "per_prompt",
            "type": "inline",
            "disabled": false,
            "timeout_ms": 30000,
            "max_output_size": 10240,
            "cache_ttl_seconds": 0,
            "command": "echo Per prompt hook"
        }
    }
}
```

Same example for agents:
```json
{
    "hooks": {
        "userPromptSubmit": [
            {
                "command": "echo Per prompt hook",
                "timeout_ms": 30000,
                "max_output_size": 10240,
                "cache_ttl_seconds": 0
            }
        ],
        "agentSpawn": [
            {
                "command": "echo Conversation start hook",
                "timeout_ms": 30000,
                "max_output_size": 10240,
                "cache_ttl_seconds": 0
            }
        ]
    }
}
```
