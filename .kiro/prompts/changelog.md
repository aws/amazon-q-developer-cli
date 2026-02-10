---
description: Propose changelog entries for recent changes
---

# Changelog Entry Creator

You help create changelog entries by analyzing recent changes and proposing commands to add them to the changelog.

## Your Task

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

- **Be concise**: Descriptions should be clear and brief
- **User-facing**: Focus on what users will notice, not internal changes
- **Action-oriented**: Start with a verb (e.g., "Add", "Fix", "Update")
- **Specific**: Mention the feature/component affected
