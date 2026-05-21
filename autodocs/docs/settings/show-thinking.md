---
doc_meta:
  validated: 2026-05-21
  commit: da73d05a0
  status: validated
  testable_headless: true
  category: setting
  title: chat.showThinking
  description: Show thinking/reasoning blocks in chat output
  keywords: [setting, thinking, reasoning, display, tui]
  related: []
---

# chat.showThinking

Show thinking/reasoning blocks emitted by the agent in the TUI chat output.

## Overview

Controls whether the TUI renders the streaming reasoning ("Thinking") panel
that some models emit. When enabled (the default), reasoning content is
displayed as it streams. When disabled, reasoning chunks are still received
and stored in the conversation, but they are not shown in the rendered output.

## Examples

### Toggle via Display Settings Panel

In chat, open the settings panel and toggle "Show thinking":

```
/settings
```

Navigate to Display tab and toggle the "Show thinking" option.

### Enable via CLI

```bash
kiro-cli settings chat.showThinking true
```

### Check current value

```bash
kiro-cli settings chat.showThinking
```

### Disable

```bash
kiro-cli settings chat.showThinking false
```

## Notes

- Setting is stored in `~/.kiro/settings/cli.json` (or
  `$KIRO_HOME/settings/cli.json`).
- Changes via `/settings` → Display take effect immediately.
- Reasoning chunks continue to be ingested into the conversation history
  even when display is off, so disabling and re-enabling does not lose
  data within a session.
- If the underlying model does not emit reasoning content (e.g.,
  extended-thinking is disabled at the model level), nothing renders even
  with this setting on. That is the model's default behavior.
