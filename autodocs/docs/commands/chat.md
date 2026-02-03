---
doc_meta:
  validated: 2026-02-02
  commit: 2cfa80d8
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli chat
  description: Start AI assistant session with support for agents, models, tool trust, and conversation management
  keywords: [chat, conversation, agent, model, interactive, headless, mcp, require-mcp-startup, log, logging, history]
  related: [slash-chat-save, slash-chat-load, slash-agent, exit-codes]
---

# kiro-cli chat

Start AI assistant session with support for agents, models, tool trust, and conversation management.

## Overview

The chat command launches an interactive AI assistant session. Supports agent selection, model choice, tool trust configuration, conversation resumption, and headless mode for automation. Primary interface for interacting with Kiro CLI.

## Usage

### Basic Usage

```bash
kiro-cli chat
```

### Common Use Cases

#### Use Case 1: Start Interactive Session

```bash
kiro-cli chat
```

**What this does**: Launches interactive chat session with default agent and model.

#### Use Case 2: Start with Specific Agent

```bash
kiro-cli chat --agent rust-expert
```

**What this does**: Starts session using rust-expert agent configuration.

#### Use Case 3: Headless Mode with Query

```bash
kiro-cli chat --no-interactive "List all Rust files in src/"
```

**What this does**: Executes single query non-interactively and exits.

#### Use Case 4: Trust All Tools

```bash
kiro-cli chat --trust-all-tools "Run tests and analyze results"
```

**What this does**: Allows agent to use any tool without approval prompts.

#### Use Case 5: Trust Specific Tools

```bash
kiro-cli chat --trust-tools=fs_read,grep "Find all TODOs"
```

**What this does**: Auto-approves only fs_read and grep tools.

#### Use Case 6: Resume Last Conversation

```bash
kiro-cli chat --resume
```

**What this does**: Resumes most recent conversation from current directory.

#### Use Case 7: Select Conversation to Resume

```bash
kiro-cli chat --resume-picker
```

**What this does**: Shows interactive picker to select conversation to resume.

## Options

| Option | Short | Type | Description |
|--------|-------|------|-------------|
| `--resume` | `-r` | flag | Resume most recent conversation |
| `--resume-picker` | | flag | Interactively select conversation to resume |
| `--agent` | | string | Agent to use (default: default agent) |
| `--model` | | string | Model to use (default: default model) |
| `--trust-all-tools` | `-a` | flag | Auto-approve all tool uses |
| `--trust-tools` | | list | Auto-approve specific tools (comma-separated) |
| `--no-interactive` | | flag | Run without user input (headless mode) |
| `--list-sessions` | `-l` | flag | List saved conversations |
| `--delete-session` | `-d` | string | Delete conversation by ID |
| `--wrap` | `-w` | enum | Line wrapping (always/never/auto) |
| `--require-mcp-startup` | | flag | Exit with code 3 if any MCP server fails to start |
| `--verbose` | `-v` | flag | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | flag | Print help information |
| `[INPUT]` | | string | Initial query to send |

## Examples

### Example 1: Quick Query

```bash
kiro-cli chat "What files are in the current directory?"
```

### Example 2: Automated Script

```bash
kiro-cli chat --no-interactive --trust-all-tools "Run cargo test and summarize results"
```

### Example 3: Specific Model

```bash
kiro-cli chat --model <model-id> "Explain this codebase"
```

### Example 4: List Conversations

```bash
kiro-cli chat --list-sessions
```

**Expected Output**:
```
Chat sessions for /path/to/project:

Chat SessionId: f2946a26-3735-4b08-8d05-c928010302d5
  2 hours ago | Implement user authentication | 15 msgs

Chat SessionId: 7bd2c90f-7080-4981-87f7-206d9147878f
  1 days ago | Refactor database layer | 23 msgs

To delete a session, use: kiro-cli chat --delete-session <SESSION_ID>
```

### Example 5: Delete Conversation

```bash
kiro-cli chat --delete-session abc123
```

**Expected Output**:
```
✔ Deleted chat session abc123
```

### Example 6: Require MCP Servers

```bash
kiro-cli chat --require-mcp-startup --no-interactive "Run analysis"
```

