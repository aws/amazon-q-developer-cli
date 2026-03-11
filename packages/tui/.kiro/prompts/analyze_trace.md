# Find Top Offenders

Analyze CPU profile to find performance bottlenecks.

## What would you like to do?

1. **Analyze latest profile** (default) - Generate HTML report from most recent profile
2. **Compare two profiles** - Diff baseline vs current to find regressions

Please specify which option, or press enter for default (1).

---

## Option 1: Analyze Latest Profile

```bash
cd packages/tui && bun run scripts/analyze-profile.ts --html
```

Then open the HTML report in browser to see:
- Top 5 hot paths with caller/callee trees
- Top 20 functions by CPU time (click rows to expand)
- Whether each is our code (🟢), dependency (🟡), or runtime (⚪)
- Sparkline trend showing hotness over 10-second windows

## Option 2: Compare Two Profiles

```bash
cd packages/tui && bun run scripts/diff-profiles.ts reports/baseline.json reports/current.json
```

Shows regressions (🔴) and improvements (🟢) between two profiles.

---

## Reading the Report

### Function Types
- 🟢 **ours** - Our code in `packages/tui/src/`, investigate and optimize
- 🟡 **dep** - Dependency in `node_modules/`, consider caching/memoization/alternatives
- ⚪ **rt** - Runtime/built-in, usually not actionable

### Columns
- **Self Time** - Time spent in the function's own code
- **Total Time** - Time including all functions it calls
- **Calls** - Number of times the function was sampled
- **Trend** - Sparkline showing CPU usage over 10-second windows

### Interpreting Trends
- `▁▁▁▁▁▁▁▁` = idle, no CPU usage
- `▁▂▃▄▅▆▇█` = increasing load over time
- `████████` = sustained high CPU
- Spikes indicate specific user actions causing load

### Hot Paths
Click any row in the HTML report to see:
- **Called by** - Which functions call this one
- **Calls** - Which functions this one calls

This helps trace the call chain to find the root cause.
