---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: delegate
  description: Launch and manage asynchronous agent processes running independently in background
  keywords: [delegate, async, background, agent, task]
  related: [use-subagent, slash-agent]
---

# delegate

Launch and manage asynchronous agent processes running independently in background.

## Overview

The delegate tool launches agents that run asynchronously in the background (non-blocking). Unlike use_subagent which runs synchronously, delegate allows main agent to continue while background agents work. Only one task per agent at a time. Files stored in `.kiro/.subagents/`.

## How It Works

Delegate spawns an agent process that runs independently. Main agent continues immediately without waiting. Check status later to see progress or results. Each agent can only run one task at a time - launching new task for same agent replaces previous task.

## Usage

### Basic Usage

```json
{
  "operation": "launch",
  "task": "Create a snake game"
}
```

### Common Use Cases

#### Use Case 1: Launch Background Task

```json
{
  "operation": "launch",
  "agent": "rust-agent",
  "task": "Create a snake game in Rust"
}
```

**What this does**: Starts rust-agent in background to work on task. Main agent continues immediately.

#### Use Case 2: Check Status

```json
{
  "operation": "status",
  "agent": "rust-agent"
}
```

**What this does**: Returns current status and full output if completed.

#### Use Case 3: Check All Agents

```json
{
  "operation": "status"
}
```

**What this does**: Returns status of all running/completed agents.

#### Use Case 4: List Available Agents

```json
{
  "operation": "list"
}
```

**What this does**: Shows all available agents for delegation.

## Configuration

Enable delegate feature:

```bash
kiro-cli settings chat.enableDelegate true
```

No agent configuration needed - delegate is trusted by default.

## Operations

### launch

Start new task with agent.

**Parameters**:
- `task` (string, required): Task description
- `agent` (string, optional): Agent name (default: "q_cli_default")

**Behavior**: Launches agent in background. Only one task per agent - new launch replaces previous.

### status

Check agent status.

**Parameters**:
- `agent` (string, optional): Agent name (default: "all" - shows all agents)

**Returns**: Status and full output if completed.

### list

Show available agents.

**Parameters**: None

**Returns**: List of all available agents.

## Examples

### Example 1: Launch Long-Running Task

```json
{
  "operation": "launch",
  "agent": "research-agent",
  "task": "Research and summarize the top 10 JavaScript frameworks"
}
```

### Example 2: Check Specific Agent

```json
{
  "operation": "status",
  "agent": "research-agent"
}
```

**Expected Output**:
```json
{
  "agent": "research-agent",
  "status": "completed",
  "output": "Research summary: ..."
}
```

### Example 3: Monitor All Agents

```json
{
  "operation": "status"
}
```

**Expected Output**:
```json
{
  "agents": [
    {"agent": "rust-agent", "status": "running"},
    {"agent": "research-agent", "status": "completed"}
  ]
}
```

## Troubleshooting

### Issue: "Delegate tool is experimental and not enabled"

**Symptom**: Tool returns error message  
**Cause**: Delegate feature not enabled  
**Solution**: Enable with `kiro-cli settings chat.enableDelegate true`

### Issue: Previous Task Replaced

**Symptom**: Can't find output from previous task  
**Cause**: Launching new task for same agent replaces previous  
**Solution**: Use different agent names for concurrent tasks, or check status before launching new task.

### Issue: Agent Not Found

**Symptom**: Error about agent not existing  
**Cause**: Specified agent doesn't exist  
**Solution**: Use `"operation": "list"` to see available agents.

### Issue: No Output from Completed Agent

**Symptom**: Status shows completed but no output  
**Cause**: Agent may have failed or produced no output  
**Solution**: Check agent logs in `.kiro/.subagents/` directory.

## Related Features

- [use_subagent](use-subagent.md) - Synchronous parallel agent execution
- [/agent](../slash-commands/agent-switch.md) - Switch agents in main conversation
- [Agent Configuration](../agent-config/overview.md) - Create specialized agents

## Limitations

- Only one task per agent at a time
- No real-time progress updates (must check status)
- Launching new task replaces previous task for same agent
- No inter-agent communication
- Files stored in `.kiro/.subagents/` directory
- Experimental feature - may change or be removed

## Technical Details

**Aliases**: `delegate`

**Execution**: Asynchronous (non-blocking) - main agent continues immediately.

**Storage**: Agent files stored in `.kiro/.subagents/` in current workspace.

**Agent Selection**: If agent not specified, uses "q_cli_default". If specified agent not found, returns error.

**Permissions**: Trusted by default. Requires `chat.enableDelegate` setting enabled.

**Important Note**: If specific agent requested but not found, DO NOT automatically retry with default agent. Report error and available agents to user.

**Difference from use_subagent**:
- delegate: Async (non-blocking), background execution
- use_subagent: Sync (blocking), parallel execution with real-time status
