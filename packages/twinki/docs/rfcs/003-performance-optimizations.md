# RFC-003: Performance Optimizations

Status: Draft

## Summary

Future optimizations identified from gold-standard terminal UI patterns (2026). These are not needed for typical CLI apps but would benefit complex UIs with 1000+ components.

## Current Architecture

Twinki uses line-based differential rendering with synchronized output. This is sufficient for most terminal UIs — the diff is O(n) where n = visible lines, and string equality with early exit makes it fast.

## Proposed Optimizations

### 1. Spatial Culling

Grid-based widget culling to O(1) discard off-screen components during layout.

**When needed:** UIs with 1000+ components where Yoga layout becomes the bottleneck.

**Approach:** Partition the terminal into grid cells. Each component registers in cells it occupies. During render, only process components in visible cells.

### 2. Subtree Render Caching

Cache rendered output per component subtree. Skip re-rendering unchanged subtrees.

**When needed:** Deep component trees where most subtrees are static between frames.

**Approach:** Hash component props + children. If hash matches previous render, return cached lines. Invalidate on prop change or `requestRender(true)`.

### 3. Segment-Level Diff

Currently Twinki diffs at line level (string equality). Segment-level diff would identify changed character ranges within a line and emit minimal cursor movement + overwrites.

**When needed:** Wide terminals with long lines where only a few characters change (e.g., progress bars, status indicators).

**Approach:** After line-level diff identifies changed lines, do a character-level scan to find first/last changed column. Emit `CSI col G` + changed segment instead of full line rewrite.

### 4. Compositor Regions

Divide the screen into independent regions that can be updated separately without affecting other regions.

**When needed:** Complex layouts with independently updating panels (e.g., sidebar + main + status bar).

**Approach:** Each region tracks its own `previousLines` and renders independently. The TUI composites regions into a single frame buffer before diffing.

### 5. Adaptive Frame Pacing

Cap render rate and detect write pressure.

**When needed:** High-frequency updates (streaming, animations) that overwhelm the terminal.

**Approach:**
- `targetFps` option gates renders via `setInterval`
- Check `process.stdout.writableNeedDrain` before writing
- Coalesce rapid `requestRender()` calls within the frame budget

## Priority

1. Adaptive frame pacing — small, high impact for streaming use cases
2. Segment-level diff — moderate complexity, helps wide terminals
3. Subtree caching — moderate complexity, helps deep trees
4. Spatial culling — only for extreme cases
5. Compositor regions — architectural change, defer

## References

- Textual (Python) implements spatial culling and compositor regions
- Ink's performance issues documented in Claude Code issues #9935, #22408, #15980
