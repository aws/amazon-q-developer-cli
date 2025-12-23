---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: tool
  title: report_issue
  description: Open browser with pre-filled GitHub issue template for reporting bugs or feature requests
  keywords: [report_issue, github, issue, bug, feature]
  related: [slash-issue]
---

# report_issue

Open browser with pre-filled GitHub issue template for reporting bugs or feature requests.

## Overview

The report_issue tool opens a browser with a pre-filled GitHub issue template. Automatically includes conversation transcript, context, request IDs, and environment details for bug reports.

## Usage

### Basic Usage

```json
{
  "title": "Bug: Command fails with error"
}
```

### Common Use Cases

#### Use Case 1: Report Bug

```json
{
  "title": "execute_bash fails on Windows",
  "expected_behavior": "Command should execute successfully",
  "actual_behavior": "Returns error: command not found",
  "steps_to_reproduce": "1. Run kiro-cli chat\n2. Execute bash command\n3. See error"
}
```

**What this does**: Opens browser with GitHub issue form pre-filled with details.

#### Use Case 2: Feature Request

```json
{
  "title": "Feature: Add support for Docker commands"
}
```

**What this does**: Opens feature request template.

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `title` | Yes | Issue title |
| `expected_behavior` | No | What should happen |
| `actual_behavior` | No | What actually happened |
| `steps_to_reproduce` | No | Steps to reproduce issue |

## Auto-Included Information

- Conversation transcript (last 3000 chars)
- Chat settings
- Request IDs
- Context files
- Tool permissions
- Environment details

## Examples

### Example 1: Simple Bug Report

```json
{
  "title": "Crash when loading large file"
}
```

### Example 2: Detailed Bug Report

```json
{
  "title": "fs_write fails with permission error",
  "expected_behavior": "File should be created",
  "actual_behavior": "Error: Permission denied",
  "steps_to_reproduce": "1. Try to write to /etc/config\n2. See error"
}
```

## Related

- [/issue](../slash-commands/issue.md) - Slash command version
- [kiro-cli issue](../commands/chat.md) - CLI command version

## Limitations

- Requires browser
- Opens GitHub (requires account)
- Transcript limited to 3000 chars
- Not available in headless mode

## Technical Details

**Aliases**: `report_issue`, `gh_issue`, `report`

**Permissions**: Trusted by default.

**Browser**: Opens default system browser.
