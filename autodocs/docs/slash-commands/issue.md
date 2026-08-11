---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
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

### Example 2: Browser Unavailable

If the browser cannot be opened (e.g., on a remote machine), the issue URL is printed to the terminal so you can copy it manually:

```
Issue Url: https://github.com/...
```

## Related Features

- [report_issue](../tools/report-issue.md) - Tool version

## Limitations

- Requires browser
- Not available in headless mode
- Requires GitHub account

## Troubleshooting

### Browser won't open

If your system browser cannot be launched or the session is remote, the command prints the full URL to the terminal. Copy the URL and paste it into a browser manually.

## Technical Details

**Browser**: Opens default system browser

**Template**: Pre-filled with session context
