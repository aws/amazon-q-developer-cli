---
doc_meta:
  validated: 2026-05-22
  commit: 127ec7961
  status: validated
  testable_headless: false
  category: slash_command
  title: /changelog
  description: View Kiro CLI changelog and version history with recent updates
  keywords: [changelog, version, history, releases, updates, whats new]
  related: [chat]
---

# /changelog

View Kiro CLI changelog and version history with recent updates.

## Overview

The `/changelog` command displays recent version changes, new features, bug fixes, and improvements from Kiro CLI releases. Changes are grouped by type (Added, Changed, Fixed, etc.) for easier scanning.

## Usage

```
/changelog
```

Shows recent changelog entries in a scrollable panel.

## Output

Displays for each version:
- Version number and release date
- Changes grouped by type:
  - **Added** - New features
  - **Changed** - Behavior changes
  - **Fixed** - Bug fixes
  - **Security** - Security updates
  - **Deprecated** - Deprecated features

## Examples

### Example 1: View Changelog

```
/changelog
```

**Output**:
```
**✨ What's new in 2.2.0 (2026-04-27)**

**Added**
- Support adaptive thinking
- New /compact command

**Fixed**
- Fix API key auth
- Improved error handling
```

### Example 2: Scroll Through Changelog

Use arrow keys to scroll through longer changelogs. Press `q` or `Escape` to close.

## Welcome Message

When you start Kiro CLI, a condensed changelog appears showing only **Added** items from recent releases. Press `Ctrl+O` to expand and see all change types (Fixed, Changed, etc.).

## Related Features

- [kiro-cli chat](../commands/chat.md) - Start chat sessions

## Technical Details

**Source**: Changelog from feed.json

**Display**: Groups changes by type (Added, Changed, Fixed, Security, Deprecated)

**Welcome bar**: Shows only Added items by default; Ctrl+O expands to full changelog
