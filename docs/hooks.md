# Hooks

Hooks allow you to execute custom commands at specific points during agent lifecycle and tool execution. This enables security validation, logging, formatting, context gathering, and other custom behaviors.

## Defining Hooks

Hooks are defined in the agent configuration file. See the [agent format documentation](agent-format.md#hooks-field) for the complete syntax and examples.

## Hook Event

Hooks receive hook event in JSON format via STDIN:

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

### Examples
- `"fs_write"` - Exact match for built-in tools
- `"fs_*"` - Wildcard pattern for built-in tools
- `"@git"` - All tools from git MCP server
- `"@git/status"` - Specific tool from git MCP server
- `"*"` - All tools (built-in and MCP)
- `"@builtin"` - All built-in tools only
- No matcher - Applies to all tools

For complete tool reference format, see [agent format documentation](agent-format.md#tools-field).

## Hook Types

### AgentSpawn

Runs when agent is activated. No tool context provided.

**Hook Event**
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

**Hook Event**
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

**Hook Event**
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

**Hook Event**
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

### Stop

Runs when the assistant finishes responding to the user (at the end of each turn). 
This is useful for running post-processing tasks like code compilation, testing, formatting, 
or cleanup after the assistant's response.

**Hook Event**
```json
{
  "hook_event_name": "stop",
  "cwd": "/current/working/directory",
  "session_id": "conversation-uuid",
  "assistant_response": "The assistant's last response text..."
}
```

- `session_id`: The current conversation/session identifier.
- `assistant_response`: The text content of the assistant's last response. Use this to inspect what the agent said without needing to parse a transcript.

**Exit Code Behavior:**
- **0**: Hook succeeded. If STDOUT contains JSON with a block decision (see below), the agent continues instead of stopping.
- **Other**: Show STDERR warning to user.

**Block Decision:** A stop hook can prevent the agent from stopping by returning JSON on STDOUT:

```json
{"decision": "block", "reason": "You haven't run the tests yet."}
```

When a stop hook returns `decision: "block"`, the `reason` is sent as a new user message to the LLM, continuing the conversation. This enables a feedback loop where a script can evaluate the agent's work and nudge it to keep going — e.g., checking if tests pass, verifying lint rules, or confirming that required files were updated before allowing the agent to stop.

If no block decision is returned (no output, non-JSON output, or JSON without `"decision": "block"`), the agent stops normally. This is fully backward compatible.

**Note**: Stop hooks do not use matchers since they don't relate to specific tools.

### MCP Example

For MCP tools, the tool name includes the full namespaced format including the MCP Server name:

**Hook Event**
```json
{
  "hook_event_name": "preToolUse",
  "cwd": "/current/working/directory",
  "tool_name": "@postgres/query",
  "tool_input": {
    "sql": "SELECT * FROM orders LIMIT 10;"
  }
}
```

## Timeout

Default timeout is 30 seconds (30,000ms). Configure with `timeout_ms` field.

## Caching

Successful hook results are cached based on `cache_ttl_seconds`:
- `0`: No caching (default)
- `> 0`: Cache successful results for specified seconds
- AgentSpawn hooks are never cached