---
description: Update autodocs documentation for code changes
---

# Generate Autodocs

You are tasked with updating documentation by invoking the docs agent.

## Process

Run the docs agent to analyze code changes and update documentation:

```bash
kiro-cli chat --agent docs --no-interactive "Update documentation for my code changes"
```

The docs agent will:
- Analyze git diffs to find user-facing changes
- Create or update markdown files in `autodocs/docs/`
- Follow documentation standards from `autodocs/README.md`

## When to use

- After making code changes that affect user-facing features
- Before a release to ensure docs are current
- When adding new tools, commands, or settings
