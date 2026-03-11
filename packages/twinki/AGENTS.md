# Development Rules

## First Message

If the user did not give a concrete task, read these files first:
- `README.md` — project overview
- `TODO.md` — current implementation status and next steps
- `TASKS.md` — full task breakdown across all phases

Then ask which area to work on.

## Project Structure

```
packages/
  twinki/           Core rendering engine (the main package)
  testing/          @twinki/testing — VirtualTerminal, frame capture, analyzers
  testing-library/  @twinki/testing-library — Ink-compatible test API (WIP)
examples/           Runnable sample apps (01-hello through 06-chat)
docs/               Design documents (5 pages)
```

Key source directories in `packages/twinki/src/`:
- `terminal/` — Terminal interface, ProcessTerminal
- `input/` — StdinBuffer, key matching, Kitty protocol
- `renderer/` — TUI class (the core rendering engine)
- `reconciler/` — React reconciler + Yoga layout bridge
- `components/` — Text, Box, Static, Newline, Spacer, Transform
- `hooks/` — useInput, useApp, useFocus, etc.
- `utils/` — visibleWidth, wrapTextWithAnsi, sliceByColumn

## Commands

### Type checking
```bash
# Check the main package
cd packages/twinki && bun tsc --noEmit

# Check testing package (requires twinki built first)
cd packages/twinki && bun tsc
cd packages/testing && bun tsc --noEmit
```

### Running tests
```bash
# All tests (run from package root, not repo root)
cd packages/twinki && bun vitest run

# Specific test file
cd packages/twinki && bun vitest run test/chat-app.test.ts

# Watch mode
cd packages/twinki && bun vitest
```

### Running examples
```bash
# IMPORTANT: install dependencies first (sets up workspace symlinks)
bun install

# IMPORTANT: rebuild first — examples import from dist/, not source
cd packages/twinki && bun tsc

# Then run
bun tsx examples/01-hello.tsx
```

### Building
```bash
cd packages/twinki && bun tsc
```

### Do NOT run
- `bun test` from repo root (use `cd packages/twinki && bun vitest run`)
- `bun run build` from repo root without checking individual packages first

## Code Quality

- No `any` types unless absolutely necessary
- Always use top-level imports, never dynamic `import()` for types
- Check `node_modules` for external API type definitions instead of guessing
- All source files use `.js` extensions in imports (NodeNext module resolution)
- JSX files use `.tsx` extension

### Code Organization and Duplication

**ALWAYS review code organization before implementing:**
- Use `code` tool (`search_symbols`, `lookup_symbols`) to find existing implementations
- Check for similar functions, utilities, or patterns already in the codebase
- Avoid duplicating logic — extract and reuse existing code
- If similar code exists in multiple places, refactor to a shared utility first
- Use `grep` or `code_search` to find related code by functionality, not just by name

**Before adding new code:**
1. Search for existing symbols that might already solve the problem
2. Check relevant directories (`utils/`, `hooks/`, `components/`) for reusable code
3. If duplication is unavoidable, document why in a comment

## Testing Approach

Tests use `@xterm/headless` as a virtual terminal to verify the full rendering pipeline without a real TTY.

### Test artifacts

Every test that creates a `TestTerminal` automatically dumps its last frame to:
```
packages/twinki/test/.artifacts/<suite>/<test>/last-frame.txt
```

For frame-by-frame analysis, tests can explicitly call:
```typescript
dumpAllFrames(term, testDir('Suite_Name', 'test_name'));
```
This writes `all-frames.txt` with every frame, inter-frame diffs, and a flicker report.

Artifacts are gitignored and wiped on each test run. Run tests first, then read the artifacts.

### Reading frame artifacts

**`last-frame.txt`** — the final rendered state of the terminal viewport:
```
Frame 8 (179B, diff):
┌──────────────────────────────────────────────────┐
│> What is the meaning of life?                    │
│                                                  │
│  The meaning of life is 42.                      │
│                                                  │
│──────────────────────────────────────────────────│
│ Ready                                            │
└──────────────────────────────────────────────────┘
```

The header tells you: frame index, bytes written to terminal, and whether it was a `full` redraw or `diff` (differential). The box shows exactly what the user sees in the terminal viewport. Each `│` row is one terminal line. Empty rows are blank lines on screen.

**`all-frames.txt`** — every frame in sequence with diffs between them:
```
Frame 0 (131B, diff):
┌────────────────────────────────────────┐
│                                        │
│────────────────────────────────────────│
│ Ready  •  0 messages                   │
└────────────────────────────────────────┘
  Changes → Frame 1:
    row 0: "" → "> Hello"
    row 2: " Ready  •  0 messages" → " Sending..."

Frame 1 (207B, diff):
┌────────────────────────────────────────┐
│> Hello                                 │
│                                        │
│ Sending...                             │
└────────────────────────────────────────┘
  (no changes → Frame 2)

--- Flicker Report ---
Clean: true
```

### How to use artifacts when developing

1. **Before making a change**: run the relevant test, read `last-frame.txt` to understand the current visual state.

