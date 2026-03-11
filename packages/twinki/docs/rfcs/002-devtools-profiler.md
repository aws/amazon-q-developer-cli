# RFC-002: Devtools Profiler Panel

Status: Draft

## Summary

An in-process overlay panel toggled with `Shift+Ctrl+D` that displays real-time rendering metrics and state.

## Motivation

Debugging rendering performance and layout issues in terminal UIs is difficult. A built-in profiler panel provides immediate visibility into the rendering pipeline.

## Design

### Toggle

`Shift+Ctrl+D` is already wired in TUI's `handleInput` to call `this.onDebug`. The devtools panel would register as the debug handler.

### Display

```
┌─ Twinki Devtools ──────────────────┐
│ Frames: 142    Full redraws: 3     │
│ Last frame: 847B (diff)  1.2ms     │
│ Lines: 24  Max: 24  ViewportTop: 0 │
│ React reconcile: 0.3ms             │
│ Yoga layout: 0.8ms                 │
│ Dirty lines: [4, 5, 12]            │
└────────────────────────────────────┘
```

### Implementation

- Use the existing overlay system (`tui.showOverlay()`)
- Instrument `doRender()` with `performance.now()` timestamps
- Track per-frame: byte count, strategy (full/diff), changed line indices
- Expose metrics via a `TUI.getMetrics()` method

### Package

Separate package `@twinki/devtools` to keep core lean. Imported optionally:

```tsx
import { enableDevtools } from '@twinki/devtools';
enableDevtools(); // registers Shift+Ctrl+D handler
```

## Open Questions

- Should it show a React component tree?
- Should metrics be exportable (JSON dump)?
- Should it support custom panels/plugins?
