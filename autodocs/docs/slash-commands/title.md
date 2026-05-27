---
doc_meta:
  validated: 2026-05-27
  commit: fada18425
  status: validated
  testable_headless: false
  category: slash_command
  title: /title
  description: Set, clear, or show the terminal window title
  keywords: [title, terminal, window, tab, workspace, settings]
---

# /title

Set, clear, or show the terminal window title.

## Overview

The `/title` command manages the terminal window title displayed in your tab bar or title bar. Requires the `chat.terminalTitle` setting to be enabled.

## Usage

```
/title              Show the current title
/title <text>       Set a sticky custom title
/title --clear      Remove custom title, revert to automatic
```

## Title Precedence

When no manual title is set, the title is derived automatically:
1. Backend session title (from first prompt, e.g. "WFP build fix")
2. Workspace basename (e.g. "~/src/my-project")

A manually set title is **sticky** — it persists through session updates until explicitly cleared.

## Examples

### Example 1: Show Current Title

```
/title
```

**Output** (toast): `Current title: kiro: ~/src/my-project`

### Example 2: Set Custom Title

```
/title weekly standup notes
```

**Output** (toast): `Title set: kiro: weekly standup notes`

### Example 3: Clear Custom Title

```
/title --clear
```

**Output** (toast): `Title cleared — showing: kiro: WFP build fix`

### Example 4: When Disabled

```
/title
```

**Output** (toast): `Current title (not active): kiro: ~/src/my-project — enable via /settings display`

## Configuration

Enable via the Display Settings panel (`/settings display`) or directly:

```
/settings set chat.terminalTitle true
```

The setting defaults to `false`. Changes take effect immediately — enabling shows the title, disabling clears it.

When disabled, `/title` shows a message explaining how to enable the feature.

## Behavior Notes

- Titles are truncated to 60 characters.
- Input consisting only of special/control characters will be rejected with an error.
- A manually set title is **sticky** — it persists through session updates until explicitly cleared.

## Technical Details

**Source**: `packages/tui/src/utils/terminal-title.ts`

**Mechanism**: Writes OSC 0 escape sequences (`\x1b]0;<title>\x07`)

**Limitations**:
- Some terminals ignore OSC 0 for security reasons
- Under tmux, requires `set-titles on` for outer terminal updates
- Title resets on exit (behavior varies by terminal)
