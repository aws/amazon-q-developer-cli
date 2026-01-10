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

Runs before tool execution. Can validate, block, or prompt for confirmation.

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
- **0**: Allow tool execution (or check JSON response for decision).
- **2**: Block tool execution, return STDERR to LLM.
- **Other**: Show STDERR warning to user, allow tool execution.

#### JSON Decision Response

PreToolUse hooks can output a JSON response to STDOUT to control tool execution:

```json
{
  "decision": "ask",
  "message": "⚠️ This command uses sudo. Allow execution?"
}
```

| Decision | Behavior |
|----------|----------|
| `allow` | Allow tool execution (same as empty output) |
| `ask` | Prompt user for confirmation (y/n) |
| `block` | Block tool execution, return message to LLM |

**Example: Sudo confirmation hook**

```python
#!/usr/bin/env python3
import json, sys, re

input_data = json.load(sys.stdin)
command = input_data.get("tool_input", {}).get("command", "")

if re.search(r'(^|;|&&|\|\||\|)\s*sudo(\s|$)', command):
    print(json.dumps({
        "decision": "ask",
        "message": f"⚠️ This command uses sudo:\n\n  {command}\n\nAllow execution?"
    }))
sys.exit(0)
```

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
  "cwd": "/current/working/directory"
}
```

**Exit Code Behavior:**
- **0**: Hook succeeded.
- **Other**: Show STDERR warning to user.

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