---
doc_meta:
  validated: 2026-05-07
  commit: dbc6e7ec
  status: validated
  testable_headless: false
  category: slash_command
  title: /changelog
  description: Show recent release notes inline in the terminal
  keywords: [changelog, version, history, releases, updates, release notes, what's new]
  related: [help]
---

# /changelog

Show recent release notes inline in the terminal.

## Overview

The `/changelog` command prints the two most recent Kiro CLI releases with their changes directly to the terminal. Each release shows the version number, date, and a categorized list of changes (added features, fixes, etc.). Output is printed inline and returns immediately — there is no interactive panel or scrolling.

## Usage

```
/changelog
```

No arguments or options.

## Output

The command prints for each release:
- Version number and release date in the header
- Change bullets grouped by type (Added, Fixed, Changed, etc.)
- Changes sorted alphabetically by type

## Examples

### Example 1: View Recent Releases

```
/changelog
```

**Output** (printed inline):
```
✨ What's new in 2.2.0 (2026-04-27)

✔ Added: Support adaptive thinking for complex tasks
✔ Added: New /compact command for context management
✔ Fixed: Fix API key authentication in certain regions

---

✨ What's new in 2.1.0 (2026-04-21)

✔ Added: Code intelligence with LSP support
✔ Fixed: Improved error handling in file operations
```

### Example 2: No Changelog Available

When release information is unavailable:

```
/changelog
```

**Output**:
```
No changelog information available.
```

## Troubleshooting

### Issue: "No changelog information available"

**Symptom**: Output shows no changelog data  
**Cause**: Release feed not embedded in the current build  
**Solution**: This is normal in development builds. Production releases include embedded changelog data.

## Related Features

- [/help](help.md) - List all available commands

## Technical Details

**Display Limit**: Shows the 2 most recent releases

**Content Source**: Embedded at build time via `include_str!("./feed.json")` — compiled into the binary, not fetched at runtime

**Output**: Printed inline to stderr using crossterm formatting
