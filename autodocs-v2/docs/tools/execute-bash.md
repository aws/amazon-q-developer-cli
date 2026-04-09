---
doc_meta:
  title: execute_bash
  description: Execute bash commands on the user's system with output capture
  category: tool
  keywords: [execute_bash, shell, bash, command, terminal, run, working_dir]
  related: [fs-read, fs-write, use-aws]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The execute_bash tool runs shell commands and captures stdout, stderr, and exit codes. It is used as a last resort when no other tool can accomplish the task.

Limitations:
- Output is buffered — appears after command completes, not streamed in real-time
- Interactive commands that require stdin (e.g., `rm -i`, `npm init`, `sudo`) will not work — use non-interactive alternatives (`rm` without `-i`, `npm init -y`, etc.)
- Does not respect the user's bash profile or aliases
- Use `working_dir` instead of prefixing commands with `cd`

## Usage

### Parameters

- `command` (string, required) — The bash command to execute
- `working_dir` (string, optional) — Working directory for command execution. Defaults to the current working directory

## Examples

### List directory contents

```json
{
  "command": "ls -la src/"
}
```

### Run in a specific directory

```json
{
  "command": "npm install",
  "working_dir": "/path/to/frontend"
}
```

### Check git status

```json
{
  "command": "git status --short"
}
```

## Troubleshooting

### Command not found

The tool uses a bare shell without the user's profile. Commands that depend on aliases or profile-loaded paths may not work. Use full paths to binaries if needed.

### Working directory not found

The `working_dir` must be an existing directory. Verify the path exists before using it.

### Output truncated

Large outputs are truncated to prevent context window overflow. Use `head`, `tail`, or `grep` to limit output in the command itself.

## Related

- [fs_read](fs-read.md) — Prefer this for reading files instead of `cat`
- [fs_write](fs-write.md) — Prefer this for writing files instead of `echo >`
- [grep](grep.md) — Prefer this for searching files instead of `grep` command
- [use_aws](use-aws.md) — Prefer this for AWS CLI calls
