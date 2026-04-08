---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: command
  title: kiro-cli logout
  description: Sign out of Kiro CLI service and clear authentication credentials
  keywords: [logout, signout, clear, credentials]
  related: [login, whoami]
---

# kiro-cli logout

Sign out of Kiro CLI service and clear authentication credentials.

## Overview

The logout command signs out of Kiro CLI by clearing stored authentication credentials. Requires login again to use chat features.

## Usage

```bash
kiro-cli logout
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

## Examples

### Example 1: Logout

```bash
kiro-cli logout
```

**Output**:
```
You are now logged out
Run kiro-cli login to log back in to Kiro CLI
```

## What Gets Cleared

- Authentication tokens
- Session credentials
- User profile information

## What's Preserved

- Agent configurations
- Saved conversations
- Settings
- MCP server configurations

## Related Features

- [kiro-cli login](login.md) - Authenticate
- [kiro-cli whoami](whoami.md) - Check login status

## Technical Details

**Credentials**: Stored in local database (application data directory)

**Scope**: Logout is user-wide, affects all workspaces
