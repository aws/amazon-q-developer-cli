---
doc_meta:
  title: /changelog
  description: View recent Kiro CLI release notes and updates
  category: slash_command
  keywords: [changelog, release, notes, updates, version, history, news, features]
  related: [help]
  validated: 2026-05-14
  commit: 6a6aa55cc
  status: validated
  testable_headless: true
---

# /changelog

View recent Kiro CLI release notes and updates.

## Overview

The `/changelog` command displays recent release notes directly in the TUI. It shows the last two releases with their version numbers, dates, and categorized changes.

This is the same content shown in the startup announcement when a new version is detected, but available on demand.

## Usage

```
/changelog
```

No arguments or options.

## Output Format

The changelog renders differently depending on the engine:

### V1 (CLI)

The output uses a sparkle header followed by entries formatted with a checkmark and change type:

```
✨ What's New in Kiro CLI

2.4.0 (2026-05-13)
✔ Added: /rewind to jump back to an earlier prompt in a conversation
✔ Changed: Shell escape (!) commands now use $SHELL instead of hardcoded bash
✔ Fixed: Fixed MCP server env variables being overridden by shell env
```

### V2 (TUI)

The output renders as markdown with a sparkle-decorated heading per release and bold-labeled bullets, separated by horizontal rules:

```
## ✨ What's new in 2.4.0 (2026-05-13)

- **Added**: /rewind to jump back to an earlier prompt
- **Changed**: Shell escape (!) commands now use $SHELL
- **Fixed**: Fixed MCP server env variables being overridden
- **Security**: Upgraded dependency to patch CVE-XXXX

---

## ✨ What's new in 2.3.0 (2026-05-06)
...
```

### Change Types

Both engines support five change type categories:

| Type | Description |
|------|-------------|
| Added | New features and capabilities |
| Changed | Modifications to existing behavior |
| Fixed | Bug fixes and corrections |
| Security | Security-related patches and updates |
| Deprecated | Features scheduled for removal |

## Examples

### Example 1: View Recent Changes

```
/changelog
```

Displays the last two releases with all their categorized changes.

### Example 2: Check After Update

After running `kiro-cli update`, use `/changelog` to see what's new:

```
/changelog
```

Review the changes to learn about new features and improvements.

### Example 3: CLI Version Subcommand

You can also view the changelog from the command line:

```bash
kiro-cli version --changelog
```

Or for a specific version:

```bash
kiro-cli version --changelog=2.4.0
```

Or all versions:

```bash
kiro-cli version --changelog=all
```

## Startup Announcement

When you start Kiro CLI after an update, a changelog announcement automatically appears showing what's new. The `/changelog` command lets you revisit this information at any time.

The startup announcement shows up to three times per version, then stops appearing automatically.

## Troubleshooting

### Issue: Changelog shows old versions

**Cause**: The changelog is embedded at build time.

**Solution**: Reinstall or update to the latest version using your package manager or the installation script.

### Issue: No changelog displayed

**Cause**: Rare edge case with corrupted installation.

**Solution**: Reinstall Kiro CLI or update to the latest version.

## Related

- [/help](help.md) — View available commands and usage
