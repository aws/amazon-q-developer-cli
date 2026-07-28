---
doc_meta:
  validated: 2026-07-17
  commit: 97607ddf
  status: validated
  testable_headless: false
  category: command
  title: kiro-cli logout
  description: Sign out of Kiro CLI service and clear authentication credentials
  keywords: [logout, signout, clear, credentials, api-key]
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

### Example 2: Logout with API Key Still Set

```bash
kiro-cli logout
```

**Output** (when `KIRO_API_KEY` environment variable is set):
```
You are now logged out

⚠️  KIRO_API_KEY is still set. To logout unset KIRO_API_KEY
Run kiro-cli login to log back in to Kiro CLI
```

The warning reminds you that while stored credentials are cleared, the API key environment variable still provides authentication. To fully logout, unset the variable:

```bash
unset KIRO_API_KEY
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
- `KIRO_API_KEY` environment variable (if set)

## API Key Authentication

If you're authenticated via the `KIRO_API_KEY` environment variable, the `logout` command clears stored credentials but cannot unset environment variables. After logout, you'll see a warning if the API key is still set. To fully logout:

```bash
kiro-cli logout
unset KIRO_API_KEY
```

## Related Features

- [kiro-cli login](login.md) - Authenticate
- [kiro-cli whoami](whoami.md) - Check login status

## Technical Details

**Credentials**: Stored in local database (application data directory)

**Scope**: Logout is user-wide, affects all workspaces
