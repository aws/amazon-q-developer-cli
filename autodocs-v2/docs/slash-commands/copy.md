---
doc_meta:
  title: /copy
  description: Copy the last assistant response to the system clipboard
  category: slash_command
  keywords: [copy, clipboard, response, output]
  related: [transcript]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/copy` command copies the last assistant response to your system clipboard. If the response spans multiple messages (e.g., interleaved with tool calls), all parts are concatenated.

For the full conversation, use `/transcript` instead.

## Usage

```
/copy
```

## Examples

### Copy last response

```
/copy
```

Copies the assistant's most recent response to clipboard.

## Troubleshooting

### "No assistant response to copy"

No assistant messages in the conversation yet. Ask a question first.

### Clipboard not working

Clipboard access depends on your terminal and OS. SSH sessions may not support clipboard operations.

## Related

- [/transcript](transcript.md) — View full conversation transcript in $PAGER
