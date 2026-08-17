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

Set to `0` to disable the deadline. The `KIRO_SUBAGENT_STALL_TIMEOUT_MS` environment variable (milliseconds) takes precedence over this setting when present; if your configured value seems ignored, check for that variable in the launching environment.
