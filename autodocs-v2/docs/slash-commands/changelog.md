---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /changelog
  description: View Kiro CLI changelog and version history with recent updates
  keywords: [changelog, version, history, releases, updates]
---

# /changelog

View Kiro CLI changelog and version history with recent updates.

## Overview

The `/changelog` command displays recent version changes, new features, bug fixes, and improvements from Kiro CLI releases.

## Usage

```
/changelog
```

Shows recent changelog entries.

## Output

Displays for each version:
- Version number
- Release date
- New features
- Bug fixes
- Improvements
- Breaking changes

## Examples

### Example 1: View Changelog

```
/changelog
```

**Output**:
```
Kiro CLI Changelog

## v1.5.0 (2025-12-15)
- Feature: Added code intelligence with LSP
- Feature: New /compact command
- Fix: Improved error handling in fs_write
...
```

## Alternative

```bash
kiro-cli --version --changelog
kiro-cli --version --changelog=all
kiro-cli --version --changelog=1.4.0
```

## Related Features

- [kiro-cli --version](../commands/chat.md) - Version information

## Technical Details

**Source**: Changelog from feed.json

**Display**: Shows recent entries by default
