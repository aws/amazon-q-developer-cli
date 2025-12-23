---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: command
  title: kiro-cli login
  description: Authenticate with Kiro CLI service using Builder ID or Identity Center
  keywords: [login, auth, authentication, builder-id, identity-center]
  related: [logout, whoami]
---

# kiro-cli login

Authenticate with Kiro CLI service using Builder ID or Identity Center.

## Overview

The login command authenticates with Kiro CLI service. Supports Builder ID (free) and Identity Center (pro). Local environments use unified auth portal. Remote environments support device flow for SSH/terminal-only access.

## Usage

### Basic Usage

```bash
kiro-cli login
```

Opens browser for authentication (local) or shows device code (remote).

### With Options

```bash
kiro-cli login --license pro --identity-provider <url> --region us-east-1
kiro-cli login --social google
kiro-cli login --use-device-flow
```

## Options

| Option | Description |
|--------|-------------|
| `--license <TYPE>` | License type: `pro` (Identity Center) or `free` (Builder ID, Google, Github) |
| `--identity-provider <URL>` | Identity provider URL (for Identity Center) |
| `--region <REGION>` | AWS region (for Identity Center) |
| `--social <PROVIDER>` | Social provider: `google` or `github` |
| `--use-device-flow` | Force device flow (for remote/SSH environments) |
| `--verbose` | Increase logging verbosity (can be repeated) |
| `--help` | Print help information |

## Authentication Methods

### Local Environment

Unified auth portal (ignores CLI flags):
1. Opens browser
2. Select authentication method
3. Complete authentication
4. Returns to CLI

### Remote Environment (SSH/Terminal)

Device flow:
1. Shows device code and URL
2. Open URL in browser on another device
3. Enter device code
4. Complete authentication
5. CLI polls for completion

## Examples

### Example 1: Basic Login

```bash
kiro-cli login
```

**Local**: Opens browser  
**Remote**: Shows device code

### Example 2: Identity Center

```bash
kiro-cli login --license pro --identity-provider https://my-org.awsapps.com/start --region us-east-1
```

### Example 3: Social Login

```bash
kiro-cli login --social google
```

### Example 4: Force Device Flow

```bash
kiro-cli login --use-device-flow
```

Useful for SSH sessions or when browser redirect doesn't work.

## Troubleshooting

### Issue: Already Logged In

**Symptom**: "Already logged in" error  
**Cause**: Active session exists  
**Solution**: Logout first: `kiro-cli logout`

### Issue: Browser Doesn't Open

**Symptom**: No browser window  
**Cause**: Remote environment or browser not available  
**Solution**: Use `--use-device-flow` flag

### Issue: Authentication Timeout

**Symptom**: Login times out  
**Cause**: Didn't complete authentication in time  
**Solution**: Restart login process

### Issue: Identity Center Not Working

**Symptom**: Identity Center login fails  
**Cause**: Invalid identity provider URL or region  
**Solution**: Verify URL and region with your administrator

## Related Features

- [kiro-cli logout](logout.md) - Sign out
- [kiro-cli whoami](whoami.md) - Check login status
- [kiro-cli chat](chat.md) - Requires authentication

## Limitations

- Local environment always uses unified portal (ignores flags)
- Remote environment requires device flow
- One active session at a time
- Session expires after period of inactivity

## Technical Details

**License Types**:
- `free`: Builder ID (AWS Builder ID)
- `pro`: Identity Center (AWS IAM Identity Center)

**Social Providers**: Google, GitHub

**Device Flow**: OAuth device authorization flow for remote environments

**Remote Detection**: Automatically detects SSH sessions and remote environments

**Session Storage**: Credentials stored securely in local database
