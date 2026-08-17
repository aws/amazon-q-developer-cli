---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: settings-group
  title: API and Service Settings
  description: Settings for API timeouts and service endpoint configurations
  keywords: [settings, api, timeout, service, network, endpoint, codewhisperer, oidc]
  related: [settings, default-model]
---

# API and Service Settings

Configure API timeouts and service endpoint settings for Kiro CLI.

## api.timeout

API request timeout in milliseconds.

### Overview

Sets the timeout duration for API requests made by Kiro CLI. Affects requests to AI models, external services, and other network operations.

### Usage

```bash
kiro-cli settings api.timeout 300000
```

**Type**: Number  
**Default**: `300000` (5 minutes)  
**Unit**: Milliseconds  
**Scope**: Workspace-overridable

### Examples

```bash
# Set 5-minute timeout (default)
kiro-cli settings api.timeout 300000

# Set 2-minute timeout for faster failure detection
kiro-cli settings api.timeout 120000

# Set 10-minute timeout for slow connections
kiro-cli settings api.timeout 600000

# Check current timeout
kiro-cli settings api.timeout
```

### Timeout Guidelines

| Duration | Milliseconds | Use Case |
|----------|-------------|----------|
| 1 minute | 60000 | Fast networks, quick failure detection |
| 2 minutes | 120000 | Balanced for most connections |
| 5 minutes | 300000 | Default, handles complex requests |
| 10 minutes | 600000 | Very slow networks or large operations |

## api.streamIdleSoftTimeout

Stream idle soft timeout in seconds.

### Overview

Inter-event silence threshold after which Kiro CLI shows a "stream stalled" warning while continuing to wait for the response stream. Measures the gap between consecutive stream events (never total response time), so long responses with steady output never trigger it.

### Usage

```bash
kiro-cli settings api.streamIdleSoftTimeout 60
```

**Type**: Number  
**Default**: `60`  
**Unit**: Seconds  
**Scope**: Workspace-overridable

Set to `0` to disable the warning.

Note the unit: seconds, unlike `api.timeout` which is milliseconds. Values above 3600 (one hour) are clamped to 3600 with a logged warning, so a milliseconds-shaped value cannot silently disable the watchdog.

Must resolve below `api.streamIdleHardTimeout` to ever fire: an explicitly configured value at or past the hard timeout disables the warning tier (with a logged warning), while the inherited default is clamped to half the hard timeout if only the hard cap is lowered.

Only consulted by the built-in Rust agent engine. With `--agent-engine=kas` the KAS engine manages its own stream resilience and ignores this setting.

## api.streamIdleHardTimeout

Stream idle hard timeout in seconds.

### Overview

Inter-event silence threshold after which Kiro CLI abandons the response stream and retries as a stream timeout. Protects against silently dead connections that would otherwise hang a turn forever. Like the soft threshold, it measures inter-event gaps, not total response time.

### Usage

```bash
kiro-cli settings api.streamIdleHardTimeout 300
```

**Type**: Number  
**Default**: `300`  
**Unit**: Seconds  
**Scope**: Workspace-overridable

Set to `0` to disable the client-side stream deadline.

Note the unit: seconds, unlike `api.timeout` which is milliseconds. Values above 3600 (one hour) are clamped to 3600 with a logged warning, so a milliseconds-shaped value cannot silently disable the watchdog.

Only consulted by the built-in Rust agent engine. With `--agent-engine=kas` the KAS engine manages its own stream resilience and ignores this setting.

## api.subagentTimeout

Per-subagent idle window in seconds.

### Overview

Cancels an individual subagent (V1 `use_subagent` child) or crew stage (V2 `agent_crew`) only after that child goes a full window with no observable progress of its own; any of its progress events (assistant tokens, tool calls, tool results) restarts its window, and human-blocked waits (tool-approval prompts, MCP OAuth grants) suspend it, bounded at 24 hours so an unanswerable prompt cannot suspend the deadline forever. Progress means emitted events: a child streaming or calling tools runs indefinitely, but a single tool call that stays silent for a full window counts as a stall — keep the window comfortably above your longest silent command before tuning it down. A busy sibling neither masks a wedged child nor is cancelled as collateral when it trips. On expiry only the stalled child is cancelled (dependent stages that never started are skipped); the parent turn continues with the other children's real summaries plus a cancellation note per cancelled child — the session does not error. One exception: if the watchdog itself cannot observe the group at all (sustained failures of the activity probe against the agent backend), it falls back to cancelling the whole group — healthy children included — rather than leaving the parent turn hung on an unobservable backend.

### Usage

```bash
kiro-cli settings api.subagentTimeout 3600
```

**Type**: Number  
**Default**: `3600`  
**Unit**: Seconds  
**Scope**: Workspace-overridable

Set to `0` to disable the deadline. The `KIRO_SUBAGENT_STALL_TIMEOUT_MS` environment variable (milliseconds) takes precedence over this setting when present; if your configured value seems ignored, check for that variable in the launching environment.

## api.codewhisperer.service

CodeWhisperer service endpoint URL.

### Overview

Overrides the default CodeWhisperer service endpoint. Used for custom deployments or testing.

### Usage

```bash
kiro-cli settings api.codewhisperer.service '{"endpoint": "https://custom.endpoint.com", "region": "us-east-1"}'
```

**Type**: Object (JSON with `endpoint` and `region` fields)  
**Default**: None (uses default AWS endpoint)  
**Scope**: Global only

## api.q.service

Q service endpoint URL.

### Overview

Overrides the default Q service endpoint. Used for custom deployments or testing.

### Usage

```bash
kiro-cli settings api.q.service '{"endpoint": "https://custom.q.endpoint.com", "region": "us-east-1"}'
```

**Type**: Object (JSON with `endpoint` and `region` fields)  
**Default**: None (uses default AWS endpoint)  
**Scope**: Global only

## api.oidc.scopePrefix

OIDC scope prefix for authentication.

### Overview

Configures the prefix used for OIDC authentication scopes. Used for custom identity provider configurations.

### Usage

```bash
kiro-cli settings api.oidc.scopePrefix "custom-prefix"
```

**Type**: String  
**Default**: None  
**Scope**: Global only

## api.kiroauth.service

Kiro authentication service endpoint.

### Overview

Overrides the default Kiro authentication service endpoint. Used for custom deployments.

### Usage

```bash
kiro-cli settings api.kiroauth.service "https://custom.auth.endpoint.com"
```

**Type**: String  
**Default**: None (uses default endpoint)  
**Scope**: Global only

## Troubleshooting

### Request Timeouts

If you experience frequent timeouts:

```bash
# Increase timeout for slow connections
kiro-cli settings api.timeout 600000
```

### Custom Endpoint Issues

For custom endpoint configurations, ensure:
- The endpoint URL is accessible from your network
- The region matches your deployment
- Authentication credentials are valid for the custom endpoint

## Related

- [Settings Command](../commands/settings.md) - Managing all CLI settings
- [Default Model Settings](default-model.md) - AI model configuration
