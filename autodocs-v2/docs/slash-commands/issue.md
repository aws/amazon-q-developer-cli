---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /issue
  description: Create GitHub issue or feature request with pre-filled template
  keywords: [issue, github, bug, feature, report]
  related: [report-issue]
---

# /issue

Create GitHub issue or feature request with pre-filled template.

## Overview

The `/issue` command opens browser with GitHub issue template pre-filled with conversation context, transcript, and environment details. Same as report_issue tool.

## Usage

```
/issue
```

Opens browser with issue form.

## What's Included

- Conversation transcript
- Chat settings
- Request IDs
- Context files
- Tool permissions
- Environment details

## Examples

### Example 1: Report Bug

```
/issue
```

Browser opens with pre-filled issue template.

## Related Features

- [report_issue](../tools/report-issue.md) - Tool version

## Limitations

- Requires browser
- Not available in headless mode
- Requires GitHub account

## Technical Details

**Browser**: Opens default system browser

**Template**: Pre-filled with session context
