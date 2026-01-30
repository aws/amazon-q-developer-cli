---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: thinking
  description: Internal reasoning tool for complex problem-solving and decision-making
  keywords: [thinking, reasoning, internal, complex]
  related: [enable-thinking]
---

# thinking

Internal reasoning tool for complex problem-solving and decision-making.

## Overview

> **Note**: This tool is used by the AI assistant automatically when enabled. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool for internal reasoning as needed.

The thinking tool allows the AI to reason through complex problems during response generation. Provides dedicated space to process tool results, navigate decision trees, and improve response quality in multi-step scenarios. Experimental feature.

## Configuration

Enable thinking feature:

```bash
kiro-cli settings chat.enableThinking true
```

Or use `/experiment` and select Thinking.

## How It Works

When enabled, AI can use thinking tool to:
- Process complex information
- Plan multi-step approaches
- Reason through decisions
- Analyze tool results

Thoughts are shown to user for transparency.

## Examples

### Example: Complex Task

```
> Refactor this codebase for better performance

[AI uses thinking tool]
I'll share my reasoning process: First, I need to analyze the current structure...
```

## Related

- [chat.enableThinking](../settings/enable-thinking.md) - Enable setting
- [/experiment](../slash-commands/experiment.md) - Toggle experiments

## Limitations

- Experimental feature
- Requires explicit enablement
- Adds to response time
- Thoughts visible to user

## Technical Details

**Aliases**: `thinking`

**Permissions**: Trusted by default. Requires `chat.enableThinking` setting enabled.

**Output**: Thoughts displayed during response generation.
