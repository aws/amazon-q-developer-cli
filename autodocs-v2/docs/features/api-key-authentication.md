---
doc_meta:
  validated: 2026-08-12
  commit: e49533c92
  status: validated
  testable_headless: true
  category: feature
  title: API Key Authentication
  description: Authenticate using KIRO_API_KEY environment variable for headless and CI/CD workflows
  keywords: [api-key, authentication, headless, ci, cd, automation, KIRO_API_KEY, non-interactive]
  related: [login, logout, whoami, chat]
---

# API Key Authentication

Authenticate using the `KIRO_API_KEY` environment variable for headless and CI/CD workflows.

## Overview

Kiro CLI supports API key authentication as an alternative to interactive login. Set the `KIRO_API_KEY` environment variable to authenticate without browser-based login flows. This enables headless operation in CI/CD pipelines, automation scripts, and non-interactive environments.

## Usage

### Basic Usage

```bash
export KIRO_API_KEY="your-api-key"
kiro-cli chat --no-interactive "Explain this code"
```

### In CI/CD Pipelines

```bash
# GitHub Actions example
KIRO_API_KEY=${{ secrets.KIRO_API_KEY }} kiro-cli chat --no-interactive "Review this PR"
```

## Authentication Priority

Kiro CLI checks authentication sources in this order:

1. **External IdP** - Enterprise identity provider (if configured)
2. **Stored credentials** - Builder ID or Social login from `kiro-cli login`
3. **API key** - `KIRO_API_KEY` environment variable

The API key is only used when no stored credentials exist. If you've previously logged in, those credentials take precedence.

## Examples

### Example 1: Headless Code Review

```bash
export KIRO_API_KEY="your-api-key"
kiro-cli chat --no-interactive "Review the changes in this diff" < changes.diff
```

### Example 2: CI Pipeline Integration

```yaml
# .github/workflows/review.yml
- name: AI Code Review
  env:
    KIRO_API_KEY: ${{ secrets.KIRO_API_KEY }}
  run: |
    kiro-cli chat --no-interactive "Analyze this codebase for security issues"
```

### Example 3: Automation Script

```bash
#!/bin/bash
export KIRO_API_KEY="your-api-key"

for file in src/*.py; do
  kiro-cli chat --no-interactive "Document this file: $(cat $file)"
done
```

### Example 4: Docker Container

```dockerfile
FROM ubuntu:latest
ENV KIRO_API_KEY=""
# Install kiro-cli (download from https://kiro.dev for your platform)
COPY kiro-cli /usr/local/bin/kiro-cli
CMD ["kiro-cli", "chat", "--no-interactive", "Hello"]
```

```bash
docker run -e KIRO_API_KEY="your-api-key" my-kiro-image
```

## Troubleshooting

### Issue: Not Logged In Error

**Symptom**: "Not logged in. Set the KIRO_API_KEY environment variable or run `kiro-cli login` first."  
**Cause**: No valid authentication found  
**Solution**: Ensure `KIRO_API_KEY` is set and not empty:
```bash
echo $KIRO_API_KEY  # Should show your key
export KIRO_API_KEY="your-api-key"
```

### Issue: API Key Ignored

**Symptom**: CLI uses stored credentials instead of API key  
**Cause**: Previously logged in with `kiro-cli login`  
**Solution**: Logout first if you want to use API key:
```bash
kiro-cli logout
export KIRO_API_KEY="your-api-key"
kiro-cli chat --no-interactive "test"
```

### Issue: Empty API Key

**Symptom**: Authentication fails despite setting variable  
**Cause**: Variable set to empty string  
**Solution**: Ensure key has a value:
```bash
# Wrong
export KIRO_API_KEY=""

# Correct
export KIRO_API_KEY="your-actual-key"
```

### Issue: Key Not Persisting

**Symptom**: Works in one terminal, not another  
**Cause**: Environment variable not exported  
**Solution**: Add to shell profile or export in each session:
```bash
# Add to ~/.bashrc or ~/.zshrc
export KIRO_API_KEY="your-api-key"
```

## Related

- [kiro-cli login](../commands/login.md) - Interactive authentication
- [kiro-cli logout](../commands/logout.md) - Clear stored credentials
- [kiro-cli whoami](../commands/whoami.md) - Check authentication status
- [kiro-cli chat](../commands/chat.md) - Chat command with `--no-interactive` flag

## Limitations

- API key is only used when no stored credentials exist
- Cannot use API key and stored credentials simultaneously
- API key must be set before starting the CLI
- No built-in key rotation mechanism

## Technical Details

**Environment Variable**: `KIRO_API_KEY`

**Priority**: Lowest priority after External IdP and stored credentials

**Validation**: Empty strings are treated as unset

**Header**: Requests include `TokenType: API_KEY` header for backend routing
