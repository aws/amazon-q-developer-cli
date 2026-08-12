---
doc_meta:
  title: Turn Usage Summary
  description: Displays credit usage and duration after each AI response
  category: feature
  keywords: [usage, credits, metering, cost, duration, time, turn, summary]
  related: [usage, context-usage-indicator]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
---

# Turn Usage Summary

After each AI response completes, Kiro displays a summary line showing credits used and time taken for that turn.

## Overview

The turn usage summary appears as a dimmed line below the AI's response, showing:
- Credits consumed during the turn
- Time elapsed for the turn

This helps you track resource consumption without running `/usage`.

## Display Format

The summary appears with an arrow indicator (▸) followed by metrics:

```
▸ Credits: 0.15 • Time: 5s
```

For longer turns:

```
▸ Credits: 1.23 • Time: 1m 30s
```

## What's Shown

| Metric | Format | Example |
|--------|--------|---------|
| Credits | 2 decimal places | `Credits: 0.15` |
| Time (< 60s) | Seconds | `Time: 5s` |
| Time (≥ 60s) | Minutes + seconds | `Time: 1m 30s` |

Multiple credit types are aggregated and shown separately if present.

## When It Appears

The summary displays:
- After the AI finishes responding
- Only when metering data is available
- For both active and historical turns (when loading saved sessions)

The summary does not appear:
- While the AI is still responding
- If no metering data was received from the backend

## Examples

### Example 1: Quick Response

```
You: What is 2+2?

Kiro: 2 + 2 = 4

▸ Credits: 0.02 • Time: 1s
```

### Example 2: Complex Task

```
You: Refactor this file to use async/await

Kiro: [... detailed response with code changes ...]

▸ Credits: 0.45 • Time: 23s
```

### Example 3: Long-Running Operation

```
You: Analyze this codebase and suggest improvements

Kiro: [... comprehensive analysis ...]

▸ Credits: 2.15 • Time: 2m 15s
```

## Related

- [/usage](../slash-commands/usage.md) - View cumulative session usage
- [Context Usage Indicator](../settings/context-usage-indicator.md) - Show context percentage in prompt

## Limitations

- Summary only appears when backend provides metering data
- Credit values depend on the model and operation type
- Historical sessions show summaries only if they were recorded

## Technical Details

The summary aggregates all metering events received during a turn. Multiple credit entries of the same unit type are summed together. Credit values are floored (not rounded). The 'Credits' label comes from the backend unit type field. Duration is measured client-side via local timestamp subtraction, representing the elapsed wall-clock time from turn start to completion.
