---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: feature
  title: Shell Escape
  description: Run shell commands directly from chat using !command syntax
  keywords: [shell, escape, command, terminal, bash, interactive, pty]
  related: [chat, execute-bash]
---

# Shell Escape

Run shell commands directly from chat using `!command` syntax.

## Overview

Shell escape lets you run shell commands without leaving the chat session. Type `!` followed by any command to execute it directly. Output appears in the conversation, and you return to the chat prompt when the command finishes.

This is useful for quick checks (git status, ls, pwd) or running interactive programs (mwinit, ssh) without switching terminals.

## Usage

```
!<command>
```

Type `!` at the start of your input, followed by the command to run.

## Examples

### Example 1: Quick Command

```
!git status
```

**Output**:
```
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

### Example 2: Interactive Program

```
!read -p "Name: " name && echo "Hello $name"
```

**Output**:
```
Name: Kiro
Hello Kiro
```

The command prompts for input, you type your response, and it continues.

### Example 3: Full-Screen Programs

```
!vim file.txt
```

Opens vim in the terminal. When you exit vim, you return to the chat.

### Example 4: Cancel Running Command

Start a long-running command:
```
!sleep 30
```

Press `Ctrl+C` to cancel. You return to the chat prompt without exiting Kiro.

## How It Works

Shell escape runs commands in two modes:

**Streaming mode** (most commands): Output streams into the conversation as it's produced. Uses a PTY (pseudo-terminal) so interactive programs can prompt for input.

**Full-screen mode** (vim, top, ssh, etc.): Switches to alternate screen buffer for programs that need full terminal control. Returns to chat when the program exits.

Full-screen mode is used for: vim, vi, nvim, nano, emacs, less, more, most, top, htop, btop, tmux, screen, ssh.

## Troubleshooting

### Issue: Command Not Receiving Input

**Symptom**: Interactive program doesn't respond to keyboard input  
**Cause**: Older versions didn't forward input to the PTY  
**Solution**: Update to latest version. Interactive input is now supported.

### Issue: Ctrl+C Exits Kiro

**Symptom**: Pressing Ctrl+C during shell escape exits Kiro entirely  
**Cause**: Older versions didn't isolate the child process  
**Solution**: Update to latest version. Ctrl+C now cancels only the running command.

### Issue: Program Output Garbled

**Symptom**: Full-screen program leaves artifacts on screen  
**Cause**: Program not in TTY_COMMANDS list  
**Solution**: The program may need full-screen mode. Report as issue if common program.

## Related

- [kiro-cli chat](../commands/chat.md) - Main chat interface
- [execute_bash](../tools/execute-bash.md) - Tool for AI to run commands

## Limitations

- Windows: Falls back to piped stdio (no PTY support)
- Cannot run background jobs (`!command &` won't work as expected)
- No job control (Ctrl+Z suspends Kiro, not the child)

## Technical Details

**PTY Support**: On Unix, commands run in a pseudo-terminal so they see a real TTY. This enables programs like `mwinit`, `passwd`, and `ssh` to prompt for passwords.

**Signal Handling**: Ctrl+C sends SIGINT to the child process only, not to Kiro. This lets you cancel commands without exiting.

**Terminal State**: Kiro saves and restores terminal state (raw mode, bracketed paste, cursor visibility) around shell escape commands.

**Exit Codes**: Non-zero exit codes are shown in the output: `[exit code: 1]`
