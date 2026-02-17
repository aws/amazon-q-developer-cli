---
doc_meta:
  validated: 2026-02-16
  commit: 36e7dac5
  status: redirect
  testable_headless: false
  category: slash_command
  title: /agent generate
  description: Alias for /agent create - Create agent with AI assistance
  keywords: [agent, generate, create, ai]
  related: [agent-create, agent-swap, cmd-agent]
---

# /agent generate

**This command is an alias for [`/agent create`](agent-create.md).**

## Overview

The `/agent generate` command is now an alias for `/agent create`. Both commands behave identically.

See [/agent create](agent-create.md) for full documentation.

## Quick Reference

```
/agent generate [NAME] [OPTIONS]
```

Is equivalent to:

```
/agent create [NAME] [OPTIONS]
```

## Migration Note

If you were previously using `/agent generate`, no changes are needed. The command continues to work exactly as before. You can also use `/agent create` which now provides the same AI-assisted agent creation functionality by default.

For the simple editor-based creation that `/agent create` previously provided, use:

```
/agent create <NAME> --manual
```
