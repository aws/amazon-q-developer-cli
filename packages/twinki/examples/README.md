# Twinki Examples

Standalone runnable apps demonstrating Twinki's capabilities, from minimal to production-realistic.

## Prerequisites

From the repo root:

```bash
npm install
```

## Running

**Important**: Build first — examples import from `dist/`, not source.

```bash
cd packages/twinki && npx tsc
cd ../..
npx tsx examples/<name>.tsx
```

Press `Ctrl+C` to exit any example (or `q` where noted).

## Examples

### 01-hello.tsx — Hello World

Minimal app. Proves the React → Terminal pipeline works.

```bash
npx tsx examples/01-hello.tsx
```

What it shows: `Text` component with bold, color, and dim styling.

---

### 02-counter.tsx — Interactive Counter

Keyboard-driven state updates.

```bash
npx tsx examples/02-counter.tsx
```

Controls: `↑`/`k` increment, `↓`/`j` decrement, `r` reset, `q` quit.

What it shows: `useInput` hook, conditional styling based on state, differential rendering (only the counter line redraws).

---

### 03-spinner.tsx — Build Pipeline

Animated progress through a sequence of tasks.

```bash
npx tsx examples/03-spinner.tsx
```

What it shows: `setInterval` animation, task completion with checkmarks, automatic exit on completion. Demonstrates that only the spinner character redraws each frame — no flicker.

---

### 04-todo.tsx — Todo List

Full CRUD todo list with keyboard navigation.

```bash
npx tsx examples/04-todo.tsx
```

Controls: `↑`/`↓` navigate, `space` toggle, `a` add, `d` delete, `q` quit.

What it shows: List rendering, cursor selection, strikethrough styling, dynamic add/remove.

---

### 05-dashboard.tsx — System Dashboard

Live-updating metrics display with borders and progress bars.

```bash
npx tsx examples/05-dashboard.tsx
```

What it shows: `Box` with `borderStyle="round"`, CPU bar visualization, multiple data sections updating every second. Only changed values redraw.

---

### 06-chat.tsx — AI Chat Interface

The most complex example. Simulates a real AI chat with:

- Scrolling message history
- Simulated streaming responses (word by word)
- Animated typing indicator (braille spinner)
- Text input with cursor
- Status bar at the bottom
- Multi-line AI responses with code blocks

```bash
npx tsx examples/06-chat.tsx
```

Controls: Type a message, press `Enter` to send. The AI responds with a simulated streaming response.

What it shows: This is the scenario where Ink typically breaks down — growing scrollback, animated indicators, and a fixed status bar. Twinki handles it with zero flicker because every frame is a single synchronized write with only the changed lines updated.

---

### 07-chat-long.tsx — AI Coding Agent (Long Duration)

Simulates an LLM coding agent that reads files, searches code, runs tests, and generates large classes. Each response is hundreds of lines, streamed character-by-character.

```bash
npx tsx examples/07-chat-long.tsx
```

Controls: Press `Enter` to send the next pre-scripted prompt (4 total). Each triggers a long streaming response.

What it shows: This is the extreme stress test — the kind of workload real coding agents produce. Responses include 80-line file reads, search results, test output, and a 180-line TypeScript class, all streamed token-by-token. The total conversation exceeds 1000 lines. Twinki's differential rendering keeps each frame update minimal even as the scrollback grows massive.

---

### 17-pura-vida.tsx — Pura Vida (Rendering Stress Test)

Interactive platformer running at 60fps in the terminal. A scrolling pixel-art world with original sprites and scenery.

```bash
npx tsx examples/17-pura-vida.tsx
```

Controls: `←` `→` move, `space` jump, `q` quit.

What it shows: This is the ultimate stress test for flicker-free differential rendering. Every frame redraws a full pixel grid (each pixel = 2 terminal characters of ANSI background color), producing ~3000+ characters of escape codes per frame at 60fps. Specific capabilities demonstrated:

- **60fps differential rendering** — the entire game grid changes every frame as the camera scrolls, yet only changed terminal lines are rewritten. Without line-level diffing, this would produce visible flicker on every frame.
- **Synchronized output** — each frame is wrapped in `CSI ?2026h/l`, so the terminal compositor presents the entire frame atomically. Even with thousands of ANSI escape codes per frame, there is zero tearing.
- **Dense ANSI content measurement** — `visibleWidth()` must correctly skip hundreds of `\x1b[48;5;XXXm` sequences per line to calculate layout widths for the bordered viewport Box.
- **High-frequency React reconciliation** — `useFrames(60)` triggers 60 state changes per second. The reconciler, Yoga layout engine, and terminal write path are all exercised at sustained high throughput.
- **Dynamic viewport sizing** — `useStdout()` reads terminal dimensions and listens for resize events. The game grid adapts in real-time.
- **Component composition under load** — HUD, game viewport, and end screen all use nested `Box` layout — recalculated every frame.

---

### 11-text-input.tsx — Single-line Text Input

Chat-style input with full editing support.

```bash
npx tsx examples/11-text-input.tsx
```

