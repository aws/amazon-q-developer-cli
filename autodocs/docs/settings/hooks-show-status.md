---
doc_meta:
  validated: 2026-03-08
  commit: 06be7c71
  status: validated
  testable_headless: true
  category: setting
  title: hooks.showStatus
  description: Show or hide hook execution status messages (spinner and summary)
  keywords: [setting, hooks, status, spinner, quiet, silent]
  related: [hooks, slash-hooks]
---

# hooks.showStatus

Show or hide hook execution status messages during chat sessions.

## Overview

Controls whether Kiro CLI displays the spinner and summary line when hooks execute. When disabled, hooks run silently but error messages for failed hooks are still displayed.

## Usage

### Disable Status Messages

```bash
kiro-cli settings hooks.showStatus false
```

### Enable Status Messages

```bash
kiro-cli settings hooks.showStatus true
```

### Check Status

```bash
kiro-cli settings hooks.showStatus
```

## Value

**Type**: Boolean  
**Default**: `true`

## What It Controls

When `true` (default), you see:
- Spinner while hooks are running
- Summary line: `✓ X of Y hooks finished in Z s`

When `false`:
- No spinner
- No summary line
- Error messages still appear for failed hooks

## Examples

### Example 1: Suppress Hook Status

```bash
kiro-cli settings hooks.showStatus false
```

Hooks run silently. Only errors are shown.

### Example 2: Re-enable Status

```bash
kiro-cli settings hooks.showStatus true
```

Spinner and summary line appear during hook execution.

## Technical Details

**Scope**: User-wide setting

**Effect**: Applies to all hook executions in all sessions

**Error Handling**: Failed hooks (non-zero exit code) still display error messages regardless of this setting
