---
description: "Twinki terminal UI framework reference — components, hooks, rendering model, and examples"
---

# Twinki reference

Twinki is a React renderer for terminal UIs: a custom reconciler lays out component trees with yoga (flexbox) and paints them with an inline, line-based differential renderer (no flicker, no alt screen unless requested). Input is native, with Kitty keyboard protocol support (Shift+Enter, key repeat/release) on iTerm2, Kitty, WezTerm, Ghostty.

## Import pattern

Always import from the package root:

```tsx
import { render, Box, Text, useInput, useMouse, Tabs, Split, useTabs } from 'twinki';
```

Entry point: `render(<App />, options?)` — options include `fullscreen`, `mouse: true` (required for click handlers), `targetFps`, `exitOnCtrlC`, `terminal` (custom, for tests). Returns `Instance` with `unmount()` / `waitUntilExit()`. `CURSOR_MARKER` in a rendered line places the hardware cursor there.

## Components

| Component | Key props | Purpose |
|---|---|---|
| `Box` | `flexDirection`, `width`, `height`, `borderStyle`, `backgroundColor`, `position="absolute"`, `onClick`, `onMouseDown` | Flexbox layout container |
| `Text` | `color`, `backgroundColor`, `bold`, `dimColor`, `wrap`, `onClick` | Styled text run |
| `Tabs` | `tabs`, `activeId`, `onActivate`, `onClose`, `width`, `showIndexes` | Tab strip; overflow windows around active tab |
| `Split` | `direction`, `ratio`, `width`, `height`, `activePane`, `onResize` | Two-pane resizable split |
| `DiffView` | `values=[old, new]`, `highlight`, `lang`, `layout` | Side-by-side/inline code diff |
| `EditorInput` | `value`, `onChange`, `onSubmit`, autocomplete, highlight | Multi-line editor with wrap/scroll |
| `TextInput` | `value`, `placeholder`, `onSubmit`, `onChange`, `isActive` | Single-line input (undo/redo, kill ring) |
| `Select` | `items`, `maxVisible`, `onSelect`, `onCancel`, `filter` | Scrollable filtered list |
| `Markdown` | `children` (string), `highlight` | Markdown → ANSI; shiki when `highlight` |
| `Typewriter` | `children`, `speed`, `markdown` | Progressive text reveal |
| `Scrollbar` | `scrollTop`, `totalLines`, `viewportHeight`, `onScrollTo` | Clickable vertical scrollbar |
| `StreamingPanel` | `content`, `streaming`, `height`, render-fn `children` | Height-capped streaming panel |
| `Region` | `id` | Render boundary — scoped re-renders |
| `Static` | `items`, render-fn `children` | Append-only scrollback, rendered once |

## Hooks

| Hook | Purpose |
|---|---|
| `useInput((input, key) => ...)` | Keyboard; `key` has `ctrl`, `shift`, `tab`, `return`, arrows, etc. |
| `useTabs(opts?)` | Tab state model (see below) |
| `useMouse((event) => ...)` | Click/move/wheel; `event.type`, `event.x/y`, `event.button` |
| `useOverlay(factory, opts?)` | Floating overlay above the main tree; returns a show fn → `OverlayHandle` |
| `usePaste((text) => ...)` | Bracketed paste, markers stripped |
| `useFullscreen()` | Alt screen on mount (prefer `render(..., { fullscreen: true })`) |
| `useFrames(fps)` | Incrementing frame counter for animations |
| `useKeyRepeat` / `useKeyRelease` | Kitty-protocol repeat/release events |
| `useScroll({ pageSize? })` | `{ scrollTop, scrollBy, scrollTo }` |
| `useFocus` / `useFocusManager` | Focus tracking / traversal |
| `useApp()` | `{ exit }` |
| `useStdin` / `useStdout` / `useStderr` | Stream access |

## New primitives

### Tabs + useTabs

`Tabs` is purely presentational; `useTabs` owns state. `TabsModel` methods: `open(tab)` (activate if already open), `close(id)` (activates adjacent; `onBeforeClose` can veto), `activate(id)`, `cycleNext()`, `cyclePrev()`, `jumpTo(index)`, `setDirty(id, bool)`, `setTitle(id, string)`. Tab shape: `{ id, title, icon?, iconColor?, dirty?, closable? }`.

```tsx
const model = useTabs({ initial: [{ id: 'a', title: 'app.ts', closable: true }] });
useInput((_i, key) => {
  if (key.ctrl && key.tab) key.shift ? model.cyclePrev() : model.cycleNext();
});
return (
  <Box flexDirection="column">
    <Tabs tabs={model.tabs} activeId={model.activeId}
      onActivate={model.activate} onClose={model.close} width={80} />
    {renderContent(model.activeId)}
  </Box>
);
```

The active tab is always visible: overflow shows `‹N` / `N›` hidden-count markers. `showIndexes` prefixes 1-based indexes for Ctrl+1..9 jumps. Only the active tab shows the `✕` close affordance.

### Split

```tsx
const [ratio, setRatio] = useState(0.3);
<Split direction="row" ratio={ratio} width={cols} height={rows}
  activePane={focus} onResize={setRatio}>
  <Sidebar />
  <Content />
</Split>
```

`direction`: `'row'` (side-by-side) or `'column'` (stacked). `ratio` 0-1 sizes pane A. Separator drags with the mouse when `onResize` is set (clamped 0.2-0.8). `activePane` (`'a'`/`'b'`) highlights the focused pane's border; focus state is the consumer's. Children must be exactly two elements.

## Creating a new example

Copy the structure of `examples/30-overlay-panel/` (an `index.tsx` plus a `README.md` with Run/Keys sections). Build first — examples import from `dist/`:

```bash
cd packages/twinki && npx tsc && cd ../..
npx tsx examples/<name>/index.tsx
```

`30-overlay-panel` is the minimal panel-app pattern: Tabs strip + Split + absolute-positioned overlay + useInput keys.

## Testing pattern

Render onto a `TestTerminal` from `test/helpers.ts` and assert on `frame.viewport` lines:

```tsx
import { render } from 'twinki';
import { TestTerminal, wait } from './helpers.js';

const term = new TestTerminal(80, 24);
const inst = render(<App />, { terminal: term });
await wait(60);
await term.flush();
const text = (term.getLastFrame()?.viewport ?? []).join('\n');
expect(text).toContain('expected content');
inst.unmount();
```

Helpers also provide `analyzeFlicker(frames)`, `dumpScreenshot` (SVG), and `serializeFrame` for artifacts.

## Conventions

- Functional components with hooks only — no class components.
- `useInput` for keyboard, `useMouse` for mouse (pass `mouse: true` to `render` or handlers never fire).
- `Box` for layout, `Text` for content; never put raw strings outside `Text`.
- State up in the consumer: `Tabs`/`Split` are controlled components.
- Mouse click handlers dispatch to the nearest node with the prop, so inner `onClick` wins over an ancestor's.
