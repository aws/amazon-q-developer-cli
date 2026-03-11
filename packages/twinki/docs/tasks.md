# Twinki — Open Tasks

Status legend: `[ ]` todo · `[~]` in progress · `[x]` done

---

## Completion Summary

The core rendering engine (phases 1–7) is complete and well-tested. The main gaps are:

1. `useFocus` / `useFocusManager` — hooks exist but are broken (local state, not shared context)
2. `@twinki/testing-library` — `lastFrame()` is implemented but async usage is broken (no await)
3. E2E test suite — almost entirely unwritten
4. `usePaste` hook — not implemented
5. `<Image>` component — not implemented
6. `@twinki/devtools` profiler panel — not implemented
7. Migration codemod — not implemented
8. npm publication — not done

---

## Gap 1: `useFocus` / `useFocusManager` — broken shared state

**File:** `packages/twinki/src/hooks/useFocusManager.ts`

`useFocusManager` creates a new `useRef`-based state object on every call. Each component that calls `useFocusManager()` (or `useFocus()`, which calls it internally) gets its own independent focus manager with its own `ids` array and `activeId`. There is no shared state between components.

Consequences:
- `focusNext()` / `focusPrevious()` only cycle through components that called `useFocusManager()` in the same render — which is never more than one.
- `useFocus({ autoFocus: true })` has a `queueMicrotask` with an empty body — it never actually focuses the component.
- `FocusManagerCtx` is defined but never provided or consumed.
- Focus state changes do not trigger re-renders (mutating a ref does not cause React to re-render).

**Fix:**
1. Create a `FocusManagerProvider` component that holds shared state in `useState` and provides it via `FocusManagerCtx`.
2. Mount `FocusManagerProvider` at the render root in `render.ts`.
3. `useFocusManager` reads from context instead of creating local ref state.
4. Wire `focus(id)` to call `tui.setFocus()` so the TUI's input dispatch routes to the focused component.
5. Fix `autoFocus` in `useFocus` to call `focus(id)` inside the `queueMicrotask`.

---

## Gap 2: `@twinki/testing-library` — `lastFrame()` is synchronous, render is async

**File:** `packages/testing-library/src/index.ts`

`render()` calls `terminal.flush()` synchronously after `twinkiRender()`. But `flush()` returns a Promise — the synchronous call discards it. The first frame may not be captured before `lastFrame()` is called.

Additionally, `lastFrame()` is synchronous but the underlying terminal write pipeline is async (React commit → `process.nextTick` → `terminal.write` → xterm parse). Tests that call `lastFrame()` immediately after `render()` will get an empty string.

**Fix:**
1. Make `render()` return a Promise (or provide an `async render()` variant) that awaits `terminal.flush()` before returning.
2. Alternatively, expose a `waitForRender()` method on the result that awaits the next frame.
3. Add a compatibility test: port a simple `@ink/testing-library` test and verify assertions pass.

---

## Gap 3: E2E test suite — rendering correctness

**Files:** `packages/twinki/test/` (new files needed)

The existing 318 tests cover unit behavior well, but the following integration scenarios have no test coverage:

- [ ] Differential update: verify `frame.isFull === false` and `frame.writeBytes` is proportional to changed lines only
- [ ] Width change: triggers full rerender with scrollback clear (`\x1b[3J`)
- [ ] Content shrink with `clearOnShrink`: ghost lines cleared
- [ ] Content shrink without `clearOnShrink`: no full rerender
- [ ] Every frame write contains `\x1b[?2026h` and `\x1b[?2026l` (synchronized output)
- [ ] Single `terminal.write()` call per frame
- [ ] Style reset: ANSI state on line N does not bleed into line N+1
- [ ] `<Static>` appends to scrollback, never appears in `previousLines`
- [ ] `<Static>` new items written before live region on subsequent renders
- [ ] `<Text color>` renders correct ANSI SGR codes through full pipeline
- [ ] `<Text wrap="truncate">` truncates at correct column
- [ ] `<Text wrap="truncate-start">` and `wrap="truncate-middle"` modes
- [ ] `<Box width={N}>` constrains child text wrapping at column N
- [ ] `<Box>` padding, margin, border rendering
- [ ] `<Box overflow="hidden">` clips children
- [ ] `<Newline count={N}>` produces N blank lines
- [ ] `<Spacer>` fills remaining flex space
- [ ] `<Transform>` applies transform to each line
- [ ] `useApp().exit()` resolves `waitUntilExit()`
- [ ] `exitOnCtrlC` default behavior

---

