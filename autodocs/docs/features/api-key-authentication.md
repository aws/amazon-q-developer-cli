---
doc_meta:
  title: API Key Authentication
  description: Authenticate with Kiro CLI using KIRO_API_KEY environment variable for headless and CI/CD use
  category: feature
  keywords: [api-key, authentication, headless, ci-cd, automation, KIRO_API_KEY, non-interactive]
  related: [login, chat, exit-codes]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
---

# API Key Authentication

Authenticate with Kiro CLI using the `KIRO_API_KEY` environment variable for headless and CI/CD use.

## Overview

API key authentication provides a way to authenticate Kiro CLI without interactive login. Set the `KIRO_API_KEY` environment variable to authenticate in CI/CD pipelines, automation scripts, and other non-interactive environments.

## Usage

### Set Environment Variable

```bash
export KIRO_API_KEY=your-api-key
```

### Use with Headless Mode

```bash
KIRO_API_KEY=your-api-key kiro-cli chat --no-interactive --trust-all-tools "Run tests"
```

## Authentication Priority

Kiro CLI checks authentication methods in this order:

1. **External IdP** - Enterprise identity provider (if configured)
2. **Builder ID / Social login** - Stored credentials from `kiro-cli login`
3. **API key** - `KIRO_API_KEY` environment variable

The API key is only used when no stored credentials exist. If you're logged in via `kiro-cli login`, those credentials take precedence.

## Examples

### Example 1: CI/CD Pipeline

```yaml
# GitHub Actions example
jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run Kiro analysis
        env:
          KIRO_API_KEY: ${{ secrets.KIRO_API_KEY }}
        run: |
          kiro-cli chat --no-interactive --trust-all-tools "Analyze code quality"
```

### Example 2: Shell Script

```bash
#!/bin/bash
export KIRO_API_KEY="your-api-key"
kiro-cli chat --no-interactive --trust-all-tools "Generate test coverage report"
```

### Example 3: Inline Usage

```bash
KIRO_API_KEY=your-api-key kiro-cli chat --no-interactive "List project structure"
```

## Troubleshooting

### Issue: Not Logged In Error

**Symptom**: "Not logged in. Set the KIRO_API_KEY environment variable or run `kiro-cli login` first."  
**Cause**: No valid authentication found  
**Solution**: Ensure `KIRO_API_KEY` is set and not empty

### Issue: API Key Not Used

**Symptom**: Using stored credentials instead of API key  
**Cause**: Already logged in via `kiro-cli login`  
**Solution**: Run `kiro-cli logout` to clear stored credentials if you want to use API key

### Issue: Empty API Key

**Symptom**: Authentication fails despite setting variable  
**Cause**: `KIRO_API_KEY` is set to empty string  
**Solution**: Ensure the variable contains a valid, non-empty key

## Related Features

- [kiro-cli login](../commands/login.md) - Interactive authentication
- [kiro-cli chat](../commands/chat.md) - Chat command with headless mode
- [Exit Codes](exit-codes.md) - Understanding CLI exit codes

## Limitations

- API key is only used when no stored credentials exist
- Cannot be used alongside active login session (login takes precedence)
- Key must be non-empty string

## Technical Details

**Environment Variable**: `KIRO_API_KEY`

**Precedence**: Lowest priority - only used as fallback when no other auth method is available

**Validation**: Empty strings are treated as unset
