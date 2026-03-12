# API Key Authentication for Kiro CLI

## Overview

Kiro CLI supports API key authentication via the `KIRO_API_KEY` environment variable as an alternative to interactive login (Builder ID, Social, External IdP). This enables headless and CI/CD use cases where interactive browser-based login is not possible.

## Requirements

1. Users can authenticate by setting `KIRO_API_KEY` environment variable
2. Stored credentials (from `kiro-cli login`) take priority over the API key
3. API key works for all commands that require authentication (chat, whoami, profile, etc.)
4. In `--non-interactive` mode with no auth, show a clear error message (no interactive login prompt)
5. In interactive mode with no auth, keep existing behavior (prompt for login)
6. Authentication failures (invalid/expired credentials) show user-friendly messages

## User Experience

### Setting the API Key

```bash
# Option 1: Export for the session
export KIRO_API_KEY=ksk_your_api_key_here
kiro-cli chat --non-interactive "hello"

# Option 2: Inline for a single command
KIRO_API_KEY=ksk_your_api_key_here kiro-cli chat --non-interactive "hello"
```

### Authentication Priority

When determining which credentials to use, the CLI follows this priority

1. Auth token from Database (External IDP/BuilderId/Social Token)
2. `KIRO_API_KEY` environment variable
3. No credentials → error or login prompt

This priority is consistent across both interactive and non interactive mode

### Behavior Matrix

| Stored Creds | KIRO_API_KEY | --non-interactive | Behavior |
|---|---|---|---|
| ✅ | Set | Yes/No | Uses stored creds, ignores API key |
| ❌ | Set | Yes | Uses API key |
| ❌ | Set | No | Uses API key, skips login prompt |
| ❌ | Not set | Yes | Error: suggests KIRO_API_KEY or login |
| ❌ | Not set | No | Prompts for interactive login |


## Technical Approach

The implementation touches three layers of the V1 (`chat_cli`) authentication pipeline:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Auth Gate (cli/mod.rs)                                   │
│    is_logged_in() → checks stored creds → checks KIRO_API_KEY│
│    Controls: login prompt vs error vs proceed                │
├─────────────────────────────────────────────────────────────┤
│ 2. Bearer Token Resolution (auth/mod.rs)                    │
│    UnifiedBearerResolver → tries stored creds → KIRO_API_KEY │
│    Controls: which token is sent as Authorization: Bearer    │
├─────────────────────────────────────────────────────────────┤
│ 3. Request Header (api_client/token_type_interceptor.rs)    │
│    AuthMode enum → sets TokenType header                     │
│    Controls: TokenType: API_KEY | EXTERNAL_IDP | (none)      │
└─────────────────────────────────────────────────────────────┘
```

### Request Flow with API Key

```
User sets KIRO_API_KEY env var
    │
    ▼
is_logged_in() ── no stored creds ── finds KIRO_API_KEY ── returns true
    │
    ▼
ApiClient::new() ── no stored creds ── finds KIRO_API_KEY ── sets AuthMode::ApiKey
    │
    ▼
HTTP Request:
    Authorization: Bearer <KIRO_API_KEY value>
    TokenType: API_KEY
    │
    ▼
Server validates ── 200 OK ── response streamed back
              └── 403 ── AccessDenied ── friendly error message
```

### Security Considerations

- The API key is read from the environment variable at runtime, never persisted to disk
- The key is transmitted as a standard `Authorization: Bearer` token over the existing HTTPS connection
- Empty `KIRO_API_KEY` values are treated as unset (filtered by `get_api_key()`)

### Future Considerations

- V2 (`chat_cli_v2`) support: apply the same pattern to the V2 crate's auth pipeline
