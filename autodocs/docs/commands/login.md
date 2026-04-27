---
doc_meta:
  validated: 2026-04-16
  commit: 7404fa72c
  status: validated
  testable_headless: false
  category: command
  title: kiro-cli login
  description: Authenticate with Kiro CLI service using Builder ID, Social (Google/GitHub), or Identity Center
  keywords: [login, auth, authentication, builder-id, identity-center, govcloud, social, google, github, device-flow, remote, headless]
  related: [logout, whoami]
---

# kiro-cli login

Authenticate with Kiro CLI service using Builder ID, Social (Google/GitHub), or Identity Center.

## Overview

The login command authenticates with Kiro CLI service. Supports Builder ID, Social login via Google or GitHub, and Organization Identity, including GovCloud regions. Local environments use unified auth portal and ignores flags. Remote environments support device flow for all authentication methods except some Identity Provider, including social login via Google and GitHub.

## Usage

### Basic Usage

```bash
kiro-cli login
```

Opens browser for authentication (local) or shows device code (remote).

### With Options

```bash
kiro-cli login --license pro --identity-provider <url> --region us-east-1
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

Device flow for all authentication methods (Builder ID, Google, GitHub, Identity Center):

1. CLI presents a menu to select login method:
   - Use with Builder ID
   - Use with Google
   - Use with GitHub
   - Use with Your Organization
2. Shows a verification URL and user code
3. Open URL in browser on another device
4. Confirm user code and authorize
5. CLI polls for completion and signs in automatically

For social providers (Google/GitHub), the device flow uses the auth portal's endpoints. The CLI displays a verification URL with the code pre-filled for convenience.

## Examples

### Example 1: Basic Login

```bash
kiro-cli login
```

**Local**: Opens browser  
**Remote**: Shows login method menu, then device code

### Example 2: Identity Center

```bash
kiro-cli login --license pro --identity-provider https://my-org.awsapps.com/start --region us-east-1
```

### Example 3: Social Login (Local)

```bash
kiro-cli login
```

CLI opens the unified auth portal and select a social provider.

### Example 4: Social Login (Remote)

In a remote/SSH environment, run `kiro-cli login` and select "Use with Google" or "Use with GitHub" from the menu. The CLI will display:

```
To sign in with Google, visit:
  https://auth.example.com/device?code=ABCD-EFGH

And confirm the code: ABCD-EFGH

⠋ Waiting for authorization...
```

Complete authorization on any browser-enabled device. The CLI signs in automatically once approved.

### Example 5: Force Device Flow

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

### Issue: Device Code Expired

**Symptom**: "Device code expired. Please try again."  
**Cause**: Authorization was not completed before the code expired  
**Solution**: Run `kiro-cli login` again to get a new device code

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
- Remote environment uses device flow for all methods except some Identity Provider
- One active session at a time
- Session expires after period of inactivity

## Technical Details

**License Types**:
- `free`: Builder ID (AWS Builder ID)
- `pro`: Identity Center (AWS IAM Identity Center)

**Social Providers**: Google, GitHub (supported in both local and remote environments)

**Device Flow**: OAuth device authorization flow for remote environments. Social providers use a dedicated device authorization endpoint that returns a user code and verification URL. The CLI polls a device poll endpoint until the user completes authorization.

**Remote Detection**: Automatically detects SSH sessions and remote environments

**Session Storage**: Credentials stored securely in local database
