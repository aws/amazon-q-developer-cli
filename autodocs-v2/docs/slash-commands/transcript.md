---
doc_meta:
  title: /transcript
  description: View or save the full conversation transcript in multiple formats
  category: slash_command
  keywords: [transcript, conversation, pager, review, history, less, save, export, plaintext, json, markdown]
  related: [copy, chat-save]
  validated: 2026-05-27
  commit: 1e0274bf4
  status: validated
  testable_headless: false
---

## Overview

The `/transcript` command opens the full conversation transcript in your system pager (`$PAGER`, defaults to `less`) or saves it to a file. Supports markdown, plaintext, and JSON output formats.

Press `q` to quit the pager and return to the chat.

## Usage

```
/transcript [save] [--plain|--json] [<filepath>]
```

| Argument | Description |
|----------|-------------|
| `save` | Save to a file instead of opening in pager |
| `--plain` | Output as plaintext (strips markdown formatting) |
| `--json` | Output as JSON (array of role/content objects) |
| `<filepath>` | File path for saving (supports `~/` expansion and spaces in paths) |

Without `save` or a filepath, the transcript opens in `$PAGER`. When using `less`, the pager starts at the bottom so the most recent messages are visible first.

Providing a filepath implicitly triggers saving (the `save` keyword is optional when a path is given).

The default format is markdown.

## Examples

### View transcript in pager

```
/transcript
```

Opens the full conversation in your pager with the most recent messages visible. Use standard pager controls:
- `q` — quit
- `/` — search forward
- `n` — next match
- `g` — go to top
- `G` — go to bottom

### View as plaintext

```
/transcript --plain
```

Opens the transcript in the pager with all markdown formatting stripped (no code fences, heading markers, bold/italic, or link syntax).

### View as JSON

```
/transcript --json
```

Opens the transcript as a JSON array of `{role, content}` objects.

### Save transcript to file

```
/transcript save
```

Saves the transcript as `transcript.md` in the current directory.

### Save as plaintext to a specific path

```
/transcript save --plain ~/notes/session.txt
```

Saves a plaintext version to `~/notes/session.txt`.

### Save as JSON

```
/transcript save --json ./debug-log.json
```

Saves the conversation as JSON to the specified path.

### Save with just a path (no save keyword)

```
/transcript ~/conversation.md
```

Providing a filepath automatically saves instead of opening the pager.

## Output Formats

| Format | Extension | Description |
|--------|-----------|-------------|
| markdown (default) | `.md` | Full markdown with role headers and formatting preserved |
| `--plain` | `.txt` | Stripped of markdown syntax; uses `User:` and `Kiro:` labels |
| `--json` | `.json` | Array of `{role, content}` objects, pretty-printed |

## Troubleshooting

### "No conversation to display"

No messages in the current session yet. Start a conversation first.

### "Failed to save: ..."

The target path is not writable. Check that the directory exists and you have write permissions.

### Pager doesn't start at bottom

The `+G` flag is only added when `$PAGER` starts with `less`. Other pagers will start at the top.

## Related

- [/copy](copy.md) — Copy just the last response to clipboard
- [/chat save](chat-save.md) — Save the full session for later loading