Controls: Type text, `Enter` submit, `Escape` quit, `Ctrl+A/E` jump start/end, `Ctrl+K/U` kill line, `Ctrl+Y` yank, `Ctrl+W` kill word, `Ctrl+-` undo.

What it shows: `TextInput` component with placeholder, horizontal scrolling, undo/redo, Emacs kill ring, bracketed paste, grapheme-aware cursor.

---

### 12-select.tsx — Selection List

Language picker with scrollable list.

```bash
npx tsx examples/12-select.tsx
```

Controls: `↑`/`↓` navigate, `Enter` select, `Escape` cancel.

What it shows: `Select` component with items, descriptions, `maxVisible` scrolling, wrap-around navigation, scroll indicators.

---

### 13-editor.tsx — Multi-line Editor

Multi-line text editor with word-wrap.

```bash
npx tsx examples/13-editor.tsx
```

Controls: `Shift+Enter` new line, `Enter` submit, `↑`/`↓` navigate lines, `Ctrl+K/U/W/Y` kill ring, `Ctrl+-` undo.

What it shows: `EditorInput` component with word-wrap, vertical scrolling, undo/redo, kill ring, history navigation, sticky column.

---

### 14-editor-autocomplete.tsx — Editor with Autocomplete

Editor with `/` command completion.

```bash
npx tsx examples/14-editor-autocomplete.tsx
```

Controls: Type `/` to trigger, `Tab` to accept, `↑`/`↓` navigate suggestions, `Escape` cancel.

What it shows: `EditorInput` with `autocompleteProvider` prop, `SelectList` integration, prefix matching, completion application.

---

### 15-component-showcase.tsx — Component Showcase

Interactive demo cycling through all new input components and their properties.

```bash
npx tsx examples/15-component-showcase.tsx
```

Controls: `Ctrl+N`/`Ctrl+P` switch between components, `Escape` quit.

What it shows: All three input components (`TextInput`, `Select`, `EditorInput`) with different prop configurations — placeholder, onChange, filtering, autocomplete, maxVisible, paddingX.

---

### 16-mouse.tsx — Mouse Events

Interactive mouse demo with hit-test debug panel.

```bash
npx tsx examples/16-mouse.tsx
```

Controls: Click boxes, hover to highlight, `Escape` quit.

What it shows: SGR mouse protocol integration with Yoga layout hit testing. Four sections:

- **Hit-Test Debug Panel** — live display of raw mouse event metadata (position, button, type, modifiers) alongside hit-test resolution (hovered node ID, clicked node ID, last handler fired). Shows the full pipeline from terminal coordinates to component dispatch.
- **Color Boxes** — clickable boxes with hover-highlighted borders. Demonstrates `onClick`, `onMouseEnter`, `onMouseLeave` props on `Box`.
- **Sidebar Menu** — clickable menu items on the left, content pane on the right updates on selection. Shows mouse-driven navigation replacing keyboard input.
- **Confirm Dialog** — Yes/No buttons with active state and result feedback. Shows mouse-driven form interaction.

---

### 17-paste.tsx — Bracketed Paste

Demonstrates bracketed paste handling.

```bash
npx tsx examples/17-paste.tsx
```

What it shows: `usePaste` hook for receiving pasted content.

---

### 18-static-chat.tsx — Static Chat with Scrollback

Chat using `<Static>` for message history.

```bash
npx tsx examples/18-static-chat.tsx
```

What it shows: `Static` component for scrollback, live area for active content.

---

### 21-overlay-tips.tsx — Overlay Tips

Floating overlay tooltips.

```bash
npx tsx examples/21-overlay-tips.tsx
```

What it shows: Overlay system for floating content above the main UI.

---

### 22-resize.tsx — Resize Handling

Stress test for terminal resize behavior.

```bash
npx tsx examples/22-resize.tsx
```

What it shows: Static + live content surviving resize events without duplication.

---

### 23-bg-colors.tsx — Background Colors

Chat messages with background-colored text boxes using chalk.

```bash
npx tsx examples/23-bg-colors.tsx
```

What it shows: `Box backgroundColor` prop with chalk-styled text content.

---

### 24-syntax-highlight.tsx — Syntax-Highlighted Editor

Multi-line editor with shiki-powered syntax highlighting and theme switching.

```bash
npx tsx examples/24-syntax-highlight.tsx
```

Controls: `e` edit, `Escape` stop editing, `Tab`/`Shift+Tab` cycle themes, `q` quit.

What it shows: `EditorInput` with `syntaxHighlight` and `syntaxTheme` props, line numbers, live theme switching across 10 themes (monokai, dracula, github-dark, catppuccin, nord, tokyo-night, etc.).

---

## Writing Your Own

```tsx
import React from 'react';
import { render, Text, Box, useInput, useApp } from 'twinki';

const MyApp = () => {
  const { exit } = useApp();
  useInput((input, key) => {
    if (input === 'q') exit();
  });

  return (
    <Box flexDirection="column">
      <Text bold>My App</Text>
      <Text>Press q to quit</Text>
    </Box>
  );
};

render(<MyApp />);
```

The API is identical to Ink. If you have an existing Ink app, change the import from `'ink'` to `'twinki'` and it should work.
