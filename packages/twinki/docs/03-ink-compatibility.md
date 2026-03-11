# Twinki — Design Document
## Page 3 of 5: Ink Compatibility — Drop-In Replacement Architecture

---

### 3.1 The Compatibility Imperative

Twinki is not useful if it requires rewriting existing applications. The Node.js
TUI ecosystem has years of investment in Ink components, hooks, and patterns.
Claude Code, Amp, Codex, and hundreds of smaller tools are built on Ink's API.
A new rendering engine that requires a full rewrite will not be adopted.

The goal is a drop-in replacement at the import level:

```diff
- import { render, Box, Text, useInput, useApp } from 'ink';
+ import { render, Box, Text, useInput, useApp } from 'twinki';
```

That single line change must be sufficient for the majority of Ink applications.
This section specifies exactly what that means, where the boundaries are, and
what the known incompatibilities are.

---

### 3.2 Ink's Complete Public API Surface

Ink's public API is smaller than it appears. The complete set of exports that
applications actually use:

**Rendering entry point:**
- `render(element, options?)` — mounts a React tree, returns an `Instance`
- `Instance` — `{ unmount(), waitUntilExit(), clear(), rerender(element) }`

**Layout components:**
- `<Box>` — flexbox container (Yoga node). Props: `flexDirection`, `alignItems`,
  `justifyContent`, `flexWrap`, `flexGrow`, `flexShrink`, `flexBasis`, `gap`,
  `rowGap`, `columnGap`, `width`, `height`, `minWidth`, `minHeight`, `maxWidth`,
  `maxHeight`, `padding`, `paddingX`, `paddingY`, `paddingTop`, `paddingBottom`,
  `paddingLeft`, `paddingRight`, `margin`, `marginX`, `marginY`, `marginTop`,
  `marginBottom`, `marginLeft`, `marginRight`, `borderStyle`, `borderColor`,
  `display`, `overflow`, `overflowX`, `overflowY`
- `<Text>` — text with optional styling. Props: `color`, `backgroundColor`,
  `dimColor`, `bold`, `italic`, `underline`, `strikethrough`, `inverse`, `wrap`
- `<Newline count?>` — explicit line break
- `<Spacer>` — flex spacer (fills remaining space)
- `<Static items render>` — renders children once, appends to scrollback, never redraws
- `<Transform transform>` — applies a string transformation to rendered output

**Hooks:**
- `useInput(handler, options?)` — keyboard input. Options: `isActive`
- `useApp()` — returns `{ exit(error?) }`
- `useStdin()` — returns `{ stdin, isRawModeSupported, setRawMode }`
- `useStdout()` — returns `{ stdout, write }` plus `columns`, `rows`
- `useStderr()` — returns `{ stderr, write }`
- `useFocus(options?)` — returns `{ isFocused }`. Options: `isActive`, `id`, `autoFocus`
- `useFocusManager()` — returns `{ enableFocus, disableFocus, focusNext, focusPrevious, focus }`

**Types:**
- `Key`, `DOMElement`, `Styles`, `TextProps`, `BoxProps`, `RenderOptions`

This is the complete surface. Twinki implements all of it.

---

### 3.3 Layer Architecture

Twinki's compatibility layer has four distinct layers, each with a clear
responsibility boundary:

**Layer 1: React Reconciler** — identical to Ink's.

Both Ink and Twinki use `react-reconciler` to map React's virtual DOM to a custom
host environment. The reconciler is responsible for creating, updating, and deleting
host nodes; calling `commitUpdate` when props change; and scheduling renders via
React's scheduler. Twinki's reconciler is a direct port of Ink's, with one
difference: instead of calling Ink's `renderToString()` at commit time, it marks
the affected subtree as dirty and calls `tui.requestRender()`.

**Layer 2: Layout Engine** — Yoga, unchanged.

Twinki uses the same `yoga-layout` bindings as Ink. The `<Box>` component's props
map directly to Yoga node properties. This is a deliberate non-change. Yoga is
well-tested, handles edge cases correctly (fractional dimensions, overflow, nested
flex containers), and the Ink community has years of experience with its behavior.
Replacing it would break layout compatibility in subtle ways.

**Layer 3: Component Rendering** — new implementation, same output contract.

`<Text>` renders to an array of ANSI-encoded strings using `wrapTextWithAnsi()`.
`<Box>` collects its children's line arrays and applies Yoga's computed layout
(padding, borders, dimensions). `<Static>` writes its children's output directly
to stdout as a single append operation and never includes them in the live region.
`<Transform>` applies its transform function to each line string.

**Layer 4: Hooks** — reimplemented against Twinki's input pipeline.

`useInput` registers a handler with the TUI's input system. `useApp` provides
access to the TUI's `stop()` method. `useStdout` returns `process.stdout` plus
the terminal's current dimensions from `tui.terminal.columns/rows`. `useFocus`
and `useFocusManager` delegate to `tui.setFocus()` and the overlay stack.

---

### 3.4 The `<Static>` Component: Critical Semantics

`<Static>` is Ink's mechanism for content that should be appended to the scrollback
buffer and never redrawn. It is the correct primitive for completed messages, tool
outputs, and any content that is "done."

The semantics are precise: when `<Static>`'s `items` prop changes (new items are
added), the new items are rendered and written directly to stdout as a single
append, before the live region is redrawn. The live region then moves down by the
number of new lines written.

Implementation:

```typescript
// In the reconciler commit phase, when Static receives new items:
const newItemLines = renderItems(newItems, terminalWidth);
const output = newItemLines.join('\r\n') + '\r\n';

// Write static content BEFORE the live region render
process.stdout.write(output);

// Update maxLinesRendered to account for the new static lines
tui.maxLinesRendered += newItemLines.length;

// The next doRender() will correctly position the cursor
// relative to the new maxLinesRendered value
```

