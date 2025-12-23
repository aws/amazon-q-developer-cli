---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /experiment
  description: Toggle experimental features like tangent mode, thinking, knowledge, and checkpoints
  keywords: [experiment, experimental, features, toggle, beta]
  related: [experiments, tangent-mode, enable-thinking]
---

# /experiment

Toggle experimental features like tangent mode, thinking, knowledge, and checkpoints.

## Overview

The `/experiment` command shows interactive picker to enable/disable experimental features. Features include tangent mode, thinking tool, knowledge base, todo lists, checkpoints, context usage indicator, and delegate tool.

## Usage

```
/experiment
```

Shows picker with all experiments and current status.

## Available Experiments

- **Knowledge**: Persistent context storage (/knowledge)
- **Thinking**: Extended reasoning tool
- **Tangent Mode**: Conversation checkpoints
- **Todo Lists**: Task tracking
- **Checkpoint**: Workspace checkpoints
- **Context Usage Indicator**: Show context % in prompt
- **Delegate**: Background agent execution

## Examples

### Example 1: Enable Feature

```
/experiment
```

**Output**:
```
Select experiment:
  [OFF] Knowledge - Persistent context storage
  [OFF] Thinking - Extended reasoning
  [ON]  Tangent Mode - Conversation checkpoints
  [OFF] Todo Lists - Task tracking
```

Select feature to toggle.

## Alternative: Settings Command

```bash
kiro-cli settings chat.enableTangentMode true
kiro-cli settings chat.enableThinking true
kiro-cli settings chat.enableKnowledge true
```

## Related

- [Experiments](../features/experiments.md) - Complete guide
- [Tangent Mode](../features/tangent-mode.md) - Tangent mode details
- [Settings](../commands/settings.md) - Alternative configuration method

## Limitations

- Interactive picker only (not in headless mode)
- Changes apply immediately
- Some features require restart

## Technical Details

**Status Indicators**: [ON] green, [OFF] grey

**Persistence**: Changes saved to settings database.

**Commands**: Each experiment lists associated commands.
