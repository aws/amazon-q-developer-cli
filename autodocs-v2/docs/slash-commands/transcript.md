---
doc_meta:
  title: /transcript
  description: Open the full conversation transcript in $PAGER for review
  category: slash_command
  keywords: [transcript, conversation, pager, review, history, less]
  related: [copy]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/transcript` command opens the full conversation transcript in your system pager (`$PAGER`, defaults to `less`). Useful for reviewing long conversations, searching through history, or reading tool outputs that were collapsed.

Press `q` to quit the pager and return to the chat.

## Usage

```
/transcript
```

## Examples

### Review conversation

```
/transcript
```

Opens the full conversation in your pager. Use standard pager controls:
- `q` — quit
- `/` — search forward
- `n` — next match
- `g` — go to top
- `G` — go to bottom

## Troubleshooting

### "No conversation to display"

No messages in the current session yet.

## Related

- [/copy](copy.md) — Copy just the last response to clipboard
