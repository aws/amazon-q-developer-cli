# Exit Codes

The Kiro CLI uses specific exit codes to indicate the status of operations. These codes can be used in scripts and CI/CD pipelines to detect different types of failures.

## Exit Code Reference

| Exit Code | Name | Description |
|-----------|------|-------------|
| 0 | Success | The command completed successfully |
| 1 | Failure | General failure (e.g., authentication error, invalid arguments, operation failed) |
| 3 | MCP Startup Failure | One or more MCP servers failed to start (only with `--require-mcp-startup`) |

## Detailed Descriptions

### Exit Code 0 - Success

The command completed without errors.

### Exit Code 1 - General Failure

Returned when a command fails for various reasons, including:
- Authentication errors (not logged in, expired token)
- Invalid command-line arguments
- Operation failures (e.g., failed to delete a session)
- Unexpected errors during execution

### Exit Code 3 - MCP Startup Failure

Returned when the `--require-mcp-startup` flag is provided and one or more MCP servers fail to start. This is useful for CI/CD pipelines or scripts that require all configured MCP servers to be available.

**Usage:**
```bash
kiro-cli chat --require-mcp-startup
```

**Example in a script:**
```bash
#!/bin/bash
kiro-cli chat --require-mcp-startup --no-interactive "Run my task"
exit_code=$?

if [ $exit_code -eq 3 ]; then
    echo "MCP servers failed to start"
    exit 1
elif [ $exit_code -ne 0 ]; then
    echo "Command failed with exit code $exit_code"
    exit $exit_code
fi
```

## Hook Exit Codes

Hooks use a separate set of exit codes to control behavior:

| Exit Code | Behavior |
|-----------|----------|
| 0 | Hook succeeded |
| 2 | (PreToolUse only) Block tool execution; STDERR is returned to the LLM |
| Other | Hook failed; STDERR is shown as a warning to the user |

For more details on hooks, see [Hooks](hooks.md).
