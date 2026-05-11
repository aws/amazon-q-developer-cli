# Raw Mode Rendering — Research and Design

Status: Research  
Date: 2026-05-11

This document investigates how to implement a raw-mode rendering path in the V2 TypeScript stack that behaves like V1's Rust/crossterm output: direct stdout writes, incremental flush, content never exceeds the viewport, and interactive input (Emacs keybindings, history, slash commands) just works.

---

## What "Raw Mode" Means Here

The term is overloaded. In this context it does not mean "disable terminal cooked mode" — that is already done by Twinki at startup (`process.stdin.setRawMode(true)`). It means a rendering mode where:

1. Output is written to stdout incrementally, one logical chunk at a time, as content arrives.
2. The live region (the part Twinki re-renders on every tick) never grows beyond the viewport height.
3. Completed content is committed to terminal scrollback immediately and never touched again.
4. The input prompt appears inline, not pinned at the bottom of a fixed layout.
5. No Yoga layout, no Box/Text component tree, no differential renderer for the content that has already been committed.

This is exactly what V1 does: `crossterm` writes to stdout directly, each completed message is a `println!`, and the only "live" region is the current streaming line and the readline prompt.

---

## How Twinki's Rendering Pipeline Works (Relevant Parts)

Understanding the pipeline is essential before deciding where to intervene.

### The TUI class and `previousLines`

`TUI` maintains `previousLines: string[]` — the front buffer of what is currently on screen in the live region. On every render tick, it computes `newLines` from the React component tree, diffs against `previousLines`, and writes only the changed lines. The live region is bounded by `maxLinesRendered` and the terminal height.

### Static content

`<Static items={...}>` is the mechanism for committing content to scrollback. When the React reconciler processes a `twinki-static` node, `ReactBridge.render()` calls `tui.writeStaticLines(lines)`. These lines are written to stdout immediately and are never part of `previousLines`. They are gone from Twinki's perspective — the terminal emulator owns them.

`writeStaticLines` writes the lines above the current live region by moving the cursor up, writing, and repositioning. The `totalStaticWritten` monotonic cursor ensures each item is written exactly once even if the `items` array is mutated.

### The `wideLines` option

When `wideLines: true`, the renderer tracks physical rows (accounting for soft-wrapped lines) for cursor positioning. This is required for `wrap="overflow"` content. It adds a small per-render cost.

### What the React reconciler adds

The reconciler (Yoga layout + React Fiber) runs on every state change. For a simple streaming text display, this is overhead: Yoga computes a layout for a tree of `<Box>` and `<Text>` nodes, the tree-renderer walks the tree to produce line strings, and the diff engine compares them. For content that is just `process.stdout.write(chunk)`, none of this is needed.

---

## The Core Insight: Two Rendering Regimes in One Process

The key observation is that Twinki's `writeStaticLines` is already a raw write — it bypasses the diff engine entirely. The live region (managed by `previousLines`) is the only part that goes through the full React/Yoga/diff pipeline.

Classic mode can exploit this split:

- **Committed content** (completed messages, tool calls, approvals, command output): written via `writeStaticLines` directly, bypassing React rendering entirely for those lines.
- **Live region** (the one thing currently changing: streaming text, thinking spinner, or input prompt): kept small — at most a few lines — and rendered through the normal Twinki path.

This means the React component tree for classic mode only ever needs to describe the live region. The committed content never enters the React tree at all.

---

## Option A: Pure `<Static>` Approach (Current Design Doc)

The approach described in the existing design documents uses `<Static items={...}>` for committed content and a small live React component for the active element.

**How it works:** Every completed message, tool call, and command output is pushed into a `ConversationItem[]` array. `<Static items={conversationItems}>` renders them. The live region is a small React component (`ClassicStreamingRegion`) that renders only the current streaming content.

**What Twinki does:** On each React render, `renderTree` walks the `twinki-static` node, finds items with index >= `totalStaticWritten`, renders them to line strings, and calls `writeStaticLines`. The live region is whatever is outside the static node.

**Viewport safety:** The live region is bounded by the number of lines in `ClassicStreamingRegion` — typically 1–5 lines for a spinner or partial streaming text. This never exceeds the viewport.

**Limitation:** The React reconciler still runs on every streaming token to check whether new static items need to be written. For a fast-streaming model (100+ tokens/sec), this means 100+ React reconciler ticks per second, each doing a Yoga layout pass over the entire component tree. For a small tree (5–10 nodes), this is ~300μs per tick — acceptable but not free.

---

## Option B: Direct `writeStaticLines` Bypass (Raw Mode)

