---
doc_meta:
  title: Shell Escape
  description: Run shell commands directly from the prompt using !command syntax with full interactive support
  category: feature
  keywords: [shell, escape, bang, command, terminal, interactive, pty, mwinit, ssh, sudo]
  related: [shell, classic-vs-tui]
  validated: 2026-08-12
  commit: 8dd390a57
  status: validated
  testable_headless: true
---

## Overview

Shell escape lets you run shell commands directly from the Kiro prompt by prefixing them with `!`. Unlike the shell tool used by the AI, shell escape runs commands in a pseudo-terminal (PTY) with full interactive support.

Use shell escape when you need to:
- Run commands that prompt for input (passwords, confirmations)
- Use interactive programs like `mwinit`, `ssh`, `sudo`
- See real-time output from long-running commands

## Usage

Type `!` followed by your command at the prompt:

```
!<command>
```

The command runs in your current working directory with your environment variables.

### Interactive Input

Commands can prompt for and receive user input. Type your response and press Enter:

```
!read -p "Name: " name && echo "Hello $name"
Name: Alice
Hello Alice
```

### Canceling Commands

Press `Ctrl+C` to cancel a running shell escape command. This sends SIGINT to the command without exiting Kiro.

```
!sleep 30
^C
(command canceled, returns to prompt)
```

### Full-Screen Programs

Programs that need full terminal control (vim, top, htop, less) automatically use the alternate screen buffer. Your Kiro session is preserved when they exit.

## Examples

### Authentication with mwinit

```
!mwinit -s
Enter your PIN:
```

### SSH with host key confirmation

```
!ssh user@newhost.example.com
The authenticity of host 'newhost.example.com' can't be established.
ED25519 key fingerprint is SHA256:abc123...
Are you sure you want to continue connecting (yes/no)? yes
```

### Sudo commands

```
!sudo apt update
[sudo] password for user:
```

### Git interactive staging

```
!git add -p
Stage this hunk [y,n,q,a,d,e,?]?
```

### Multi-step input

```
!read -p "First: " a && read -p "Second: " b && echo "$a and $b"
First: foo
Second: bar
foo and bar
```

## Troubleshooting

### Command output not appearing

Shell escape streams output in real-time. If you see no output, the command may be waiting for input or running silently. Check if it's prompting for something.

### Ctrl+C not working

Ctrl+C sends SIGINT to the running command. Some programs catch SIGINT and require multiple presses or a different signal. The command will eventually terminate and return you to the prompt.

### Windows limitations

On Windows, shell escape falls back to piped stdio without PTY support. Interactive programs that require a real terminal may not work correctly.

### Full-screen program display issues

If a full-screen program (vim, top) leaves artifacts after exiting, the terminal state should auto-restore. If not, run `!reset` or restart your terminal.

## Related

- [shell](../tools/shell.md) — AI-invoked command execution (non-interactive)
- [Classic vs TUI](classic-vs-tui.md) — Differences between interface modes
