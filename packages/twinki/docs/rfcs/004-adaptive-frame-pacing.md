# RFC-004: Adaptive Frame Pacing

Status: Draft

## Summary

Intelligent render throttling that adapts to terminal throughput, preventing write buffer saturation and ensuring the user always sees the most recent state.

## Problem

Twinki currently renders on every `requestRender()` via `process.nextTick` debounce. This works well for low-frequency updates (keyboard input, occasional state changes) but breaks down under high-frequency updates:

- **Streaming text** (LLM token output): 50-200 tokens/sec = 50-200 renders/sec
- **Progress bars**: continuous updates
- **Live data feeds**: real-time metrics, logs

When renders outpace terminal consumption:
1. Node's stdout write buffer fills up (`writableNeedDrain` = true)
2. Writes start blocking or queueing
3. User sees stale frames — the terminal is still painting frame N while the app is on frame N+50
4. In multiplexers (tmux, screen), this causes scroll event flooding (the Claude Code problem)

## Design

### Frame Budget

A `targetFps` option sets the maximum render rate:

```typescript
const tui = new TUI(terminal, { targetFps: 30 });
```

Implementation: `requestRender()` checks if enough time has elapsed since the last render. If not, it schedules a timer for the remaining budget instead of `process.nextTick`.

```
requestRender() called
  → time since last render >= frameBudget?
    → yes: render immediately via process.nextTick
    → no: schedule timer for (frameBudget - elapsed), set pendingRender flag
         subsequent requestRender() calls are no-ops while pendingRender is set
```

This means the latest state always renders — intermediate states are naturally coalesced.

### Write Pressure Detection

Before writing a frame, check stdout's buffer state:

```typescript
if (process.stdout.writableNeedDrain) {
  // Terminal hasn't consumed previous frame yet
  // Skip this render, schedule retry after drain
  process.stdout.once('drain', () => this.requestRender());
  return;
}
```

This prevents buffer saturation without a fixed FPS cap — the system self-throttles based on actual terminal throughput.

### Adaptive Rate

Instead of a fixed `targetFps`, measure round-trip render time and adjust:

```
renderStart = performance.now()
terminal.write(frame)
// After write completes:
renderTime = performance.now() - renderStart
// Exponential moving average
avgRenderTime = 0.8 * avgRenderTime + 0.2 * renderTime
// Set budget to 2x average render time (headroom)
frameBudget = Math.max(avgRenderTime * 2, 1000 / maxFps)
```

This means:
- Fast local terminal: budget ~2ms, effective ~500fps (capped by maxFps)
- SSH connection: budget ~50ms, effective ~20fps
- tmux with slow pane: budget ~100ms, effective ~10fps

### Metrics

Expose pacing metrics for observability:

```typescript
interface FramePacingMetrics {
  framesRendered: number;
  framesSkipped: number;      // coalesced away
  framesDrainDeferred: number; // skipped due to write pressure
  avgFrameBudgetMs: number;
  effectiveFps: number;
  writePressureEvents: number;
}
```

### Integration with Existing Debounce

Current `requestRender()` uses `process.nextTick` for same-tick coalescing. Frame pacing adds a second layer:

```
State change → requestRender()
  → nextTick debounce (coalesce within same tick)
    → frame budget check (coalesce across ticks)
      → write pressure check (defer if terminal is behind)
        → doRender()
```

The nextTick debounce stays — it handles the common case of multiple synchronous state changes. Frame pacing handles the temporal case of rapid async updates.

### Configuration

```typescript
interface FramePacingOptions {
  targetFps?: number;        // Max FPS (default: unlimited)
  adaptiveRate?: boolean;    // Auto-adjust based on terminal speed (default: false)
  maxFps?: number;           // Hard ceiling for adaptive mode (default: 120)
  writePressure?: boolean;   // Respect writableNeedDrain (default: true)
}
```

## Open Questions

- Should `targetFps` be settable at the `render()` level or only on TUI?
- Should we expose a `skipFrame()` API for apps that know they're about to batch updates?
- How to handle `requestRender(force=true)` — should it bypass pacing?
- Should the devtools panel (RFC-002) show pacing metrics?

## Testing Strategy

- Unit test: verify frame coalescing (N requestRender calls → 1 doRender)
- Unit test: verify write pressure deferral (mock writableNeedDrain)
- E2E test: stream 1000 updates, verify frame count << 1000
- E2E test: measure effective FPS under load matches targetFps
- Benchmark: compare byte throughput with/without pacing