## Gap 4: E2E test suite — flicker and collision detection

**Files:** `packages/twinki/test/` (new files needed)

The flicker and collision analyzers exist but are not exercised in any test:

- [ ] `analyzeFlicker` returns clean for normal render sequences (spinner, streaming text)
- [ ] `analyzeFlicker` detects injected flicker (test the detector itself)
- [ ] Spinner animation: zero flicker events across 100+ frames
- [ ] Streaming text append: zero flicker events
- [ ] `analyzeCollisions` returns clean for overlay within declared bounds
- [ ] `analyzeCollisions` detects injected out-of-bounds overlay

---

## Gap 5: E2E test suite — overlay system

**Files:** `packages/twinki/test/` (new files needed)

Overlay compositing has unit tests but no full-pipeline E2E tests:

- [ ] All 9 anchor positions produce correct `row`/`col` in rendered output
- [ ] Percentage width (`"50%"`) resolves correctly at different terminal widths
- [ ] `maxHeight` truncates overlay at correct line count
- [ ] `visible()` callback: overlay hidden when returns false, shown when returns true
- [ ] `setHidden(true/false)`: overlay toggles, focus restored correctly on hide
- [ ] ANSI state: no style bleed from base content into overlay or from overlay into content after it
- [ ] Overlay wider than declared width: hard-truncated, no terminal width overflow

---

## Gap 6: E2E test suite — input pipeline

**Files:** `packages/twinki/test/` (new files needed)

- [ ] `useInput` handler fires on correct key
- [ ] `useInput` does not fire when `isActive: false`
- [ ] Key release events filtered by default
- [ ] Key release events passed when `wantsKeyRelease: true`
- [ ] Keypress → UI update latency < 10ms (via `measureInputLatency`)
- [ ] Multiple rapid state changes coalesce into single render

---

## Gap 7: `usePaste` hook — not implemented

**File:** `packages/twinki/src/hooks/usePaste.ts` (new file)

The `StdinBuffer` already emits a `paste` event for bracketed paste sequences. A `usePaste` hook needs to subscribe to this event and expose the pasted content to React components.

- [ ] Returns `{ paste: string | null }` — last pasted content
- [ ] Clears after one render cycle
- [ ] Works alongside `useInput` without conflict
- [ ] Export from `src/index.ts`

---

## Gap 8: `<Image>` component — not implemented

**File:** `packages/twinki/src/components/Image.tsx` (new file)

Terminal image rendering via Kitty graphics protocol and iTerm2 protocol. Requires cell dimension data from the `\x1b[16t` query that TUI already sends on startup.

- [ ] Props: `src: string | Buffer`, `width?: number`, `height?: number`, `preserveAspectRatio?: boolean`, `fallback?: string`
- [ ] Kitty graphics protocol: base64 PNG/JPEG, chunked transmission, image ID management
- [ ] iTerm2 protocol: base64 with size hints
- [ ] Text fallback when no image protocol available
- [ ] `calculateImageRows(widthCells, heightPx, cellHeightPx)`: compute cell height from pixel dimensions
- [ ] Export from `src/index.ts`

---

## Gap 9: `@twinki/devtools` profiler panel — not implemented

**Package:** `packages/devtools/` (new package)

In-process overlay panel toggled with `Shift+Ctrl+D`.

- [ ] Display: frame count, full redraws, last frame bytes, last frame time
- [ ] Display: `previousLines.length`, `maxLinesRendered`, `viewportTop`
- [ ] Display: React reconcile time, Yoga layout time
- [ ] Display: dirty region visualization (highlight changed lines in last frame)
- [ ] Package setup: `package.json`, `tsconfig.json`, exports

---

## Gap 10: Migration codemod — not implemented

**Package:** `packages/codemod/` or standalone script (new)

AST transform to migrate Ink projects to Twinki.

- [ ] Replace `from 'ink'` with `from 'twinki'` in all source files
- [ ] Replace `from '@ink/testing-library'` with `from '@twinki/testing-library'`
- [ ] Report known incompatibilities found in the codebase
- [ ] Dry-run mode: show changes without applying
- [ ] `npx twinki-migrate` CLI entry point

---

## Gap 11: npm publication — not done

- [ ] Verify `twinki` package: CJS + ESM dual build, type declarations
- [ ] Verify `@twinki/testing` package exports
- [ ] Verify `@twinki/testing-library` package exports
- [ ] CI: test on Node.js 20, 22, 24
- [ ] CI: test on macOS, Linux, Windows
- [ ] Changelog and semantic versioning setup
- [ ] Publish to npm registry
