---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: settings-group
  title: Chat UI and Application Settings
  description: Settings for chat interface mode, data cleanup, and auto-updates
  keywords: [settings, chat, ui, tui, legacy, cleanup, autoupdate]
  related: [settings, classic-vs-tui, telemetry-privacy-settings]
---

# Chat UI and Application Settings

Configure chat interface behavior, data retention, and application updates.

## chat.ui

Chat UI mode selection.

### Overview

Controls which chat interface to use. The TUI (default) provides a rich terminal interface with themes, overlays, and enhanced tool rendering. Legacy mode uses the original Rust-based experience.

### Usage

```bash
kiro-cli settings chat.ui "tui"
```

**Type**: String  
**Default**: `tui`  
**Values**: `tui`, `legacy`  
**Scope**: Workspace-overridable

### Examples

```bash
# Use the new TUI (default)
kiro-cli settings chat.ui "tui"

# Use legacy/classic mode
kiro-cli settings chat.ui "legacy"

# Check current mode
kiro-cli settings chat.ui
```

### Notes

You can also switch modes per-session using CLI flags:
- `kiro-cli chat --legacy-ui` or `kiro-cli chat --classic` for legacy
- Default launches TUI

See [Classic Mode vs New TUI](../features/classic-vs-tui.md) for differences between modes.

---

## cleanup.periodDays

Data retention period in days.

### Overview

Sets how many days to retain old conversations and data before automatic cleanup. After this period, old sessions and associated data are deleted to manage storage.

This is a **global-only** setting and cannot be overridden per-workspace.

### Usage

```bash
kiro-cli settings cleanup.periodDays 30
```

**Type**: Number  
**Default**: None (no automatic cleanup)  
**Scope**: Global only

### Examples

```bash
# Keep data for 30 days
kiro-cli settings cleanup.periodDays 30

# Keep data for 90 days
kiro-cli settings cleanup.periodDays 90

# Check current setting
kiro-cli settings cleanup.periodDays

# Remove setting (disable automatic cleanup)
kiro-cli settings --delete cleanup.periodDays
```

---

## app.disableAutoupdates

Disable automatic updates on startup.

### Overview

When enabled, prevents Kiro CLI from checking for and applying updates automatically when starting. You can still update manually.

This is a **global-only** setting and cannot be overridden per-workspace.

### Usage

```bash
kiro-cli settings app.disableAutoupdates true
```

**Type**: Boolean  
**Default**: `false`  
**Scope**: Global only

### Examples

```bash
# Disable automatic updates
kiro-cli settings app.disableAutoupdates true

# Enable automatic updates (default)
kiro-cli settings app.disableAutoupdates false

# Check current setting
kiro-cli settings app.disableAutoupdates
```

---

## Troubleshooting

### UI Mode Not Changing

**Symptom**: Changed `chat.ui` but still seeing old interface  
**Solution**: Exit and restart the chat session. UI mode is determined at startup.

### Cleanup Deleting Too Much

**Symptom**: Conversations disappearing unexpectedly  
**Solution**: Increase `cleanup.periodDays` or remove the setting to disable automatic cleanup.

### Auto-updates Interfering

**Symptom**: Updates happening at inconvenient times  
**Solution**: Set `app.disableAutoupdates true` and update manually when convenient.

## Related

- [Settings Command](../commands/settings.md) - Managing all CLI settings
- [Classic Mode vs New TUI](../features/classic-vs-tui.md) - Interface differences
- [Telemetry and Privacy Settings](telemetry-privacy-settings.md) - Privacy controls
