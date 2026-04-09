---
doc_meta:
  title: /guide
  description: Switch to the guide agent for help with Kiro CLI features and commands
  category: slash_command
  keywords: [guide, help, agent, documentation, questions, kiro_guide]
  related: [help, agent-swap, introspect]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/guide` command switches to the built-in `kiro_guide` agent, which can answer questions about Kiro CLI features, commands, tools, and configuration. It uses the `introspect` tool to search documentation.

Running `/guide` again while already on the guide agent toggles back to your previous agent.

## Usage

```
/guide
```

Switch to the guide agent.

```
/guide [question]
```

Switch to the guide agent and immediately ask a question.

## Examples

### Switch to guide agent

```
/guide
```

### Ask a question directly

```
/guide How do I save a conversation?
```

### Return to previous agent

While on the guide agent:

```
/guide
```

Toggles back to the agent you were using before.

### Ask another question while on guide

```
/guide What tools are available?
```

When already on the guide agent, forwards the question without switching.

## Troubleshooting

### "Guide agent not found"

The `kiro_guide` built-in agent is missing. This shouldn't happen in a normal installation — try updating Kiro CLI.

### Guide agent doesn't know about a feature

The guide agent searches embedded documentation via the introspect tool. Very new features may not be documented yet.

## Related

- [/help](help.md) — Show all available slash commands
- [/agent](agent-swap.md) — Switch to any agent
- [introspect](../tools/introspect.md) — Documentation search tool used by the guide agent
