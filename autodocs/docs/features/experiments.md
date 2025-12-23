---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Experiments
  description: Experimental features including tangent mode, thinking, knowledge, todo lists, checkpoints, and delegate
  keywords: [experiments, experimental, beta, features, toggle]
  related: [slash-experiment]
---

# Experiments

Experimental features including tangent mode, thinking, knowledge, todo lists, checkpoints, and delegate.

## Overview

Kiro CLI includes experimental features that can be enabled/disabled. These features are in beta and may change or be removed in future versions. Enable via `/experiment` command or settings.

## Available Experiments

### Knowledge
Persistent context storage and retrieval across chat sessions.

**Enable**: `kiro-cli settings chat.enableKnowledge true`  
**Commands**: `/knowledge`

**Use Cases**:
- Store project documentation for future reference
- Build searchable knowledge base of code patterns
- Maintain context across multiple chat sessions
- Share knowledge between team members

### Thinking
Extended reasoning tool for complex problem-solving.

**Enable**: `kiro-cli settings chat.enableThinking true`  
**Tool**: `thinking`

**Use Cases**:
- Break down complex architectural decisions
- Work through multi-step debugging processes
- Analyze trade-offs in design choices
- Document reasoning for future reference

### Tangent Mode
Conversation checkpoints for exploring side topics.

**Enable**: `kiro-cli settings chat.enableTangentMode true`  
**Commands**: `/tangent`, Ctrl+T

**Use Cases**:
- Explore side questions without losing main thread
- Test different approaches in parallel
- Keep help/documentation queries separate
- Maintain focus on primary task

### Todo Lists
Task tracking and management for multi-step work.

**Enable**: `kiro-cli settings chat.enableTodoList true`  
**Tool**: `todo_list`, **Commands**: `/todo`

**Use Cases**:
- Track progress on complex projects
- Break down large tasks into manageable steps
- Coordinate work across multiple sessions
- Generate project reports and status updates

### Checkpoint
Workspace snapshots and restoration.

**Enable**: `kiro-cli settings chat.enableCheckpoint true`  
**Commands**: `/checkpoint`

**Use Cases**:
- Save workspace state before risky changes
- Create restore points during development
- Share workspace snapshots with team
- Rollback to known good states

### Context Usage Indicator
Show context window usage percentage in prompt.

**Enable**: `kiro-cli settings chat.enableContextUsageIndicator true`

**Use Cases**:
- Monitor context window usage in long conversations
- Know when to summarize or start fresh
- Optimize prompt efficiency
- Avoid hitting context limits unexpectedly

### Delegate
Background asynchronous agent execution.

**Enable**: `kiro-cli settings chat.enableDelegate true`  
**Tool**: `delegate`

**Use Cases**:
- Run long-running tasks in background
- Parallel processing of multiple subtasks
- Asynchronous code generation and testing
- Multi-agent collaboration workflows

## Enabling Experiments

### Method 1: Interactive

```
/experiment
```

Shows picker with all experiments and current status. Select to toggle.

### Method 2: Settings Command

```bash
kiro-cli settings chat.enableTangentMode true
kiro-cli settings chat.enableThinking true
kiro-cli settings chat.enableKnowledge true
kiro-cli settings chat.enableTodoList true
kiro-cli settings chat.enableCheckpoint true
kiro-cli settings chat.enableContextUsageIndicator true
kiro-cli settings chat.enableDelegate true
```

## Examples

### Example 1: Enable Tangent Mode

```
/experiment
```

Select "Tangent Mode" from list.

### Example 2: Enable Multiple Features

```bash
kiro-cli settings chat.enableTangentMode true
kiro-cli settings chat.enableThinking true
kiro-cli settings chat.enableTodoList true
```

## Troubleshooting

### Issue: Feature Not Working After Enable

**Symptom**: Feature enabled but not working  
**Cause**: May require restart  
**Solution**: Restart chat session

### Issue: Can't Find Experiment

**Symptom**: Feature not in list  
**Cause**: Feature may have been removed or renamed  
**Solution**: Check settings list for current features

## Related Features

- [/experiment](../slash-commands/experiment.md) - Toggle experiments
- [Settings](../commands/settings.md) - Alternative configuration

## Limitations

- Experimental features may change or be removed
- Some features conflict (tangent mode vs checkpoint)
- Features require explicit enablement
- No automatic migration if features change

## Technical Details

**Status**: All experiments are beta/experimental

**Persistence**: Settings saved to database

**Conflicts**: Some features mutually exclusive (tangent mode disables checkpoint)
