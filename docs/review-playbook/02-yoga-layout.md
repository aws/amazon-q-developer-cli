# Review 2 — Yoga layout edge cases

**Why this class matters.** Yoga layout is the single largest source of pathological CPU/memory in this codebase. The failure modes are all edge cases at the boundaries: zero width, negative width, width exceeding container, empty children, or misclamped measure functions. Prior fixes have shown that a single un-guarded `wrapTextWithAnsi(text, 0)` can produce millions of string allocations per render and drive RSS past 6 GB.

**Scope.** Any code that (a) computes sizes for a layout engine or text wrapper, (b) implements a measure callback the layout engine calls back into, or (c) renders text given a width constraint. This covers yoga calls (`calculateLayout`, `setWidth`, `setHeight`, `setMeasureFunc`), text-wrap utilities (`wrapTextWithAnsi`, `breakLongWord`, `renderText`, `renderLines`), and any component prop that flows into those paths (`width`, `minWidth`, `maxWidth`, `flexShrink`, `flexGrow`, `padding`, `margin`).

Concrete entry points include twinki's `node-factory.ts`, `text-renderer.ts`, `wrap-ansi.ts`, `tree-renderer.ts`, and any component in `packages/tui/src/components/` that sets explicit dimensions. Any future text-rendering utility (code-block renderer, diff viewer, table renderer, markdown inline renderer) must be reviewed the same way.

## Techniques

1. **[code] Width-boundary audit.** Grep for every call site that passes a width into yoga or text-wrapping utilities: `rg -n "calculateLayout|setWidth|setMeasureFunc|wrapTextWithAnsi|renderText" packages`. For each, verify the width is clamped to `>= MIN_LAYOUT_WIDTH` (currently 10) before use. If no clamp exists, the caller must document why.

2. **[code] Measure-function audit.** Find every `setMeasureFunc(...)` or measure callback. For each, check the returned dimensions cannot exceed the container (this is the bug that produced maxWidth=0 children in #2156). A measure function that returns unbounded `Infinity` should be reviewed line-by-line.

3. **[code] Empty-input audit.** For each text utility (`wrapTextWithAnsi`, `breakLongWord`, `renderText`, `renderLines`, etc.) check the behaviour when the input is empty, undefined, or has width less-than-or-equal-to zero. Each should return a shared frozen singleton (e.g., `EMPTY_LINES`) — not allocate a fresh `[]` or `[""]` per call.

4. **[code] Hot-path allocation scan.** Open the top 20 functions from the latest cpuprofile (`docs/bun-performance-analysis.md` has the approach). For each twinki/yoga function with high self-time, read the source and count allocations per call: new arrays, new strings, new objects. Prefer reuse.

5. **[code] Per-character loop scan.** Grep for per-character iteration inside the text pipeline: `for` loops over `text.length`, `.split('')`, spread into array. Per-character loops that allocate a string per char are the pattern behind #2156.

6. **[code] Flex overflow scan.** Grep for `flexShrink`, `flexGrow`, `width=`, `minWidth=`, `maxWidth=` in `packages/tui/src/components/`. Flag any container that has children wider than itself without `flexShrink={1}` (StatusBar fix #4eb0f04f5).

7. **[code] Regression test inventory.** Inspect `packages/twinki/packages/twinki/test/` for `zero-width`, `oom`, `yoga-overflow`, `resize-memory` tests. Each newly discovered edge case should be matched by a red-then-green regression test.

8. **[blackbox] Bounded-memory unit probe.** A test that calls `renderText(text, width)` for width in the set of -1, 0, 1, 2, 5, 10 in a loop 10 000 times and asserts `process.memoryUsage().rss < 50MB`. This is exactly the pattern in `yoga-zero-width-oom.test.ts` — extend it to any other hot utility.

9. **[blackbox] Live-terminal squeeze probe.** Manually drag the terminal window from full width down to 1 column and back, 10 times. Monitor `ps -o rss` of the TUI process. Memory must return to baseline. If it grows monotonically, finding.

10. **[blackbox] Yoga input fuzz.** Generate random flex trees (random width/height, random child count 0 to 20, random padding, random text content including ANSI escapes and CJK) and render each. Assert no RangeError, no RSS > 100 MB, and wall time < 500 ms per render. Persist failing trees as corpus entries.

11. **[blackbox] Text-content fuzz.** For each text utility, fuzz with random inputs: empty, single char, 10 MB single line, text containing every Unicode category (RTL, combining marks, wide chars, ZWJ, tags, control chars, lone surrogates). Assert termination within 100 ms and bounded RSS.

12. **[blackbox] ANSI-injection probe.** Feed text containing malformed or nested ANSI escape sequences through the render pipeline. The renderer must strip, normalise, or reject them without crashing, and without producing output that could break terminal state.

## What to record

Finding, utility, boundary condition that triggers it, existing guard (if any), missing guard, regression test status.

## Done criteria

Every yoga-facing width parameter is clamped at its entry point. Every empty-input path returns a shared singleton. There is a regression test for each of: width=0, width<0, text="", text=undefined, child width > container width. Fuzz probes (techniques 10, 11, 12) run in CI without findings.
