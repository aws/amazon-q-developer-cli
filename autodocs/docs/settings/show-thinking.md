---
doc_meta:
  validated: 2026-05-16
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
that some models emit. When disabled (the default), reasoning chunks are
still received and stored in the conversation, but they are not shown in
the rendered output.

This setting is **startup-only** — changes take effect on the next TUI
launch.

## Examples

### Enable

```bash
kiro-cli settings chat.showThinking true
```

After restart, the TUI will render the `Thinking` panel for any reasoning
text the agent streams.

### Check current value

```bash
kiro-cli settings chat.showThinking
```

### Disable (return to default)

```bash
kiro-cli settings chat.showThinking false
```

## Notes

- Setting is read from `~/.kiro/settings/cli.json` (or
  `$KIRO_HOME/settings/cli.json`) once at TUI launch.
- Reasoning chunks continue to be ingested into the conversation history
  even when display is off, so disabling and re-enabling does not lose
  data within a session.
- If the underlying model does not emit reasoning content (e.g.,
  extended-thinking is disabled at the model level), nothing renders even
  with this setting on. That is the model's default behavior.