A more aggressive approach bypasses React entirely for committed content. Instead of putting completed messages into a `<Static>` array, the classic layout calls `tui.writeStaticLines(lines)` directly from a Zustand store subscription.

**How it works:**

```
Zustand store change (message finalized)
  → store subscriber in ClassicLayout
    → renderMessageToLines(message)  // pure function, no React
      → tui.writeStaticLines(lines)  // direct write, bypasses reconciler
```

The React component tree for classic mode shrinks to just the live region: a spinner or a few lines of streaming text plus the input prompt. The reconciler only runs when the live region changes, not when a message is finalized.

**Accessing `tui` from a store subscriber:** The `tui` instance is available via `useTwinkiContext()` (which returns `{ tui, exit, adjustStaticCursor }`). In a store subscriber (not a React component), it can be accessed by storing the context value in a ref during the component's mount and passing it to the subscriber.

**Viewport safety:** `writeStaticLines` writes above the live region. The live region is whatever the React tree renders — kept to a few lines. The viewport constraint is automatically satisfied.

**Tradeoff:** This approach requires `renderMessageToLines` to be a pure function that produces ANSI-encoded line strings without going through React/Yoga. For simple text this is straightforward (chalk + string operations). For markdown it requires calling the markdown renderer outside of React context, which is possible since `MarkdownRenderer` ultimately calls `markdown.ts` utilities that are pure functions.

---

## Option C: Twinki `rawWrite` API (New Twinki Capability)

The cleanest long-term solution is a first-class `rawWrite(lines: string[])` method on the `Instance` returned by `render()`. This is essentially `writeStaticLines` exposed as a public API.

Currently `writeStaticLines` is a method on the `TUI` class (not on `Instance`). Adding it to `Instance` is a one-line change:

```ts
// In render.ts, Instance interface:
rawWrite(lines: string[]): void;

// Implementation:
rawWrite(lines: string[]) {
  tui.writeStaticLines(lines);
  tui.requestRender();
},
```

This gives classic mode a clean, stable API: call `instance.rawWrite(lines)` whenever content is finalized, and let the React tree handle only the live region. No internal Twinki APIs are accessed, no coupling to `ReactBridge` internals.

**This is the recommended approach.** It is minimal (one method on `Instance`), safe (uses the existing `writeStaticLines` path), and gives classic mode a clean contract.

---

## Option D: Headless React + Direct stdout (No Twinki for Content)

The most radical option: bypass Twinki entirely for content rendering. Use Twinki only for the input prompt (which needs raw mode stdin and the Emacs keybinding layer). Write content directly to `process.stdout`.

```
process.stdout.write(chalk.green('> ') + userMessage + '\n');
process.stdout.write(chalk.dim('⚙ read_file src/main.rs … done\n'));
process.stdout.write(renderedMarkdown + '\n');
```

The input prompt is a single Twinki component (`<PromptInput />`) rendered in a 1-line live region at the bottom of the viewport.

**Why this is attractive:** It is exactly what V1 does. Zero overhead for content rendering. Perfect scrollback behavior. Works with `less`, `tee`, pipes.

**Why it is problematic:** Twinki owns the terminal (raw mode, bracketed paste, cursor visibility). Writing to `process.stdout` while Twinki is running risks cursor positioning conflicts. Twinki's differential renderer tracks `hardwareCursorRow` — a direct `process.stdout.write` moves the cursor without updating that tracking state, causing the next Twinki render to position the cursor incorrectly.

The fix is to route all writes through `tui.writeStaticLines()`, which is exactly Option C. Option D collapses into Option C.

---

## Recommendation: Option C with a Thin React Live Region

The implementation should:

1. Add `rawWrite(lines: string[]): void` to the `Instance` interface in `packages/twinki/packages/twinki/src/reconciler/render.ts`. This is a one-line addition to the existing `Instance` object.

2. In `ClassicLayout`, subscribe to the Zustand store for finalized messages. When a message is finalized, call `instance.rawWrite(renderToLines(message))` directly. Do not put finalized messages into a `<Static>` array.

3. Keep the React component tree for classic mode to the absolute minimum: a `<ClassicLiveRegion>` that renders only the currently-active element (spinner, streaming text, or input prompt). This tree has 5–10 nodes and runs the reconciler only when the live element changes.

4. The live region must never exceed `terminalHeight - 2` lines. For streaming text, use the paragraph-based flush strategy from the existing design: when a paragraph boundary is detected, call `instance.rawWrite(flushedLines)` and clear the live region to just the remaining partial content.

