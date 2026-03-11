# Twinki — Design Document
## Page 2 of 5: Rendering Engine — High and Low Level Design

---

### 2.1 The Fundamental Model: Lines, Not Cells

The first architectural decision is the granularity of the diff. Two options exist:

**Cell-level diff**: Maintain a 2D grid of `{char, fg, bg, flags}` structs. Diff
cell by cell. Write only changed cells with precise cursor positioning. This is
what Ratatui, Textual, and most Rust/Python TUI frameworks do.

**Line-level diff**: Maintain an array of ANSI-encoded strings, one per terminal
row. Diff by string equality. Write only changed lines. This is what pi-tui does,
and what Twinki inherits.

The line-level approach is correct for a React-backed TUI for three reasons:

First, the diff is O(n) where n is the number of lines, with early exit on the
first differing character. For a 24-line terminal with 3 changed lines, the diff
scans approximately 264 character comparisons. At V8's ~1 billion operations per
second, this takes under 2μs. Cell-level diff on the same terminal would scan
1,920 cells — still fast, but with higher constant factors due to struct access.

Second, ANSI escape sequences are already line-scoped. SGR attributes (color, bold,
italic) apply to a run of characters and are reset at line boundaries. A line-level
model maps naturally to this: each line string carries its own ANSI state, and a
full SGR reset is appended to every line to prevent bleed.

Third, React's component model produces lines naturally. A `<Text>` component
renders to one or more line strings. A `<Box>` container concatenates its
children's line arrays. The line array is the natural output type of the component
tree.

The tradeoff: line-level diff cannot skip unchanged characters within a changed
line. If one character changes in a 220-column line, the entire line is rewritten.
For typical TUI content (text, borders, status indicators), this is acceptable.
For content with many small changes within long lines (e.g., a hex dump editor),
a cell-level diff would be more efficient. Twinki accepts this tradeoff.

---

### 2.2 The Component Interface

Every component in Twinki implements:

```typescript
interface Component {
    render(width: number): string[];
    handleInput?(data: string): void;
    invalidate(): void;
    wantsKeyRelease?: boolean;
}
```

`render(width)` returns an array of strings. Each string is one terminal line,
may contain ANSI SGR escape codes, and must not exceed `width` visible columns
as measured by `visibleWidth()`. The `width` parameter is the current terminal
width, passed down from the TUI root on every render cycle.

`invalidate()` clears any cached render output. Called when the theme changes,
when the terminal resizes, or when a parent forces a full re-render. Components
that cache their output (e.g., a completed markdown block that will never change)
return the cached value from `render()` until `invalidate()` is called.

`handleInput(data)` receives raw terminal bytes when the component has focus.
The component calls `matchesKey(data, keyId)` to interpret them.

`wantsKeyRelease` opts the component into receiving Kitty protocol key release
events. Default false. Used by components that need held-key detection (e.g.,
a game-style input handler).

---

### 2.3 The TUI Class: State Machine

`TUI extends Container`. The complete rendering state:

```typescript
class TUI extends Container {
    // Rendering state
    private previousLines: string[] = [];    // front buffer: what is on screen
    private previousWidth = 0;               // terminal width at last render
    private maxLinesRendered = 0;            // high-water mark of lines ever rendered
    private previousViewportTop = 0;         // viewport top at last render

    // Cursor tracking (two separate concepts)
    private cursorRow = 0;                   // logical end of rendered content
    private hardwareCursorRow = 0;           // actual terminal cursor position

    // Scheduling
    private renderRequested = false;         // debounce flag

    // Overlay stack
    private overlayStack: OverlayEntry[] = [];

    // Counters (for testing)
    public fullRedrawCount = 0;
}
```

The distinction between `cursorRow` and `hardwareCursorRow` is critical. After a
differential render that only updates lines 5-7 of a 10-line output, the hardware
cursor is at row 7. But `cursorRow` is 9 (the last line of content). The next
render must move the cursor from row 7 to wherever the first changed line is,
using the hardware cursor position as the reference point, not the logical cursor.

`maxLinesRendered` is the high-water mark. It grows as content is added but never
shrinks (unless a full clear is performed). This is used to compute `viewportTop`:

```
viewportTop = max(0, maxLinesRendered - terminalHeight)
```

As content grows beyond the terminal height, `viewportTop` advances. The TUI can
only write to lines within `[viewportTop, viewportTop + terminalHeight)`. Any
change above `viewportTop` (the user scrolled up) triggers a full clear and
rerender.

---

### 2.4 Render Scheduling: Event-Driven, Not Timer-Driven

```typescript
requestRender(force = false): void {
    if (force) {
        this.previousLines = [];
        this.previousWidth = -1;
        this.cursorRow = 0;
        this.hardwareCursorRow = 0;
        this.maxLinesRendered = 0;
        this.previousViewportTop = 0;
    }
    if (this.renderRequested) return;   // coalesce
    this.renderRequested = true;
    process.nextTick(() => {
        this.renderRequested = false;
        this.doRender();
    });
}
```

