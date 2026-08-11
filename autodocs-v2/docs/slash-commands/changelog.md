---
doc_meta:
  title: /changelog
  description: Show recent release notes for Kiro CLI
  category: slash_command
  keywords: [changelog, release, notes, updates, version, whats-new]
  related: [help]
  validated: 2026-05-07
  commit: dbc6e7ec
  status: validated
  testable_headless: false
---

# /changelog

Show recent release notes for Kiro CLI.

## Overview

The `/changelog` command prints the most recent release notes inline in the terminal. It shows the last 2 releases with their version numbers, dates, and categorized change lists (Added, Fixed, Changed, etc.).

This is useful for discovering new features after an update or reviewing what changed in recent versions.

## Usage

```
/changelog
```

## Output

Prints inline to the terminal:
- Version headers with release dates (e.g., "✨ What's new in 2.2.0 (2026-04-27)")
- Categorized bullet lists of changes (Added, Fixed, Changed, etc.)
- Horizontal separators between releases

Output is printed directly and returns immediately — there is no interactive panel or scrolling.

## Examples

### Example 1: View Recent Changes

```
/changelog
```

**Output** (printed inline):

```
✨ What's new in 2.2.0 (2026-04-27)

✔ Added: Support adaptive thinking for complex tasks
✔ Fixed: Fix API key authentication edge case

---

✨ What's new in 2.1.0 (2026-04-21)

✔ Added: New /spawn command for parallel agent sessions
✔ Changed: Improved context window management
```

## Troubleshooting

### Issue: "No changelog information available"

**Symptom**: Output shows "No changelog information available" message  
**Cause**: Release feed data is not available in the current build  
**Solution**: This is expected in development builds. Production releases include embedded changelog data.

### Issue: Shows Old Releases

**Symptom**: Changelog doesn't show expected recent releases  
**Cause**: You may be running an older version of Kiro CLI  
**Solution**: Update to the latest version with your package manager

## Limitations

- Shows only the 2 most recent releases
- Available only in classic mode (not TUI mode)
- Changelog data is embedded at build time via `include_str!`; it does not fetch from the network

## Related

- [/help](help.md) — List all available commands