---

## Viewport Safety: The Invariant

The invariant that "content never exceeds the viewport" is maintained by the following rule:

The React component tree for classic mode renders at most `MAX_LIVE_LINES` lines at any time. `MAX_LIVE_LINES` is set to `Math.min(terminalHeight - 2, 10)`. The `-2` reserves one line for the input prompt and one line of margin.

During streaming, the live region grows as tokens arrive. When the live region would exceed `MAX_LIVE_LINES`, the oldest complete paragraph is flushed via `rawWrite` and removed from the live region state. This keeps the live region bounded.

The `rawWrite` path writes above the live region (Twinki handles the cursor repositioning). The terminal scrollback grows naturally. The user can scroll up to see earlier content. The live region stays at the bottom of the viewport.

---

## Emacs Keybindings and Interactive Input

`PromptInput` already implements the full Emacs keybinding set (Ctrl+A, Ctrl+E, Ctrl+K, Ctrl+U, Alt+F, Alt+B, etc.) via `input-editing.ts`. It uses `useKeypress` which hooks into Twinki's input pipeline. This works identically in classic mode because the input pipeline is independent of the rendering mode.

The only difference in classic mode is that `PromptInput` is rendered inline (not wrapped in `PromptBar`) and is hidden while `isProcessing` is true. The keybinding behavior is unchanged.

Slash command autocomplete (`CommandMenu`) is a Twinki overlay in modern mode. In classic mode, it is replaced by inline text completion: as the user types `/`, matching commands are shown as a single line below the prompt (not a floating menu). This is a simpler component that renders in the live region.

---

## Changes Required

### In Twinki (`packages/twinki/packages/twinki/src/reconciler/render.ts`)

Add to `Instance` interface:
```ts
/** Write lines directly to terminal scrollback, bypassing the React tree. */
rawWrite(lines: string[]): void;
```

Add to `instance` object:
```ts
rawWrite(lines: string[]) {
  tui.writeStaticLines(lines);
  tui.requestRender();
},
```

That is the entire Twinki change. One method, two lines of implementation.

### In the TUI package (`packages/tui/`)

Everything else is in the TUI package, following the existing design documents. The key difference from the previous design is:

- `ClassicConversation` does **not** use `<Static items={...}>`. Instead it calls `instance.rawWrite()` directly from a store subscription when messages are finalized.
- The React component tree for classic mode contains only `ClassicLiveRegion` and `ClassicPromptLine` — no `ClassicConversation` React component at all.
- `renderMessageToLines(message): string[]` is a pure TypeScript function (no React) that converts a finalized message to ANSI-encoded line strings using chalk and the existing markdown utilities.

### Updated file map

```
packages/twinki/packages/twinki/src/reconciler/render.ts   MODIFY — add rawWrite to Instance
packages/tui/src/components/layout/classic/
  ClassicLayout.tsx          — mounts store subscription, calls rawWrite on finalization
  ClassicLiveRegion.tsx      — React component: spinner | streaming | prompt
  ClassicPromptLine.tsx      — PromptInput rendered inline
  ClassicToolCall.tsx        — one-line tool call renderer (live region only)
  ClassicApproval.tsx        — inline approval prompt (live region only)
  renderToLines.ts           — pure function: message → string[]
  ClassicCommandFormatter.ts — pure function: command result → string[]
  useClassicFlush.ts         — paragraph-based flush hook for streaming
  index.ts
```

`ClassicConversation.tsx` from the previous design is replaced by the `rawWrite` subscription in `ClassicLayout.tsx` and the `renderToLines.ts` pure function. The React tree is smaller and the reconciler runs less often.

---

## Summary of Rendering Modes

| Aspect | Modern (InlineLayout) | Classic (proposed) |
|---|---|---|
| Content rendering | React tree → Yoga → diff → write | `rawWrite()` → direct scrollback write |
| Live region | Full viewport (ConversationView + PromptBar) | ≤10 lines (spinner/streaming/prompt) |
| React reconciler runs | On every state change | Only when live region changes |
| Viewport overflow possible? | No (VirtualScrollList manages it) | No (rawWrite writes above live region) |
| Scrollback | Via `<Static>` | Via `rawWrite` (same underlying path) |
| Input | PromptBar (pinned bottom) | PromptInput (inline, after last output) |
| Emacs keybindings | PromptInput | PromptInput (identical) |
| Slash command autocomplete | CommandMenu overlay | Inline single-line suggestion |
| Pipe-safe | No (spinners, colors) | Yes (NO_COLOR + !isTTY guards) |
