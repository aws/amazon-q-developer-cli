---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: execute_bash
  description: Execute bash commands on the user's system with output capture and safety checks
  keywords: [execute_bash, shell, bash, command, terminal, run]
  related: [fs-read, fs-write, use-aws]
---

# execute_bash

Execute bash commands on the user's system with output capture and safety checks.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to execute commands as needed.

The execute_bash tool runs shell commands and captures stdout, stderr, and exit codes. It includes safety checks for dangerous patterns and supports configuration for auto-allowing read-only commands or specific command patterns.

## How It Works

Commands are validated against allowed/denied lists and safety patterns before execution. Read-only commands (ls, cat, pwd, etc.) can be auto-approved. Multi-line commands and dangerous patterns (pipes, redirects, command substitution) require explicit approval unless configured otherwise.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Basic Usage

```json
{
  "command": "ls -la"
}
```

### Common Use Cases

#### Use Case 1: List Files

```json
{
  "command": "ls -la src/",
  "summary": "List source directory contents"
}
```

**What this does**: Executes ls command and returns output. Auto-approved if autoAllowReadonly is enabled.

#### Use Case 2: Check Git Status

```json
{
  "command": "git status",
  "summary": "Check repository status"
}
```

**What this does**: Runs git status and captures output. Requires approval unless in allowedCommands.

#### Use Case 3: Run Tests

```json
{
  "command": "cargo test --lib",
  "summary": "Run library tests"
}
```

**What this does**: Executes test command and returns results with exit code.

#### Use Case 4: Search Files

```json
{
  "command": "find . -name '*.rs' -type f",
  "summary": "Find all Rust files"
}
```

**What this does**: Searches for files matching pattern. Auto-approved if no dangerous flags (-exec, -delete).

## Configuration

Configure command restrictions in agent's `toolsSettings`:

```json
{
  "toolsSettings": {
    "execute_bash": {
      "allowedCommands": ["git status", "git fetch", "cargo check"],
      "deniedCommands": ["rm -rf .*", "git push .*"],
      "autoAllowReadonly": true,
      "denyByDefault": false
    }
  }
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedCommands` | array | `[]` | Commands allowed without prompting. Supports regex (anchored with \\A and \\z) |
| `deniedCommands` | array | `[]` | Commands that are blocked. Supports regex. Evaluated before allow rules |
| `autoAllowReadonly` | boolean | `false` | Auto-approve read-only commands (ls, cat, echo, pwd, which, head, tail, find, grep) |
| `denyByDefault` | boolean | `false` | Deny commands not in allowedCommands instead of prompting |

**Note**: Regex does NOT support look-around (look-ahead/look-behind).

## Safety Checks

The tool automatically requires approval for:

- **Multi-line commands** - Commands containing `\n` or `\r`
- **Dangerous patterns** - `<(`, `$(`, backticks, `>`, `&&`, `||`, `&`, `;`, `$`, IFS manipulation
- **Command substitution** - Any form of command execution within commands
- **find with mutations** - `-exec`, `-execdir`, `-delete`, `-ok`, `-okdir`, `-fprint`, `-fls`
- **grep with RCE** - `-P` or `--perl-regexp` flags

## Examples

### Example 1: Check System Information

```json
{
  "command": "uname -a"
}
```

**Expected Output**:
```
Linux hostname 5.15.0 #1 SMP x86_64 GNU/Linux
```

### Example 2: Count Lines of Code

```json
{
  "command": "find src -name '*.rs' | xargs wc -l"
}
```

**Expected Output**:
```
  150 src/main.rs
  200 src/lib.rs
  350 total
```

### Example 3: Check Disk Usage

```json
{
  "command": "df -h ."
}
```

**Expected Output**:
```
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda1       100G   45G   50G  48% /
```

### Example 4: Run Build Command

```json
{
  "command": "cargo build --release",
  "summary": "Build release binary"
}
```

**Expected Output**: Build logs with compilation progress and final binary location.

## Troubleshooting

### Issue: Command Requires Approval Every Time

**Symptom**: Prompted for approval on every execution  
**Cause**: Command not in allowedCommands and autoAllowReadonly is false  
**Solution**: Add command to allowedCommands or enable autoAllowReadonly for read-only commands.

### Issue: Command Denied Unexpectedly

**Symptom**: Command blocked without prompt  
**Cause**: Command matches deniedCommands pattern or denyByDefault is true  
**Solution**: Check deniedCommands list. If denyByDefault is true, add command to allowedCommands.

### Issue: Pipe Commands Always Require Approval

**Symptom**: Commands with `|` always prompt  
**Cause**: Pipes are considered potentially dangerous  
**Solution**: Add full piped command to allowedCommands with regex pattern, or approve when prompted.

### Issue: Multi-line Script Fails

**Symptom**: Multi-line commands always require approval  
**Cause**: Safety feature - multi-line commands are complex  
**Solution**: This is intentional. Approve when prompted or split into separate commands.

### Issue: Output Truncated

**Symptom**: Command output cut off  
**Cause**: Output exceeds MAX_TOOL_RESPONSE_SIZE  
**Solution**: Use output redirection to file, then read file with fs_read.

## Related Features

- [fs_read](fs-read.md) - Read command output from files
- [fs_write](fs-write.md) - Create scripts to execute
- [use_aws](use-aws.md) - AWS CLI integration

## Limitations

- Multi-line commands always require approval (safety feature)
- Output limited by MAX_TOOL_RESPONSE_SIZE
- No interactive command support (commands requiring user input will hang)
- No real-time output streaming (output returned after completion)
- Commands run in user's shell environment
- No timeout configuration (commands can run indefinitely)
- Regex patterns don't support look-around assertions

## Technical Details

**Aliases**: `execute_bash`, `execute_cmd`, `shell`

**Platform**: Works on Unix/Linux/macOS (bash) and Windows (cmd/PowerShell)

**Environment**: Commands execute in user's current shell with environment variables. Working directory is current directory.

**Read-only Commands**: ls, cat, echo, pwd, which, head, tail, find (without mutation flags), grep (without -P), dir, type

**Permissions**: Prompts by default unless command in allowedCommands or is read-only with autoAllowReadonly enabled. Deny rules evaluated before allow rules.

**Exit Codes**: Captured and included in output. Non-zero exit codes don't cause tool failure.

**Output Handling**: stdout and stderr captured separately, then combined in output. Unicode tags sanitized for safety.
