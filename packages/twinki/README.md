# Twinki

A high-performance React renderer for terminal UIs with flicker-free differential rendering. Drop-in replacement for [Ink](https://github.com/vadimdemedes/ink).

## Why Twinki?

Ink uses alternate screen mode and a fixed 60fps render loop. Twinki uses **inline rendering** with **line-based differential updates** and **synchronized output** — no alt screen, no timer, no flicker.

| | Ink | Twinki |
|---|---|---|
| Rendering | Full repaint every frame | Differential (only changed lines) |
| Frame rate | Capped at 60fps | Event-driven (uncapped) |
| Screen mode | Alternate screen | Inline (preserves scrollback) |
| Flicker | Common with complex layouts | Zero (synchronized output) |
| Testing | Text snapshot only | Frame capture with flicker detection |

## Quick Start

```bash
npm install twinki
```

```tsx
import React from 'react';
import { render, Text } from 'twinki';

const App = () => <Text>Hello, Twinki!</Text>;

render(<App />);
```

## Migrating from Ink

### Components and Hooks

Change your imports:

```diff
- import { render, Text, Box, useInput } from 'ink';
+ import { render, Text, Box, useInput } from 'twinki';
```

All Ink components and hooks are supported: `Text`, `Box`, `Newline`, `Spacer`, `Static`, `Transform`, `useInput`, `useApp`, `useStdin`, `useStdout`, `useStderr`, `useFocus`, `useFocusManager`.

Twinki also includes interactive components not found in Ink:

- `TextInput` — Single-line text input with horizontal scrolling, undo/redo, kill ring
- `Select` — Scrollable selection list with keyboard navigation and filtering
- `EditorInput` — Multi-line text editor with word-wrap, scrolling, autocomplete

### Mouse Support

Twinki supports mouse events via the SGR protocol — not available in Ink:

```tsx
import { Box, Text, useMouse } from 'twinki';
import type { MouseEvent } from 'twinki';

const App = () => {
  useMouse((event: MouseEvent) => {
    // event: { x, y, button, type, shift, alt, ctrl }
  });

  return (
    <Box
      onClick={() => console.log('clicked!')}
      onMouseEnter={() => console.log('enter')}
      onMouseLeave={() => console.log('leave')}
    >
      <Text>Click me</Text>
    </Box>
  );
};
```

- `onClick`, `onMouseEnter`, `onMouseLeave` props on `Box` and `Text`
- `useMouse(handler)` hook for raw mouse events
- Hit testing against the Yoga layout tree maps terminal coordinates to components
- Supports left/middle/right buttons, scroll, motion, and shift/alt/ctrl modifiers

### Testing

For tests using `@ink/testing-library`, swap the import:

```diff
- import { render } from '@ink/testing-library';
+ import { render } from '@twinki/testing-library';
```

The API is compatible, but note:
- `lastFrame()` includes ANSI codes — use `.toContain()` instead of exact matches
- `frames` returns rich `Frame` objects (not just strings) with timestamps and byte counts
- No separate `stdout`/`stderr` objects

See [`@twinki/testing-library` docs](packages/testing-library/README.md) for details.

## Running Examples

```bash
# Install dependencies
npm install

# Build first (examples import from dist/)
cd packages/twinki && npx tsc && cd ../..

# Run any example
npx tsx examples/01-hello.tsx
npx tsx examples/02-counter.tsx
npx tsx examples/03-spinner.tsx
npx tsx examples/04-todo.tsx
npx tsx examples/05-dashboard.tsx
npx tsx examples/06-chat.tsx
npx tsx examples/07-chat-long.tsx
npx tsx examples/07b-diff.tsx
npx tsx examples/08-ai-coder.tsx
npx tsx examples/09-mario.tsx
npx tsx examples/10-kiro-ghost.tsx
npx tsx examples/11-text-input.tsx
npx tsx examples/12-select.tsx
npx tsx examples/13-editor.tsx
npx tsx examples/14-editor-autocomplete.tsx
npx tsx examples/15-component-showcase.tsx
npx tsx examples/16-mouse.tsx
```

See [examples/README.md](examples/README.md) for descriptions of each example.

## Running Tests

```bash
# Run all tests (318 tests across 32 files)
cd packages/twinki && npx vitest run

# Run a specific test file
cd packages/twinki && npx vitest run test/chat-app.test.ts

# Run tests in watch mode
cd packages/twinki && npx vitest
```

## Architecture

```
packages/
  twinki/              Core rendering engine
    src/
      terminal/        Terminal interface + ProcessTerminal
      input/           StdinBuffer, key matching, Kitty protocol, mouse parsing
      renderer/        TUI class (4-strategy differential renderer)
      reconciler/      React reconciler + Yoga layout
      components/      Text, Box, Static, Newline, Spacer, Transform, TextInput, Select, EditorInput
      hooks/           useInput, useApp, useFocus, useMouse, etc.
      utils/           visibleWidth, wrapTextWithAnsi, sliceByColumn
  testing/             @twinki/testing — VirtualTerminal, frame capture
  testing-library/     @twinki/testing-library — Ink-compatible test API
examples/              Runnable sample apps (01-hello through 16-mouse)
docs/                  Design documents
```

### Rendering Pipeline

```
React setState → reconciler commit → Yoga layout → TUI.render()
  → line-based diff against previousLines
  → build single escape sequence buffer (synchronized output)
  → one terminal.write() call
```

### Four Render Strategies

1. **First render** — write all lines, no cursor movement
2. **Width changed** — clear scrollback + full rewrite
3. **Shrink clear** — full clear when content shrinks (optional)
4. **Differential** — find first/last changed line, rewrite only that range

Every frame is wrapped in synchronized output (`CSI ?2026h/l`) to prevent tearing.

## Packages

| Package | Description |
|---------|-------------|
| `twinki` | Core TUI rendering engine with React reconciler |
| `@twinki/testing` | Testing framework: VirtualTerminal, FrameCapturingTerminal, flicker/collision analyzers |
| `@twinki/testing-library` | Ink-compatible testing API (WIP) |

## Documentation

- [01-motivation.md](docs/01-motivation.md) — Why Twinki exists
- [02-rendering-engine.md](docs/02-rendering-engine.md) — Differential rendering design
- [03-ink-compatibility.md](docs/03-ink-compatibility.md) — Ink API compatibility
- [04-testing-framework.md](docs/04-testing-framework.md) — Frame-accurate E2E testing
- [05-capabilities-and-roadmap.md](docs/05-capabilities-and-roadmap.md) — Terminal protocols and roadmap

## License

MIT