`process.nextTick` fires after the current synchronous operation completes but
before any I/O callbacks. Latency: 10–50μs. Multiple state changes within the
same event loop tick all call `requestRender()`, but only the first sets
`renderRequested = true`. The subsequent calls return immediately. One render
fires per tick, regardless of how many state changes occurred.

This is the correct scheduling primitive for TUI. `setImmediate` (50–200μs) is
too slow for 240Hz. `queueMicrotask` (1–10μs) is faster but risks starving the
event loop if called recursively. `process.nextTick` is the right balance.

The effective frame rate is `min(state_change_rate, terminal_throughput_fps)`.
There is no artificial cap. If state changes arrive at 200 per second, renders
happen at 200 per second. The terminal's write throughput is the actual limit.

---

### 2.5 The `doRender()` Algorithm: Complete Specification

The render algorithm has four strategies, evaluated in priority order:

**Strategy 1 — First render** (`previousLines.length === 0 && !widthChanged`):

Write all lines sequentially without any cursor movement or clearing. Assumes the
terminal is in a clean state (cursor at column 0, no prior content). This is the
only render that does not move the cursor before writing.

```
buffer = CSI ?2026h
for i in 0..newLines.length:
    if i > 0: buffer += CRLF
    buffer += newLines[i]
buffer += CSI ?2026l
terminal.write(buffer)
```

**Strategy 2 — Width changed** (`previousWidth !== currentWidth`):

Terminal width change invalidates all line wrapping. A line that was 80 characters
wide may now wrap differently at 60 or 120 characters. The entire output must be
recomputed and rewritten. This is the only strategy that clears the scrollback
buffer.

```
buffer = CSI ?2026h
buffer += CSI 3J    ← clear scrollback
buffer += CSI 2J    ← clear screen
buffer += CSI H     ← cursor to home (1,1)
for i in 0..newLines.length:
    if i > 0: buffer += CRLF
    buffer += newLines[i]
buffer += CSI ?2026l
terminal.write(buffer)
reset all tracking state
```

**Strategy 3 — Content shrunk below working area** (configurable):

When `newLines.length < maxLinesRendered` and no overlays are active, ghost lines
from previous renders may remain visible. A full clear prevents this. This strategy
is off by default (controlled by `setClearOnShrink(true)`) because it causes a
visible flash on slow terminals. Applications that need clean shrinkage (e.g., a
spinner that completes and disappears) enable it explicitly.

**Strategy 4 — Differential update** (the common case):

```
// Find changed range
firstChanged = -1, lastChanged = -1
for i in 0..max(newLines.length, previousLines.length):
    oldLine = previousLines[i] ?? ""
    newLine = newLines[i] ?? ""
    if oldLine !== newLine:
        if firstChanged === -1: firstChanged = i
        lastChanged = i

if firstChanged === -1:
    // No changes — only update hardware cursor if needed
    positionHardwareCursor(cursorPos)
    return

if firstChanged < previousContentViewportTop:
    // Change is above visible viewport — full rerender
    fullRender(clear: true)
    return

// Build differential buffer
buffer = CSI ?2026h
[move cursor from hardwareCursorRow to firstChanged]
buffer += CR                    ← column 0
for i in firstChanged..lastChanged:
    if i > firstChanged: buffer += CRLF
    buffer += CSI 2K            ← clear line
    buffer += newLines[i]
[clear extra lines if previousLines.length > newLines.length]
buffer += CSI ?2026l
terminal.write(buffer)          ← ONE syscall
```

The single `terminal.write(buffer)` call is non-negotiable. Multiple writes risk
partial frames even with synchronized output, because the terminal may process the
end-sync sequence before all writes have been flushed through the PTY.

---

### 2.6 Synchronized Output: The Atomic Frame Protocol

Every render — full or differential — is wrapped in DEC private mode 2026:

```
CSI ? 2 0 2 6 h   ← begin synchronized update (8 bytes)
... all writes for this frame ...
CSI ? 2 0 2 6 l   ← end synchronized update (8 bytes)
```

Total overhead: 16 bytes per frame. At 867 KB/s throughput, this adds 18.5μs —
negligible. The benefit: the terminal's GPU compositor defers all screen updates
until the end-sync sequence arrives, presenting the entire frame atomically.

Capability is queried at startup via `CSI ? 2026 $ p`. The terminal responds with
`CSI ? 2026 ; 1 $ y` (enabled), `CSI ? 2026 ; 2 $ y` (disabled but supported),
or no response (not supported). On unsupported terminals, the sequences are ignored
and the single-write approach still provides near-atomic updates because the OS
write buffer is large enough to hold a typical differential frame.

Support matrix (2026): Ghostty ✓, Kitty ✓, WezTerm ✓, iTerm2 ✓, Alacritty ✓,
Windows Terminal ✓, foot ✓, tmux (passthrough) ✓, macOS Terminal.app ✗, old xterm ✗.

---

### 2.7 The PTY Buffer Constraint

The kernel PTY buffer is typically 4,096 bytes on Linux and macOS. A `write()`
call that exceeds this size blocks until the terminal drains the buffer. This is
the real performance constraint for large terminals.

