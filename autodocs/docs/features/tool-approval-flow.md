---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: feature
  title: Tool Approval Flow
  description: Interactive approval prompts for tool execution with trust options and keyboard shortcuts
  keywords: [approval, permission, trust, allow, deny, prompt, tool, execute, confirm]
  related: [tools, execute-bash, fs-write, agent-configuration]
---

# Tool Approval Flow

Interactive approval prompts for tool execution with trust options and keyboard shortcuts.

## Overview

When a tool requires permission, Kiro displays an approval panel with options to allow or deny the action. Some tools support granular trust options that let you approve specific patterns (like command prefixes) rather than trusting all uses of the tool.

## How It Works

1. Tool requests permission (e.g., execute_bash wants to run a command)
2. Approval panel appears with the tool name and action
3. You choose an option using keyboard shortcuts or arrow keys
4. For "Trust" option, a sub-menu may appear with pattern-based choices
5. Your choice is applied and the tool proceeds (or is blocked)

## Approval Options

| Option | Shortcut | Description |
|--------|----------|-------------|
| Yes, single permission | `y` | Allow this one request |
| Trust | (enter) | Open trust options or always allow |
| No, single rejection | `n` | Deny this one request |
| Never | - | Always deny this tool |

### Trust Options

When you select "Trust", some tools show a sub-menu with granular trust options:

**Example for execute_bash:**
```
execute_bash requires approval · trust options

> Full command     df -h
  Base command     df *
```

- **Full command**: Trust the exact command (e.g., `df -h`)
- **Base command**: Trust the command prefix with any arguments (e.g., `df *`)

Press `Esc` to go back to the main approval menu.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `y` | Allow once |
| `n` | Deny once |
| `t` | Open trust options (when available) |
| `↑`/`↓` | Navigate options |
| `Enter` | Select highlighted option |
| `Esc` | Cancel/go back |
| `Tab` | Edit (for allow/deny with modifications) |

**Note**: The `t` shortcut opens the trust options page when pattern-based trust is available. If no trust options exist, `t` directly trusts the tool for all future requests.

## Examples

### Example 1: Allow Single Command

```
execute_bash requires approval

> Yes, single permission
  Trust
  No, single rejection
  Never
```

Press `y` to allow this one execution.

### Example 2: Trust with Pattern Options

```
execute_bash requires approval

> Yes, single permission
  Trust                    ← Enter to see more options
  No, single rejection
  Never
```

Select "Trust" and press Enter:

```
execute_bash requires approval · trust options

> Full command     git status
  Base command     git *
```

Select "Base command" to trust all `git` commands.

### Example 3: Crew Approval (Subagents)

When subagents request approval, you see a consolidated prompt:

```
3 subagent(s) waiting for tool approval

> (a) Approve all pending
  (t) Trust
  (c) Configure individually (agent monitor)
  (x) Exit (cancel subagents)
```

Press `t` to see trust options for batch approval.

## Configuration

### Agent-Level Trust

Configure tools that never require approval in agent configuration:

```json
{
  "allowedTools": ["fs_read", "grep", "glob"]
}
```

### Tool-Specific Settings

Configure allowed patterns in `toolsSettings`:

```json
{
  "toolsSettings": {
    "execute_bash": {
      "allowedCommands": ["git status", "cargo check"],
      "autoAllowReadonly": true
    }
  }
}
```

### Session-Level Trust

Use `/tools trust <tool>` to trust tools for the current session only.

## Troubleshooting

### Issue: No Trust Options Shown

**Symptom**: "Trust" directly approves without showing pattern options  
**Cause**: Tool doesn't support granular trust patterns  
**Solution**: This is expected. Only some tools (like execute_bash) support pattern-based trust.

### Issue: Trust Not Persisting

**Symptom**: Same approval prompt appears again  
**Cause**: Trust options are session-only unless configured in agent  
**Solution**: Add patterns to agent's `toolsSettings` for permanent trust.

### Issue: Can't Cancel Approval

**Symptom**: Escape key doesn't work  
**Cause**: May be in trust options sub-menu  
**Solution**: Press Esc to go back to main menu, then Esc again to cancel.

## Related Features

- [/tools](../slash-commands/tools.md) - Manage tool permissions
- [execute_bash](../tools/execute-bash.md) - Command execution with patterns
- [Agent Configuration](agent-configuration.md) - Permanent tool settings

## Limitations

- Trust patterns are tool-specific (not all tools support them)
- Session trust doesn't persist across sessions
- Pattern matching uses regex (no look-around support)
- Crew approval batches all pending requests together

## Technical Details

**Trust Options**: Provided by the backend based on the tool and action. For execute_bash, options include full command match and base command prefix patterns.

**Pattern Storage**: When you select a trust option, the pattern is added to the tool's allowed list for the session. Patterns use regex matching.

**Approval Queue**: Multiple approval requests queue up. Crew mode shows consolidated approval for subagent requests.