2. **After making a change**: run the test again, compare the new `last-frame.txt` to the old one. The visual output should match your expectations.

3. **Verifying differential rendering**: add `dumpAllFrames(term, testDir('Suite', 'test'))` to a test. In `all-frames.txt`:
   - Check the `Changes →` sections. Only the rows you expect to change should appear.
   - If a row shows up in the diff that shouldn't have changed, your change is causing unnecessary redraws.
   - The `(no changes → Frame N)` marker means the frame was identical — no terminal writes needed.

4. **Checking byte efficiency**: compare `writeBytes` across frames in `all-frames.txt`. Differential frames should be much smaller than the first full frame. If a diff frame has similar byte count to a full frame, the diff algorithm isn't working for that transition.

5. **Detecting flicker**: the `--- Flicker Report ---` at the end of `all-frames.txt` tells you if any cell went non-blank → blank → non-blank across consecutive frames. `Clean: true` means zero flicker. If flicker is detected, it lists the frame index, row, and column.

6. **Debugging layout issues**: read the frame viewport rows to verify component positioning. Row 0 is the top of the viewport. Check that:
   - Content appears at the expected row
   - Borders/separators are at the right positions
   - No content overlaps (two components writing to the same row)
   - Padding/margins produce the expected blank space

### Frame properties reference

```typescript
interface Frame {
  index: number;       // sequential frame number (0 = first render)
  timestamp: bigint;   // nanosecond timestamp (for latency measurement)
  viewport: string[];  // terminal lines as the user sees them (ANSI stripped)
  writeBytes: number;  // bytes written to terminal for this frame
  isFull: boolean;     // true = full redraw, false = differential update
}
```

- `isFull === true` on first render and after terminal resize. All other frames should be `isFull === false`.
- `writeBytes` for a differential frame should be proportional to the number of changed rows, not total content size.
- `viewport` is what xterm parsed — it reflects the actual terminal state, not what the code intended to write.

### Test infrastructure
- `test/helpers.ts` — shared `TestTerminal`, `MutableComponent`, `analyzeFlicker`, `diffFrames`, `serializeFrame`, `dumpLastFrame`, `dumpAllFrames`, `testDir`
- `test/setup.ts` — vitest afterEach hook that auto-dumps last frame for every test
- `vitest.global-setup.ts` — cleans artifacts dir before each test run
- Tests create a `TestTerminal` + `TUI`, add components, call `tui.start()`, then `await wait()` + `await term.flush()` to capture frames

### Key test patterns
```typescript
// Create terminal and TUI
const term = new TestTerminal(40, 10);
const tui = new TUI(term);
const comp = new MutableComponent();
comp.lines = ['Hello'];
tui.addChild(comp);
tui.start();

// Wait for render + capture frame
await wait();
await term.flush();

// Assert on captured frame
expect(term.getLastFrame()!.viewport[0]).toContain('Hello');

// Update and verify differential
comp.lines = ['Changed'];
tui.requestRender();
await wait();
await term.flush();
expect(term.getLastFrame()!.isFull).toBe(false); // differential, not full

// Cleanup
tui.stop();
```

### What to test
- **Rendering correctness**: content appears in viewport
- **Differential updates**: `frame.isFull === false`, `diffFrames()` shows only changed rows
- **Zero flicker**: `analyzeFlicker(frames).clean === true`
- **Byte efficiency**: `frame.writeBytes` for diff < first render
- **Input dispatch**: `term.sendInput('a')` reaches focused component

### When writing tests
- Run them immediately after writing
- Fix failures before moving on
- Never skip or disable tests without explanation

## Architecture Decisions

1. **Line-based diff, not cell-based** — O(n) where n=lines, string equality with early exit
2. **Event-driven scheduling** — `process.nextTick` debounce, no fixed render loop
3. **Synchronized output** — `CSI ?2026h/l` wrapping every frame, single `terminal.write()` call
4. **Static content model** — `<Static>` writes directly to scrollback, never in `previousLines`

### TUI doRender() Four Strategies
1. First render: write all, no cursor movement
2. Width changed: `\x1b[3J\x1b[2J\x1b[H` + full rewrite
3. Shrink clear: full clear when content shrinks (optional, `clearOnShrink`)
4. Differential: find firstChanged/lastChanged, write only that range

### Critical State Fields (TUI class)
```typescript
previousLines: string[]      // front buffer (what's on screen)
previousWidth: number        // terminal width at last render
maxLinesRendered: number     // high-water mark
previousViewportTop: number  // viewport top at last render
cursorRow: number           // logical end of content
hardwareCursorRow: number   // actual terminal cursor position
```

## Style

- Keep answers short and concise
- No emojis in code or commits
- Technical prose, direct and clear
- Comments only where logic is non-obvious

## Git Rules

- Always use `git add <specific-files>`, never `git add -A` or `git add .`
- Never `git reset --hard`, `git checkout .`, `git clean -fd`, or `git stash`
- Never `git commit --no-verify`
- Include issue references in commits when applicable