Measured frame sizes:

| Scenario | Bytes | PTY buffer | Blocking? |
|---|---|---|---|
| 80×24 full rewrite | 2,400 | 4,096 | No |
| 220×50 full rewrite | 13,000 | 4,096 | Yes — 15ms at 867 KB/s |
| 10 lines diff, 80 cols | 1,000 | 4,096 | No |
| 10 lines diff, 220 cols | 2,700 | 4,096 | No |
| 1 line diff, 80 cols | 130 | 4,096 | No |

The critical finding: a full rewrite of a large terminal (220×50) takes 15ms,
exceeding both the 120Hz budget (8.33ms) and the 240Hz budget (4.17ms). This
makes differential rendering not just an optimization but a correctness requirement
for high-refresh-rate operation on large terminals. Full rewrites must be minimized
to width-change events and explicit force-redraws.

---

### 2.8 The `visibleWidth()` Function: Hot Path Optimization

`visibleWidth()` is called on every line during rendering and diffing. It must be
fast. The implementation uses three layers:

**Layer 1 — Pure ASCII fast path** (covers ~80% of typical TUI content):
```typescript
let isPureAscii = true;
for (let i = 0; i < str.length; i++) {
    const code = str.charCodeAt(i);
    if (code < 0x20 || code > 0x7e) { isPureAscii = false; break; }
}
if (isPureAscii) return str.length;
```
Cost: O(n) with early exit. For a pure ASCII 80-char line: ~80 integer comparisons,
~80ns at V8 speeds.

**Layer 2 — LRU cache** (512 entries, covers repeated strings):
```typescript
const cached = widthCache.get(str);
if (cached !== undefined) return cached;
```
Cost: O(1) hash lookup, ~100ns. Cache hit rate for typical TUI content (status
bars, borders, repeated UI elements): ~70–80%.

**Layer 3 — Full Unicode calculation** (for ANSI-encoded and non-ASCII strings):
Strip ANSI SGR codes, OSC 8 hyperlinks, APC sequences. Use `Intl.Segmenter` to
split into grapheme clusters. For each grapheme: check zero-width regex, check
emoji regex (with codepoint-range pre-filter to avoid the expensive
`/^\p{RGI_Emoji}$/v` test), then call `eastAsianWidth()`.

The emoji pre-filter checks codepoint ranges (0x1F000–0x1FBFF, 0x2300–0x27BF,
etc.) before running the regex. This avoids the regex for the vast majority of
non-emoji characters while still correctly handling all emoji.

---

### 2.9 Full Pipeline Timing at 120Hz

Measured estimates for a typical coding agent UI (50 components, 24 lines, 80 cols,
10 lines changed per frame):

| Stage | Time |
|---|---|
| process.nextTick scheduling | 50μs |
| React Fiber reconcile (50 components) | 150μs |
| Yoga layout (50 nodes) | 150μs |
| Component render() calls | 100μs |
| Overlay compositing | 50μs |
| Line diff scan (24 lines) | 24μs |
| visibleWidth() calls (48×, cached) | 24μs |
| Buffer assembly (escape sequences) | 50μs |
| process.stdout.write (1KB diff) | 1,153μs |
| Terminal sync output processing | 300μs |
| **Total** | **2,051μs = 2.05ms** |

Frame budget at 120Hz: 8.33ms. Budget remaining: 6.28ms (75% headroom).
Frame budget at 240Hz: 4.17ms. Budget remaining: 2.12ms (51% headroom).

The write time (1,153μs) dominates the pipeline. All other stages combined total
898μs. Optimizing the diff algorithm or the React reconciler yields diminishing
returns compared to minimizing the number of bytes written per frame.

---

### 2.10 Overlay Compositing

Overlays (modals, autocomplete dropdowns, context menus) are rendered on top of
base content by splicing their line strings into the base line array at calculated
positions. The `compositeLineAt()` function performs this in a single pass:

1. Extract the "before" segment (columns 0 to `startCol`) from the base line,
   tracking ANSI SGR state.
2. Extract the "after" segment (columns `startCol + overlayWidth` to end),
   inheriting the SGR state from before the overlay region.
3. Truncate the overlay line to exactly `overlayWidth` visible columns.
4. Concatenate: `before + padding + SGR_RESET + overlay + padding + SGR_RESET + after`.
5. Hard-truncate the result to `terminalWidth` visible columns as a safety guard.

The SGR state tracking ensures that styling from before the overlay does not bleed
into the overlay, and styling from the overlay does not bleed into the after segment.
Each segment boundary emits `\x1b[0m\x1b]8;;\x07` (full SGR reset + OSC 8 hyperlink
reset) to guarantee clean state.

Overlay positions are resolved from an `OverlayOptions` object that supports
absolute coordinates, percentage-based positioning (`"50%"`), anchor-based
positioning (center, top-left, bottom-right, etc.), and margin constraints. All
values are resolved to absolute row/col at render time, clamped to terminal bounds.
