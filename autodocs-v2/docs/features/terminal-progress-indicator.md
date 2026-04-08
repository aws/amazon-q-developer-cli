---
doc_meta:
  validated: 2026-03-13
  commit: 5091402b
  status: validated
  testable_headless: false
  category: feature
  title: Terminal progress indicator
  description: OSC 9;4 progress indicator in the terminal tab/title bar
  keywords: [terminal, progress, osc, iterm2, wezterm, indicator]
  related: [terminal-hyperlinks]
---

# Terminal Progress Indicator

Kiro displays a progress indicator in the terminal tab or title bar using the OSC 9;4 escape sequence. This gives you at-a-glance status without looking at the main UI.

## Supported Terminals

- iTerm2
- WezTerm
- Windows Terminal

Unsupported terminals silently ignore the escape sequences.

## States

### While Processing

| Visual | State | When |
|--------|-------|------|
| 🟢 Spinning green | Active | Agent is streaming, running tools, or compacting |
| 🟡 Solid yellow (100%) | Paused | Waiting for tool approval |
| 🔴 Pulsing red | Error | An error occurred during processing |

### While Idle

| Visual | State | When |
|--------|-------|------|
| 🟡 Static yellow bar | Warning | Context usage ≥ 60% (bar shows actual %) |
| 🔴 Pulsing red | Error | A blocking error requires user action (e.g. auth) |
| _(hidden)_ | Normal | Everything is fine |

## Opt Out

Disable the progress indicator by setting the environment variable:

```bash
export KIRO_NO_PROGRESS=1
```

## Technical Details

The indicator uses the OSC 9;4 escape sequence:

```
ESC ] 9 ; 4 ; <state> [ ; <percent> ] BEL
```

| Code | Meaning | Percent |
|------|---------|---------|
| `0` | Clear/hidden | — |
| `1` | Success (green) | Required for static bar, omit for hidden |
| `2` | Error (red) | Optional — omit for pulsing, include for static |
| `3` | Indeterminate (spinning green) | — |
| `4` | Warning (yellow) | Required — omit shows nothing |
