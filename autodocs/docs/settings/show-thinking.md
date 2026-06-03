---
doc_meta:
  validated: 2026-06-03
  commit: 54d1f048f
  status: validated
  testable_headless: true
  category: setting
  title: chat.showThinking
  description: Control how reasoning/thinking blocks are displayed (collapsed, expanded, or off)
  keywords: [setting, thinking, reasoning, display, tui, collapsed, expanded]
  related: [enable-thinking]
---

# chat.showThinking

Control how the agent's reasoning ("thinking") blocks are displayed in the TUI.

## Overview

When models emit reasoning content, this setting controls its visibility:

| Mode | Behavior |
|------|----------|
| `expanded` | Full reasoning stream shown inline (default) |
| `collapsed` | Header only; press ctrl+o to expand |
| `off` | Reasoning hidden entirely |

Reasoning chunks are always received and stored in the conversation history
regardless of this setting — only the display is affected.

## Values

- **`expanded`** (default): The full reasoning stream is always shown inline as it arrives. No user interaction needed to see reasoning.
- **`collapsed`**: Shows only a "Thinking..." header while streaming, or "Thought for Ns..." when complete. Press ctrl+o to expand and view the full reasoning. Press ctrl+o again to collapse.
- **`off`**: Reasoning is never rendered, though it remains in the conversation history.

### Legacy boolean values

For backwards compatibility:
- `true` → treated as `collapsed`
- `false` → treated as `off`

## Examples

### Set to expanded (default)

```bash
kiro-cli settings chat.showThinking expanded
```

Reasoning streams inline as the model thinks.

### Set to collapsed

```bash
kiro-cli settings chat.showThinking collapsed
```

Shows only the header. Use ctrl+o to toggle the full reasoning view.

### Disable reasoning display

```bash
kiro-cli settings chat.showThinking off
```

### Check current value

```bash
kiro-cli settings chat.showThinking
```

### Toggle in-session via Display Settings

Press ctrl+d to open the Display Settings panel, navigate to "Show thinking",
and press left/right arrows to cycle through: `collapsed` → `expanded` → `off`.

## Visual examples

### Collapsed mode (while streaming)

```
⋮ Thinking... (esc to cancel · ctrl+o to view)
```

### Collapsed mode (after completion)

```
● Thought for 3s... (ctrl+o to view)
```

### Expanded mode (ctrl+o pressed, or `expanded` setting)

```
● Thought for 3s... (ctrl+o to collapse details)
  ╰ Let me analyze the authentication middleware...
    The key changes will be detecting expired tokens,
    checking for valid refresh tokens, then issuing
    new access tokens.
```

### Static (history) view

Once a turn is flushed to scrollback, it shows a frozen hint with no
ctrl+o affordance:

```
● Thought for 3s...
```

## Troubleshooting

### Reasoning not appearing even with setting enabled

- The model may not emit reasoning content. Not all models or configurations
  produce thinking blocks.
- Check that `chat.enableThinking` is also enabled if available.

### ctrl+o not working

- ctrl+o only works in `collapsed` mode on the active turn.
- In the static (history) buffer, the expansion state is frozen.
- In `expanded` mode, ctrl+o is a no-op on thinking blocks since they're
  already fully shown.

### Setting doesn't take effect immediately

This setting is read at TUI launch. Restart the chat session for changes
to take effect, or use the Display Settings panel (ctrl+d) to change it
mid-session.

## Technical Details

- Setting stored in `~/.kiro/settings/cli.json` (or `$KIRO_HOME/settings/cli.json`)
- The ctrl+o expansion state is shared with tool outputs — pressing ctrl+o
  toggles all collapsible content at once
- Duration shown ("Thought for Ns") is wall-clock time from first reasoning
  chunk to first content or tool call

## Related

- `chat.enableThinking` - Enable/disable thinking at the model level
