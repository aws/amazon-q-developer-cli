# twinki

A high-performance terminal UI renderer for React.

## Quick start

```bash
npm install twinki
```

```tsx
import React from 'react';
import { render, Box, Text } from 'twinki';

const App = () => (
  <Box flexDirection="column">
    <Text bold>Hello, twinki!</Text>
  </Box>
);

render(<App />);
```

## Architecture

twinki is a custom React reconciler that targets the terminal instead of the DOM. Component trees are laid out with yoga (flexbox), then painted by an inline, line-based differential renderer — only changed lines are rewritten, with synchronized output so complex layouts never flicker. Input is handled natively, including the Kitty keyboard protocol (force-enabled on iTerm2, Kitty, WezTerm, Ghostty) for enhanced key detection such as Shift+Enter, key repeat, and key release events. Mouse tracking (SGR), bracketed paste, and alternate-screen fullscreen mode are opt-in via `render()` options or hooks.

## Components

| Component | Description | Key props |
|---|---|---|
| `Text` | Styled text run (color, bold, dim, wrap) | `color`, `backgroundColor`, `bold`, `dimColor`, `wrap`, `onClick` |
| `Box` | Flexbox layout container | `flexDirection`, `width`, `height`, `borderStyle`, `position`, `onClick`, `onMouseDown` |
| `Tabs` | Horizontal tab strip with overflow windowing | `tabs`, `activeId`, `onActivate`, `onClose`, `width` |
| `Split` | Two-pane resizable split layout | `direction`, `ratio`, `width`, `height`, `activePane`, `onResize` |
| `DiffView` | Side-by-side or inline code diff (Myers + word-level) | `values`, `highlight`, `lang`, `layout` |
| `Editor` | Multi-line editor engine (component class, no React) | keybindings, autocomplete provider, themes |
| `EditorInput` | React multi-line editor with wrap, scroll, autocomplete | `value`, `onChange`, `onSubmit`, `autocomplete`, `highlight` |
| `TextInput` | Single-line input with h-scroll, undo/redo, kill ring | `value`, `placeholder`, `onSubmit`, `onChange`, `isActive` |
| `Select` | Scrollable selection list with filtering | `items`, `maxVisible`, `onSelect`, `onCancel`, `filter` |
| `SelectList` | Low-level select engine used by `Select` | `items`, theme functions |
| `Markdown` | Markdown to styled ANSI (optional shiki highlighting) | `children`, `highlight`, `theme` |
| `Typewriter` | Progressive text reveal with natural pacing | `children`, `speed`, `markdown` |
| `Scrollbar` | Vertical scrollbar, optionally clickable (ratatui-style) | `scrollTop`, `totalLines`, `viewportHeight`, `onScrollTo` |
| `StreamingPanel` | Height-capped panel for streaming content | `content`, `streaming`, `height`, `children` (render fn) |
| `Region` | Render boundary — state changes inside re-render only the subtree | `id` |
| `Static` | Append-only scrollback items rendered once | `items`, `children` (render fn) |

Ink-compatible components (`Newline`, `Spacer`, `Transform`) are also exported.

## Hooks

| Hook | Description |
|---|---|
| `useInput` | Keyboard input handler `(input, key) => void`; Kitty-aware key parsing |
| `useTabs` | Tab-collection state model for the `Tabs` component |
| `useOverlay` | Show a React element as a floating overlay above the main tree |
| `useMouse` | Mouse events (click, move, wheel); auto-enables SGR tracking |
| `usePaste` | Bracketed-paste events with markers stripped |
| `useFullscreen` | Enter alt screen on mount, exit on unmount (prefer `render(..., { fullscreen: true })`) |
| `useFrames` | Frame counter incrementing at a given FPS, for animations |
| `useKeyRepeat` | Key repeat events (Kitty protocol terminals only) |
| `useKeyRelease` | Key release events (Kitty protocol terminals only) |
| `useScroll` | `scrollTop` state with `scrollBy`/`scrollTo` helpers |
| `useFocus` / `useFocusManager` | Ink-compatible focus tracking and traversal |
| `useApp` | `exit()` handle for the running app |
| `useStdin` / `useStdout` / `useStderr` | Access to the underlying streams |

## New primitives

### Tabs

