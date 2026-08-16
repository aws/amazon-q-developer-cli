---
doc_meta:
  validated: 2026-02-25
  commit: d6d0a7b9
  status: validated
  testable_headless: true
  category: settings-group
  title: API and Service Settings
  description: Settings for API timeouts and service configurations
  keywords: [settings, api, timeout, service, network]
---

# API and Service Settings

Configure API timeouts and service-related settings for Kiro CLI.

## api.timeout

API request timeout in milliseconds.

### Overview

Sets the timeout duration for API requests made by Kiro CLI. Affects requests to AI models, external services, and other network operations.

### Usage

```bash
kiro-cli settings api.timeout 30000
```

**Type**: Number  
**Default**: `30000`  
**Unit**: Milliseconds

### Examples

```bash
# Increase for slow connections
kiro-cli settings api.timeout 60000

# Decrease for fast networks
kiro-cli settings api.timeout 15000

# Check current timeout
kiro-cli settings api.timeout
```

### Timeout Guidelines

- **15000**: Fast networks, quick failure detection (15s)
- **30000**: Balanced (default, 30s)
- **60000**: Slow networks, complex requests (60s)
- **120000**: Very slow networks or large requests (120s)

### Use Cases

**Increase timeout for**:
- Slow internet connections
- Complex AI model requests
- Large file operations
- Unstable networks

**Decrease timeout for**:
- Fast, reliable connections
- Quick failure detection
- Interactive responsiveness
## api.streamIdleSoftTimeout

Stream idle soft timeout in seconds.

### Overview

Inter-event silence threshold after which Kiro CLI shows a "stream stalled" warning while continuing to wait for the response stream. Measures the gap between consecutive stream events (never total response time), so long responses with steady output never trigger it.

In the classic (V1) CLI these thresholds govern the delegated subagent engine's response streams; the main chat loop has its own streaming timeout (`api.timeout`).

### Usage

```bash
kiro-cli settings api.streamIdleSoftTimeout 60
```

**Type**: Number
**Default**: `60`
**Unit**: Seconds

Set to `0` to disable the warning.

Note the unit: seconds, unlike `api.timeout` which is milliseconds. Values above 3600 (one hour) are clamped to 3600 with a logged warning, so a milliseconds-shaped value cannot silently disable the watchdog.

Must resolve below `api.streamIdleHardTimeout` to ever fire: an explicitly configured value at or past the hard timeout disables the warning tier (with a logged warning), while the inherited default is clamped to half the hard timeout if only the hard cap is lowered.

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

Set to `0` to disable the client-side stream deadline.

Note the unit: seconds, unlike `api.timeout` which is milliseconds. Values above 3600 (one hour) are clamped to 3600 with a logged warning, so a milliseconds-shaped value cannot silently disable the watchdog.
