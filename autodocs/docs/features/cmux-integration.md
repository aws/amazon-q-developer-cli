---
doc_meta:
  title: cmux Integration
  description: Agent status reporting in the cmux sidebar when running inside cmux
  category: feature
  keywords: [cmux, sidebar, status, integration, multiplexer]
  related: [terminal-progress-indicator]
  validated: 2026-03-24
  commit: 88f4aec4
  status: validated
  testable_headless: false
---

# cmux Integration

When running inside cmux, Kiro automatically reports agent status to the cmux sidebar. This gives you at-a-glance visibility into what the agent is doing without switching focus.

## Overview

The integration is automatic—no configuration needed. Kiro detects cmux by checking for `CMUX_WORKSPACE_ID` and `CMUX_SURFACE_ID` environment variables, then uses the `cmux` CLI to update the sidebar.

All status updates are fire-and-forget. Failures are silently ignored so they never affect your chat experience.

## Status States

| Status | Icon | Color | When |
|--------|------|-------|------|
| thinking… | 🧠 brain | purple | Agent is generating a response |
| running tool | 🔨 hammer | orange | Agent is executing a tool |
| compacting | 📦 archive | blue | Conversation is being compacted |
| needs approval | ✋ hand | yellow | Waiting for tool approval |
| error | ⚠️ warning | red | An error occurred |
| ready | ✓ check | green | Idle, waiting for input |

## Opt Out

Disable cmux integration by setting the environment variable:

```bash
export KIRO_NO_CMUX=1
```

## Requirements

- Running inside cmux (detected via `CMUX_WORKSPACE_ID` and `CMUX_SURFACE_ID`)
- `cmux` CLI available at one of:
  - `/Applications/cmux.app/Contents/Resources/bin/cmux`
  - `/usr/local/bin/cmux`
  - Anywhere in `PATH`

## Examples

### Normal Workflow

When you send a message:
1. Sidebar shows "thinking…" with brain icon (purple)
2. If tools run, shows "running tool" with hammer icon (orange)
3. If approval needed, shows "needs approval" with hand icon (yellow)
4. When complete, shows "ready" with check icon (green)

### During Compaction

When context is compacted:
1. Sidebar shows "compacting" with archive icon (blue)
2. Progress bar shows "Compacting conversation…"
3. Returns to "ready" when done

### On Error

If an error occurs:
1. Sidebar shows "error" with warning icon (red)
2. Progress bar is cleared

## Troubleshooting

### Status not appearing in sidebar

1. Verify you're running inside cmux:
   ```bash
   echo $CMUX_WORKSPACE_ID $CMUX_SURFACE_ID
   ```
   Both should have values.

2. Check cmux CLI is available:
   ```bash
   which cmux
   ```

3. Verify opt-out is not set:
   ```bash
   echo $KIRO_NO_CMUX
   ```
   Should be empty or unset.

### Status stuck on old state

Status updates are deduplicated. If the same status is sent twice without a detail change, the second is skipped. This is normal behavior to reduce overhead.

## Related

- [Terminal Progress Indicator](terminal-progress-indicator.md) - Progress in terminal tab/title bar
