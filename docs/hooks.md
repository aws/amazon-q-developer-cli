# Agent Hooks

Agent hooks allow you to execute shell commands at specific trigger points during the agent's lifecycle. The output of these commands is added to the agent's context, enabling dynamic information injection and external notifications.

## Hook Configuration

Hooks are configured in the agent's JSON file under the `hooks` field:

```json
{
  "hooks": {
    "agentSpawn": [...],
    "userPromptSubmit": [...],
    "agentNeedsAttention": [...]
  }
}
```

Each hook trigger accepts an array of hook commands with the following properties:

- `command` (required): The shell command to execute
- `timeout_ms` (optional): Maximum execution time in milliseconds (default: 30000)
- `max_output_size` (optional): Maximum output size in bytes (default: 10240)
- `cache_ttl_seconds` (optional): How long to cache results in seconds (default: 0)

## Available Hook Triggers

### agentSpawn
Triggered once when the agent is initialized.

**Environment Variables:**
- `USER_PROMPT`: The initial user prompt (if any)

**Example:**
```json
{
  "agentSpawn": [
    {
      "command": "git status --porcelain",
      "timeout_ms": 5000
    }
  ]
}
```

### userPromptSubmit
Triggered each time the user submits a message.

**Environment Variables:**
- `USER_PROMPT`: The current user prompt (truncated to 4096 chars)

**Example:**
```json
{
  "userPromptSubmit": [
    {
      "command": "echo \"Processing: $USER_PROMPT\" >> /tmp/q-prompts.log",
      "timeout_ms": 2000
    }
  ]
}
```

### agentNeedsAttention
Triggered when the agent needs user attention. Currently supports tool approval and completion notifications.

**Environment Variables:**
- `USER_PROMPT`: The current user prompt (truncated to 4096 chars)
- `ATTENTION_REASON`: Reason for attention e.g. `tool_approval`
- `TOOL_NAME`: Name of the tool requiring approval (when ATTENTION_REASON=tool_approval)
- `TOOL_COMMAND`: Command/arguments that require approval (when ATTENTION_REASON=tool_approval)

## Complete Example Agent

```json
{
  "name": "notification-agent",
  "description": "Agent with comprehensive notification hooks",
  "hooks": {
    "agentSpawn": [
      {
        "command": "echo \"[$(date)] Agent started\" >> /tmp/q-agent.log",
        "timeout_ms": 2000
      },
      {
        "command": "if command -v osascript >/dev/null 2>&1; then osascript -e 'display notification \"Q Developer agent ready\" with title \"Q Developer\"'; fi",
        "timeout_ms": 3000
      }
    ],
    "userPromptSubmit": [
      {
        "command": "echo \"[$(date)] User: $USER_PROMPT\" >> /tmp/q-agent.log",
        "timeout_ms": 2000,
        "max_output_size": 256
      }
    ],
    "agentNeedsAttention": [
      {
        "command": "echo \"[$(date)] Attention: $ATTENTION_REASON\" >> /tmp/q-agent.log",
        "timeout_ms": 2000
      },
      {
        "command": "if [ \"$ATTENTION_REASON\" = \"tool_approval\" ] && command -v osascript >/dev/null 2>&1; then osascript -e \"display notification \\\"Approve: $TOOL_NAME\\\" with title \\\"Q Developer\\\" sound name \\\"Glass\\\"\"; fi",
        "timeout_ms": 5000,
        "cache_ttl_seconds": 0
      }
    ]
  }
}
```