**What this does**: Exits with code 3 if any configured MCP server fails to start. Useful for CI/CD pipelines.

## Headless Mode

Use `--no-interactive` for automation and scripts:

**Requirements**:
- Must provide initial query as argument
- Use `--trust-all-tools` or `--trust-tools` to avoid hanging on tool approvals
- Interactive slash commands won't work (/experiment, /model picker, /agent picker)
- No mid-session user input possible

**What Works**:
- Natural language queries
- Tool invocations (with trust flags)
- Informational slash commands (/help, /tools)
- Settings via `kiro-cli settings` command

**What Doesn't Work**:
- Interactive slash commands requiring selection
- Tool approval prompts (will hang)
- Mid-session confirmations
- Terminal UI features

## Troubleshooting

### Issue: Tool Approval Hangs in Headless Mode

**Symptom**: Command hangs waiting for input  
**Cause**: Tool requires approval but no interactive input available  
**Solution**: Use `--trust-all-tools` or `--trust-tools=tool1,tool2`

### Issue: Can't Resume Conversation

**Symptom**: --resume shows no conversations  
**Cause**: No saved conversations in current directory  
**Solution**: Conversations are directory-specific. Use `/chat save` to save conversations.

### Issue: Agent Not Found

**Symptom**: Error about agent not existing  
**Cause**: Specified agent doesn't exist  
**Solution**: Check agent name. Use `kiro-cli agent list` to see available agents.

### Issue: Interactive Command Fails in Headless

**Symptom**: Error "not a terminal"  
**Cause**: Interactive slash command used in headless mode  
**Solution**: Use direct CLI commands instead (e.g., `kiro-cli settings` instead of `/experiment`)

### Issue: MCP Server Startup Failure

**Symptom**: Exit code 3 with `--require-mcp-startup`  
**Cause**: One or more MCP servers failed to start  
**Solution**: Check MCP server configuration. Verify server paths and dependencies are correct.

## Related Features

- [/chat save](../slash-commands/chat-save.md) - Save conversations
- [/chat load](../slash-commands/chat-load.md) - Load conversations
- [/agent](../slash-commands/agent-switch.md) - Switch agents mid-session
- [kiro-cli agent](agent.md) - Manage agent configurations
- [kiro-cli settings](settings.md) - Configure behavior

## Limitations

- Conversations saved per-directory
- Headless mode doesn't support interactive commands
- Tool trust applies to entire session
- Can't change trust settings mid-session
- Resume only works for conversations in current directory

## Technical Details

**Aliases**: `chat` (default command if no subcommand specified)

**Conversation Storage**: Saved in database, scoped to current directory path.

**Agent Resolution**: Local agents (`.kiro/agents/`) take precedence over global (`~/.kiro/agents/`).

**Model Selection**: Uses specified model or default from settings. Can be changed mid-session with `/model`.

**Tool Trust**: 
- `--trust-all-tools`: Bypasses all tool approval prompts
- `--trust-tools=`: Empty list trusts no tools
- `--trust-tools=a,b`: Trusts only tools a and b
- Without flags: Prompts for each tool use (unless in agent's allowedTools)

**Headless Mode**: Sets non-interactive flag, disables terminal UI, requires initial query.

**Line Wrapping**: Auto-detects terminal width. Override with `--wrap always` or `--wrap never`.

**Keyboard Shortcuts**:
- `Ctrl+R`: Search command history (case-insensitive)
- `Ctrl+C`: Cancel current operation or exit
- `Ctrl+T`: Toggle tangent mode (if enabled)
- `Up/Down`: Navigate command history

**Logging**: Chat sessions write logs to platform-specific locations:
- **macOS**: `$TMPDIR/kiro-log/kiro-chat.log`
- **Linux**: `$XDG_RUNTIME_DIR/kiro-log/kiro-chat.log`
- **Windows**: `%TEMP%/kiro-log/logs/kiro-chat.log`

Override with `KIRO_CHAT_LOG_FILE` environment variable:

```bash
KIRO_CHAT_LOG_FILE=/tmp/my-debug.log kiro-cli chat
```
