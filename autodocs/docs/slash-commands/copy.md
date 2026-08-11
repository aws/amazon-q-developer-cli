---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: slash_command
  title: /copy
  description: Copy the last assistant response to the system clipboard
  keywords: [copy, clipboard, response, paste, export]
  related: [clear, compact]
---

# /copy

Copy the last assistant response to the system clipboard.

## Overview

The `/copy` command copies the most recent assistant response to your system clipboard. This lets you quickly paste the response into other applications without manually selecting text.

## Usage

```
/copy
```

No arguments or confirmation required.

## Examples

### Example 1: Copy After Getting a Response

```
You: Explain what a closure is in JavaScript

Kiro: A closure is a function that has access to variables from its outer
      (enclosing) scope, even after the outer function has returned...

/copy
```

**Output**:
```
✔ Copied to clipboard
```

You can now paste the explanation into any application.

### Example 2: No Response Available

```
/copy
```

**Output** (when no assistant response exists):
```
✖ No response to copy
```

### Example 3: Clipboard Tool Not Found

```
/copy
```

**Output** (when no clipboard utility is available):
```
✖ Failed to copy — no clipboard tool found
```

## Troubleshooting

### "No response to copy"

The command only copies assistant responses. Ensure you have received at least one response in the current session before using `/copy`.

### "Failed to copy — no clipboard tool found"

The command requires a system clipboard utility:

- **macOS**: `pbcopy` (built-in)
- **Windows**: PowerShell (built-in)
- **Linux (Wayland)**: Install `wl-copy` (part of `wl-clipboard`)
- **Linux (X11)**: Install `xclip` or `xsel`

Install the appropriate tool for your system:

```bash
# Debian/Ubuntu (X11)
sudo apt install xclip

# Debian/Ubuntu (Wayland)
sudo apt install wl-clipboard

# Fedora
sudo dnf install xclip   # or wl-clipboard for Wayland
```

## Related

- [/clear](clear.md) - Clear conversation history
- [/compact](compact.md) - Summarize and compact conversation

## Limitations

- Only copies the last assistant response (not user messages or earlier responses)
- Requires a clipboard utility on Linux
- Multi-line responses are copied with original formatting

## Technical Details

**Platform Support**:
- macOS: Uses `pbcopy`
- Windows: Uses PowerShell `Set-Clipboard`
- Linux: Tries `wl-copy` (Wayland), then `xclip`, then `xsel`

**What's Copied**: The full text content of the most recent assistant message, preserving newlines and formatting.
