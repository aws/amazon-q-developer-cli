---
doc_meta:
  title: Exit Codes
  description: CLI exit codes for scripting and CI/CD integration
  category: feature
  keywords: [exit, code, status, error, ci, cd, automation, script, mcp]
  related: [chat, hooks]
  validated: 2026-01-28
  commit: 0b13a71c
  status: validated
  testable_headless: true
---

# Exit Codes

Kiro CLI uses specific exit codes to indicate operation status. Use these in scripts and CI/CD pipelines to detect different failure types.

## Overview

Exit codes allow programmatic detection of success, general failures, and specific error conditions like MCP startup failures.

## Exit Code Reference

| Code | Name | Description |
|------|------|-------------|
| 0 | Success | Command completed successfully |
| 1 | Failure | General failure (auth error, invalid args, operation failed) |
| 3 | MCP Startup Failure | MCP server failed to start (with `--require-mcp-startup`) |

## Usage

### Detecting MCP Failures

Use `--require-mcp-startup` to fail fast when MCP servers don't start:

```bash
kiro-cli chat --require-mcp-startup --no-interactive "Run task"
```

If any configured MCP server fails to start, exits with code 3.

### Script Example

```bash
#!/bin/bash
kiro-cli chat --require-mcp-startup --no-interactive --trust-all-tools "Run analysis"
exit_code=$?

case $exit_code in
    0) echo "Success" ;;
    3) echo "MCP servers failed to start"; exit 1 ;;
    *) echo "Failed with code $exit_code"; exit $exit_code ;;
esac
```

### CI/CD Example

```yaml
- name: Run Kiro task
  run: |
    kiro-cli chat --require-mcp-startup --no-interactive --trust-all-tools "Analyze code"
  continue-on-error: false
```

## Examples

### Example 1: Check Exit Code

```bash
kiro-cli chat --no-interactive "Hello"
echo "Exit code: $?"
```

### Example 2: Require MCP Servers

```bash
kiro-cli chat --require-mcp-startup --no-interactive "Use MCP tool"
# Exits 3 if MCP servers fail to start
```

### Example 3: Conditional Logic

```bash
if kiro-cli chat --require-mcp-startup --no-interactive "Task"; then
    echo "Task completed"
else
    echo "Task failed"
fi
```

## Hook Exit Codes

Hooks use separate exit codes:

| Code | Behavior |
|------|----------|
| 0 | Hook succeeded |
| 2 | (PreToolUse only) Block tool execution; STDERR returned to LLM |
| Other | Hook failed; STDERR shown as warning |

See [Hooks](hooks.md) for details.

## Troubleshooting

### Issue: Unexpected Exit Code 3

**Symptom**: Script fails with exit code 3  
**Cause**: MCP server failed to start with `--require-mcp-startup`  
**Solution**: Check MCP configuration. Verify server paths and dependencies.

### Issue: Exit Code 1 Without Details

**Symptom**: General failure without clear message  
**Cause**: Various issues (auth, args, runtime errors)  
**Solution**: Add `-v` or `-vv` for verbose output to diagnose.

## Related Features

- [kiro-cli chat](../commands/chat.md) - Chat command with `--require-mcp-startup`
- [Hooks](hooks.md) - Hook exit codes

## Limitations

- Exit code 3 only returned when `--require-mcp-startup` is used
- Hook exit codes are separate from CLI exit codes
- No exit code for partial MCP failures without the flag

## Technical Details

**Exit Code 3**: Only returned when `--require-mcp-startup` flag is provided and at least one enabled MCP server fails to start. Without this flag, MCP failures are logged as warnings but don't affect the exit code.

**Timeout**: MCP startup status check has a 1-second timeout. If the check times out, it's treated as a failure when `--require-mcp-startup` is enabled.
