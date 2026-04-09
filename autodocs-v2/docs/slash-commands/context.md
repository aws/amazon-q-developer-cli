---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
  category: slash_command
  title: /context
  description: View context window usage and manage context files with add, remove, show, and clear operations
  keywords: [context, files, usage, tokens, window, manage, percentage, skill]
  related: [agent-config, hooks]
---

# /context

View context window usage and manage context files with add, remove, show, and clear operations.

## Overview

The `/context` command manages context files and displays context window token usage. Without subcommands, shows detailed breakdown of token usage by component (context files, tools, messages) including the backend-reported context percentage. With subcommands, add/remove context file rules.

## Usage

### View Context Usage

```
/context
```

Shows token usage breakdown.

### Manage Context Files

```
/context show
/context add <paths>
/context remove <paths>
/context clear
```

## Subcommands

### (no subcommand)

Display context window token usage breakdown.

```
/context
```

Shows:
- Context files usage
- Tools usage
- Kiro responses usage
- Your prompts usage
- Session files usage
- Total usage and percentage

### show

Display context rules and matched files with expanded details.

```
/context show
```

Shows:
- Agent context files (permanent)
- Session context files (temporary)
- Matched files with token counts
- Files dropped due to size limits

### add

Add context file rules (paths or glob patterns).

```
/context add <paths...>
```

**With --force or -f**:
```
/context add --force <paths...>
/context add -f <paths...>
```

Adds even if files exceed size limits or don't exist.

**Note**: Changes are temporary (session-only). For permanent context, edit agent configuration.

### remove

Remove context file rules.

```
/context remove <paths...>
```

**Alias**: `/context rm`

### clear

Remove all context rules.

```
/context clear
```

Clears all session context files.

## Examples

### Example 1: View Usage

```
/context
```

**Output**:
```
Context breakdown - 75% used
```

The UI displays a breakdown with categories:
- `context_files` - Agent context files
- `tools` - Tool definitions
- `kiro_responses` - Assistant responses
- `your_prompts` - User prompts
- `session_files` - Temporary session files

### Example 2: Show Context Files

```
/context show
```

**Output**:
```
Agent (rust-expert)
  - src/**/*.rs
      src/main.rs
      src/lib.rs
  - Cargo.toml /Users/me/project/Cargo.toml
  - skill://.kiro/skills/**/SKILL.md
      database-helper

Session (temporary)
  <none>

3 matched files in use
- src/main.rs (2.3% of context window)
- src/lib.rs (1.8% of context window)
- database-helper (0.1% of context window)

Context files total: 4.2% of context window
```

Glob patterns (containing `*`, `?`, or `[`) display the pattern on its own line with matched files indented below. Single file paths show the pattern and resolved path on the same line.

Both regular files and skill resources show their estimated context usage.

### Example 3: Add Context Files

```
/context add README.md docs/**/*.md
```

**Output**:
```
Added 2 path(s) to context
```

For a single file:
```
/context add README.md
```

**Output**:
```
Added 'README.md' to context
```

### Example 4: Remove Context

```
/context remove README.md
```

**Output**:
```
Removed 'README.md' from context
```

Using the alias:
```
/context rm README.md
```

### Example 5: Clear All

```
/context clear
```

**Output**:
```
Cleared all context rules
```

## Context File Types

### Agent Context (Permanent)

Defined in agent configuration. Persists across sessions.

```json
{
  "resources": [
    "src/**/*.rs",
    "Cargo.toml"
  ]
}
```

### Session Context (Temporary)

Added via `/context add` or loaded from saved conversations. Cleared when session ends.

## Size Limits

Context files have size limits to prevent overwhelming context window:
- Files exceeding limit are dropped (oldest first)
- Warning shown when files dropped
- Use `--force` or `-f` to add despite limits

## Troubleshooting

### Issue: Files Not Matching

**Symptom**: `/context show` shows "(no matches)"  
**Cause**: Glob pattern doesn't match any files  
**Solution**: Check pattern syntax. Use `**` for recursive matching.

### Issue: Files Dropped

**Symptom**: Warning about dropped files  
**Cause**: Context files exceed size limit  
**Solution**: Remove unnecessary files or use more specific patterns

### Issue: Changes Not Persisting

**Symptom**: Context changes lost after session  
**Cause**: `/context add/remove` are temporary  
**Solution**: Edit agent configuration for permanent changes

### Issue: Path Not Found Error

**Symptom**: Error "Path not found: <path>"  
**Cause**: File doesn't exist or path contains unquoted spaces  
**Solution**: Wrap paths with spaces in quotes, or use `--force` to add anyway

### Issue: None of the Paths Found

**Symptom**: Error "None of the specified paths were found in the context"  
**Cause**: Trying to remove paths that aren't in the context  
**Solution**: Use `/context show` to see current context paths

## Related Features

- [Agent Configuration](../features/agent-configuration.md) - Permanent context configuration
- [Hooks](../features/hooks.md) - Context hooks for dynamic content
- [/agent](agent-swap.md) - Switch agents with different context

## Limitations

- `/context add/remove/clear` are temporary (session-only)
- Context files loaded from disk (not embedded)
- Size limits enforced to prevent context overflow
- Glob patterns use gitignore syntax
- Changes don't affect agent configuration file

## Technical Details

**Token Counting**: Uses approximate token counter (characters / 4).

**Context Window**: Size varies by model (e.g., 200K for Claude 3.5 Sonnet).

**File Matching**: Uses glob patterns with gitignore syntax. `**` matches recursively.

**Size Limits**: Calculated as percentage of context window. Oldest files dropped first when limit exceeded.

**Agent vs Session**: Agent context from configuration (permanent). Session context from `/context add` or loaded conversations (temporary).

**Skill Resources**: `skill://` resources show their estimated size based on the metadata sent to the model (name, description, filepath). Full skill content is loaded on demand.

**Hooks**: Context can include dynamic content via hooks. See agent configuration documentation.
