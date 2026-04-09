---
doc_meta:
  title: feedback
  description: Submit feedback, request features, or report issues
  category: slash_command
  keywords: [feedback, issue, bug, feature, request, report]
  related: [help]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/feedback` command lets you submit feedback, request features, or report issues directly from the chat interface.

## Usage

```
/feedback
```

Opens a selection menu with feedback type options.

## Examples

### Submit feedback

```
/feedback
```

Select from the available feedback categories and provide your input.

## Troubleshooting

### Selection menu doesn't appear

Ensure you're running in interactive mode. The feedback command requires a TUI selection interface.

## Related

- [/help](help.md) — Show all available commands
