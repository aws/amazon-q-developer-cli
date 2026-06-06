---
doc_meta:
  title: /lite
  description: Switch to lite mode — a classic-style scrollback UI with configurable verbosity
  category: slash_command
  keywords: [lite, mode, ui, classic, scrollback, switch, tui, display]
  related: [settings, verbose, classic-vs-tui]
  validated: 2026-06-06
  commit: 89110a61d
  status: validated
  testable_headless: false
---

## Overview

The `/lite` command switches the current session from the TUI (React/Ink) interface to lite mode — a classic-style scrollback UI that renders directly to the terminal's scroll buffer.

Lite mode is available to internal and nightly users. Outside that cohort, `/lite` shows a "not available" message and does nothing.

## Usage

```
/lite
```

Switches from TUI to lite mode immediately. The conversation history is re-rendered in lite style so the entire session appears consistent.

To switch back:

```
/tui
```

## What is Lite Mode

Lite mode renders the conversation as plain scrollback text rather than the TUI's overlay panels and Ink components. Key differences:

- Messages render as `You:` / `Agent:` headers followed by content
- Tool calls render inline with configurable verbosity (see [/verbosity](verbose.md))
- No overlay panels — commands like `/help` print to scrollback
- Terminal scroll buffer is preserved (you can scroll up to see shell history)
- Lighter resource usage (no React render loop)

## Starting in Lite Mode

### CLI Flag

```bash
kiro-cli chat --lite
```

Launches directly in lite mode without switching from TUI first.

### Persisted Preference

Use `/settings display` to set the default UI mode. The `chat.ui.mode` setting controls which mode starts on launch:

```
/settings display
```

Select "Default UI mode" and choose between `tui` and `lite`.

## Examples

### Switch to lite mid-session

```
/lite
```

Output:
```
Switched to lite mode
```

### Switch back to TUI

```
/tui
```

### Start a new session in lite mode

```bash
kiro-cli chat --lite
```

### Lite mode unavailable

If the feature is not enabled for your build:

```
/lite
```

Output:
```
Lite mode is not available in this build
```

## Availability

Lite mode is gated behind a rollout flag. It is currently available to:

- Internal users on the nightly channel
- Insider toolbox channel installs

The `--lite` CLI flag and `/lite` slash command are no-ops outside this cohort.

## Troubleshooting

### Issue: "/lite" says not available

**Symptom**: "Lite mode is not available in this build"
**Cause**: You're on a stable release outside the rollout cohort.
**Solution**: Wait for general availability or use a nightly build.

### Issue: Scrollback looks garbled after switching

**Symptom**: Mixed TUI and lite rendering on screen after `/lite`
**Cause**: Terminal didn't fully clear the TUI output.
**Solution**: Type `/clear` after switching to reset the view. The conversation is re-rendered in lite style automatically, so content is not lost.

### Issue: Panels don't open

**Symptom**: `/help`, `/tools`, etc. don't show overlay panels
**Cause**: Lite mode prints command output to scrollback instead of panels.
**Solution**: This is expected behavior. Scroll up to see the output.

## Related

- [/verbosity](verbose.md) — Configure tool output detail level in lite mode
- [/settings](settings.md) — Set default UI mode and other preferences
- [Classic vs TUI](../features/classic-vs-tui.md) — Comparison of UI modes
