---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli whoami
  description: Display current login session information including user and authentication method
  keywords: [whoami, user, login, info, status]
  related: [login, logout]
---

# kiro-cli whoami

Display current login session information including user and authentication method.

## Overview

The whoami command displays information about the current authenticated user, including username, authentication method, and session details.

## Usage

```bash
kiro-cli whoami
```

### With Output Format

```bash
kiro-cli whoami --format json
kiro-cli whoami --format json-pretty
```

## Output

Shows:
- Username/user ID
- Authentication method (Builder ID, Identity Center, Social)
- Session status
- Additional profile information

## Examples

### Example 1: Check Current User

```bash
kiro-cli whoami
```

**Output**:
```
Logged in with IAM Identity Center (https://amzn.awsapps.com/start)

Profile:
Q-Dev-Amazon-Profile
arn:aws:codewhisperer:us-east-1:...:profile/...
```

Or for Builder ID:
```
Logged in with Builder ID

Profile:
builder-id-username
```

### Example 2: JSON Output

```bash
kiro-cli whoami --format json
```

**Output**:
```json
{
  "username": "john.doe",
  "auth_method": "builder_id",
  "status": "active"
}
```

## Troubleshooting

### Issue: Not Logged In

**Symptom**: "Not logged in" error  
**Cause**: No active session  
**Solution**: Login with `kiro-cli login`

## Related Features

- [kiro-cli login](login.md) - Authenticate
- [kiro-cli logout](logout.md) - Sign out

## Technical Details

**Output Formats**: plain (default), json, json-pretty

**Session Info**: Retrieved from local database
