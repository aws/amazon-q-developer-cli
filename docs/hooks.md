# Hooks

Hooks allow you to execute custom commands at specific points during agent lifecycle and tool execution. This enables security validation, logging, formatting, context gathering, and other custom behaviors.

## Defining Hooks

Hooks are defined in the agent configuration file. See the [agent format documentation](agent-format.md#hooks-field) for the complete syntax and examples.

## Hook Input (STDIN)

Hooks receive JSON input via STDIN containing context about the hook execution:

```json
{
  "hook_event_name": "agentSpawn",
  "cwd": "/current/working/directory"
}
```

For tool-related hooks, additional fields are included:
- `tool_name`: Name of the tool being executed
- `tool_input`: Tool-specific parameters (see individual tool documentation)
- `tool_response`: Tool execution results (PostToolUse only)

## Hook Output

- **Exit code 0**: Hook succeeded. STDOUT is captured but not shown to user.
- **Exit code 2**: (PreToolUse only) Block tool execution. STDERR is returned to the LLM.
- **Other exit codes**: Hook failed. STDERR is shown as warning to user.

## Tool Matching

Use the `matcher` field to specify which tools the hook applies to:
- `"fs_write"` - Exact match for built-in tools
- `"fs_*"` - Wildcard pattern for built-in tools
- `"query"` - Exact match for MCP tools (e.g., query tool from postgres MCP, uses base tool name only)
- No matcher - Applies to all tools

## Hook Types

### AgentSpawn

Runs when agent is activated. No tool context provided.

**Input JSON:**
```json
{
  "hook_event_name": "agentSpawn",
  "cwd": "/current/working/directory"
}
```

**Exit Code Behavior:**
- **0**: Hook succeeded, STDOUT is added to agent's context
- **Other**: Show STDERR warning to user

### UserPromptSubmit

Runs when user submits a prompt. Output is added to conversation context.

**Input JSON:**
```json
{
  "hook_event_name": "userPromptSubmit",
  "cwd": "/current/working/directory",
  "prompt": "user's input prompt"
}
```

**Exit Code Behavior:**
- **0**: Hook succeeded, STDOUT is added to agent's context
- **Other**: Show STDERR warning to user

### PreToolUse

Runs before tool execution. Can validate and block tool usage.

**Input JSON:**
```json
{
  "hook_event_name": "preToolUse",
  "cwd": "/current/working/directory",
  "tool_name": "fs_read",
  "tool_input": {
    "operations": [
      {
        "mode": "Line",
        "path": "/current/working/directory/docs/hooks.md"
      }
    ]
  }
}
```

**Exit Code Behavior:**
- **0**: Allow tool execution.
- **2**: Block tool execution, return STDERR to LLM.
- **Other**: Show STDERR warning to user, allow tool execution.

### PostToolUse

Runs after tool execution with access to tool results.

**Input JSON:**
```json
{
  "hook_event_name": "postToolUse",
  "cwd": "/current/working/directory",
  "tool_name": "fs_read",
  "tool_input": {
    "operations": [
      {
        "mode": "Line",
        "path": "/current/working/directory/docs/hooks.md"
      }
    ]
  },
  "tool_response": {
    "success": true,
    "result": ["# Hooks\n\nHooks allow you to execute..."]
  }
}
```

**Exit Code Behavior:**
- **0**: Hook succeeded.
- **Other**: Show STDERR warning to user. Tool already ran.

## Timeout

Default timeout is 30 seconds (30,000ms). Configure with `timeout_ms` field.

## Caching

Successfull hook results are cached based on `cache_ttl_seconds`:
- `0`: No caching (default)
- `> 0`: Cache successful results for specified seconds
- AgentSpawn hooks are never cached