Presentational tab strip. State lives in the consumer (pair with `useTabs`). The active tab renders as an inverted chip and can never scroll out of view: overflow windows around the active tab with `‹N` / `N›` hidden-count markers. Click a tab to activate; the active tab shows a close affordance (`✕`). Wire Ctrl+Tab / Ctrl+Shift+Tab / Ctrl+1..9 / Ctrl+w through `useInput` to the model.

| Prop | Type | Description |
|---|---|---|
| `tabs` | `Tab[]` | Ordered open tabs (`{ id, title, icon?, iconColor?, dirty?, closable? }`) |
| `activeId` | `string` | Id of the active tab |
| `onActivate` | `(id) => void` | Tab clicked or keyboard-activated |
| `onClose` | `(id) => void` | Close requested (`✕` click) |
| `showIndexes` | `boolean` | Prefix each tab with its 1-based index (Ctrl+N targets) |
| `width` | `number` | Strip width in columns; enables overflow windowing |
| `activeColor` / `inactiveColor` / `borderColor` | `string` | Accent, dim text, separator colors |
| `stripColor` / `activeTextColor` | `string` | Strip background band; text color on the active chip |

```tsx
const model = useTabs({ initial: [{ id: 'a', title: 'app.ts', closable: true }] });
useInput((_input, key) => {
  if (key.ctrl && key.tab) key.shift ? model.cyclePrev() : model.cycleNext();
});
<Tabs tabs={model.tabs} activeId={model.activeId}
  onActivate={model.activate} onClose={model.close} width={80} />
```

### Split

Two-pane layout (`row` = side-by-side, `column` = stacked) with a 1-cell separator. `ratio` (0-1) sizes pane A; the separator is mouse-draggable when `onResize` is provided (clamped to 0.2-0.8). `activePane` highlights the focused pane's border — focus ownership stays with the consumer.

| Prop | Type | Description |
|---|---|---|
| `direction` | `'row' \| 'column'` | Side-by-side or stacked |
| `ratio` | `number` | 0-1 proportion given to pane A |
| `width` / `height` | `number` | Total available size in cells |
| `activePane` | `'a' \| 'b'` | Which pane gets the highlighted border (default `'a'`) |
| `activeColor` / `inactiveColor` | `string` | Border colors |
| `children` | `[element, element]` | Exactly two panes |
| `onResize` | `(ratio) => void` | New ratio during separator drag; omit for a fixed split |

```tsx
const [ratio, setRatio] = useState(0.3);
<Split direction="row" ratio={ratio} width={cols} height={rows}
  activePane={focused} onResize={setRatio}>
  <Sidebar />
  <Content />
</Split>
```

### useTabs

Owns the ordered tab list and active id; returns a `TabsModel`:

| Method | Description |
|---|---|
| `open(tab)` | Add a tab (or activate if already open) and focus it |
| `close(id)` | Close; if active, activates the adjacent tab. `onBeforeClose` can veto |
| `activate(id)` | Activate by id (no-op if unknown) |
| `cycleNext()` / `cyclePrev()` | Cycle with wrap-around |
| `jumpTo(index)` | Activate by 0-based index |
| `setDirty(id, dirty)` | Toggle the dirty marker |
| `setTitle(id, title)` | Rename a tab |

```tsx
const model = useTabs({ onBeforeClose: (id) => !isDirty(id) });
model.open({ id: 'readme', title: 'README.md', closable: true });
```

## Rendering

`render(element, options)` is the entry point: it wires the React reconciler to a `TUI` instance and returns an `Instance` with `unmount()` and `waitUntilExit()`. Options include `fullscreen`, `mouse`, `targetFps`, `exitOnCtrlC`, and a custom `terminal` (used by tests). The `TUI` class is the low-level renderer for non-React components implementing the `Component` interface. Embed `CURSOR_MARKER` in a rendered line to place the hardware cursor at that cell — used for screen-reader-friendly menus and input caret positioning.

## Performance

- **Region-scoped rendering**: state changes inside a `<Region>` re-render only that subtree; the rest of the tree serves cached lines.
- **Frame pacing**: rendering is event-driven and uncapped by default; `targetFps` caps repaint rate when needed.
- **Off-screen skip**: content outside the viewport is not painted; differential updates rewrite only changed lines with synchronized output.

## Examples

See `examples/` for working demos. Build first, then run:

```bash
cd packages/twinki && npx tsc -p packages/twinki/tsconfig.build.json
npx tsx examples/<name>.tsx       # single-file examples
npx tsx examples/<name>/index.tsx # directory examples
```