The invariant: **static content is never part of `previousLines`**. It lives in
the scrollback buffer. `previousLines` only tracks the live region. This keeps
the diff algorithm O(live_region_height) regardless of how much static content
has been emitted — critical for long-running sessions.

---

### 3.5 `render()` Options and the Output Target

Ink's `render()` accepts:

```typescript
interface RenderOptions {
    stdout?: NodeJS.WriteStream;
    stdin?: NodeJS.ReadStream;
    stderr?: NodeJS.WriteStream;
    debug?: boolean;
    exitOnCtrlC?: boolean;
    patchConsole?: boolean;
}
```

Twinki accepts the same options. The `stdout` option is the key to testability:
by passing a mock `WriteStream`, all output is captured without a real terminal.
Twinki's test framework uses this to inject a `VirtualTerminal` as the output
target.

`patchConsole`: Ink patches `console.log` to route output through its rendering
pipeline. Twinki does the same, but with a cleaner implementation: `console.log`
output is queued as static content and written before the next live region render,
rather than interrupting the current frame.

`exitOnCtrlC`: When true (default), Ctrl+C calls `app.exit()`. Twinki implements
this in the input handler before forwarding to the focused component, matching
Ink's behavior exactly.

Twinki adds one new option not in Ink:

```typescript
interface TwinkiRenderOptions extends RenderOptions {
    targetFps?: number;  // default: Infinity (uncapped)
}
```

`targetFps` sets a maximum frame rate for the adaptive pacing algorithm. Set to
`60` to match Ink's behavior exactly. Set to `120` for high-refresh displays.
`Infinity` (default) means the terminal's throughput is the only limit.

---

### 3.6 `useInput` Reimplementation

Ink's `useInput` provides a clean abstraction over raw stdin:

```typescript
useInput((input, key) => {
    if (key.upArrow) { ... }
    if (key.ctrl && input === 'c') { ... }
    if (key.return) { ... }
});
```

Twinki's `useInput` is API-identical. The `key` object has the same shape as
Ink's: `{ upArrow, downArrow, leftArrow, rightArrow, return, escape, ctrl, shift,
alt, tab, backspace, delete, pageUp, pageDown, home, end, meta }`.

The underlying implementation uses `matchesKey(data, keyId)` from pi-tui's
`keys.ts`, which handles both legacy terminal sequences and Kitty keyboard protocol
sequences. The `input` parameter is the printable character (if any) or an empty
string for non-printable keys.

One behavioral difference from Ink: when Kitty keyboard protocol is active,
`useInput` receives key release events by default only if the component sets
`wantsKeyRelease = true`. This prevents double-firing of handlers that only care
about key presses. Ink has no concept of key release events.

---

### 3.7 Focus Management

Ink's focus system (`useFocus`, `useFocusManager`) is reimplemented against
Twinki's `tui.setFocus()` and overlay stack. The semantics are identical:

- `useFocus({ autoFocus: true })` — component receives focus on mount
- `useFocusManager().focusNext()` — moves focus to the next focusable component
- `useFocusManager().focusPrevious()` — moves focus to the previous focusable component

Twinki extends this with the `Focusable` interface from pi-tui: components that
want to position the hardware cursor (for IME candidate window placement) emit
`CURSOR_MARKER` (`\x1b_pi:c\x07`) at the cursor position in their render output.
The TUI extracts this marker, strips it from the line, and moves the hardware
cursor to that position after the frame is written.

This is invisible to Ink-compatible components (they don't emit the marker) but
available to Twinki-native components that need precise cursor positioning.

---

### 3.8 Known Incompatibilities

A small number of Ink behaviors are intentionally not replicated:

**1. Direct escape sequence writes**: Applications that write `process.stdout.write('\x1b[?1049h')` directly to enter alternate screen bypass Twinki's rendering model. They will still work — the escape sequences are passed through — but Twinki cannot track the resulting terminal state. These applications should use Twinki's `useFullscreen()` hook instead.

**2. `ink-testing-library` internals**: The popular `@ink/testing-library` package
accesses Ink internals (`ink/build/render`, `ink/build/components/App`). These
paths do not exist in Twinki. Applications using `@ink/testing-library` must
switch to `@twinki/testing-library`, which provides an identical public API.

**3. `measureText` internal export**: Some applications import Ink's internal
`measureText` utility. Twinki exports it as a public API for compatibility.

**4. `patchConsole` timing**: In Ink, `console.log` during a render cycle may
appear before or after the current frame depending on timing. In Twinki, it always
appears as static content before the next frame. This is more predictable but
technically different behavior.

---

### 3.9 Migration Path

For applications migrating from Ink to Twinki:

**Step 1**: Add `twinki` as a dependency. Keep `ink` installed.

**Step 2**: In the application entry point only, change the import:
```diff
- import { render } from 'ink';
+ import { render } from 'twinki';
```

**Step 3**: Run the application. Observe behavior. The vast majority of Ink
applications will work without any further changes.

**Step 4**: Run the test suite. If using `@ink/testing-library`, replace with
`@twinki/testing-library`. All test assertions remain identical.

**Step 5**: Optionally migrate component files to import from `twinki` directly,
removing the `ink` dependency entirely.

Twinki re-exports everything from `ink` for anything it does not override:

```typescript
// twinki/src/index.ts
export * from 'ink';                          // everything Twinki doesn't override
export { render, Instance } from './renderer'; // Twinki's rendering layer
export { useInput } from './hooks/use-input';  // Twinki's input handling
// ... other overrides
```

This means components that import from `ink` directly continue to work even when
the application entry point uses Twinki's `render()`.
