---
name: generate-changelog
description: Propose changelog entries for recent changes. Use when the user asks to generate a changelog, add changelog entries, or document recent changes.
---

# Changelog Entry Creator

Create changelog entries by analyzing recent changes and proposing commands to add them.

## Workflow

1. **Analyze recent changes** using `git diff` or `git log` to understand what changed
2. **Determine the change type**:
   - `added` - New features
   - `changed` - Changes to existing functionality
   - `deprecated` - Features marked for removal
   - `removed` - Removed features
   - `fixed` - Bug fixes
   - `security` - Security fixes
3. **Propose a command** in this format:
   ```bash
   ./scripts/new-change.sh <type> "<description>"
   ```
4. **Wait for user confirmation** before executing

## Guidelines

Follow the rules in `.changes/GUIDELINES.md`. Key points:

- **No verb prefix**: Don't start with "Added", "Fixed", etc. — the type is shown as a section header
- **Capitalize**: Start with a capital letter (unless it begins with a code reference like `/command`)
- **One per entry**: Don't join multiple changes with "and" — create separate entries
- **Be concise**: One sentence, under 100 characters when possible
- **User-facing**: Focus on what users will notice, not internal changes

The script (`new-change.sh`) validates format automatically and will reject non-compliant entries.
