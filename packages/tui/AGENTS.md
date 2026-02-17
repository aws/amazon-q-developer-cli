# AGENTS.md — Kiro TUI Codebase Guide

This document describes the architecture, patterns, and conventions used in the Kiro TUI codebase. Follow these guidelines when creating or updating components, hooks, stores, commands, utilities, and tests.

---

## Project Overview

This is a terminal-based UI (TUI) built with [Ink](https://github.com/vadimdemedes/ink) (React for CLIs) and [Zustand](https://github.com/pmndrs/zustand) for state management. It renders a chat interface for interacting with an AI agent over an ACP (Agent Communication Protocol) connection.

### Key Technologies

- **Ink** — React renderer for terminal UIs (`<Box>`, `<Text>`, `useInput`)
- **Zustand** — Lightweight state management (single `app-store.ts`)
- **Chalk** — Terminal string styling (colors, bold, dim, etc.)
- **TypeScript** — Strict typing throughout
- **Vitest** — Unit and integration testing
- **Custom Storybook** — Terminal-based component previews (`src/storybook/`)

### Directory Structure

```
src/
├── components/
│   ├── ui/              # Reusable primitives (Card, Text, Chip, Icon, Divider, etc.)
│   ├── chat/            # Chat-specific components (message, tools, prompt-bar, status-bar)
│   ├── layout/          # Layout shells (InlineLayout, ExpandedLayout, AppContainer)
│   ├── brand/           # Branding components (Wordmark)
│   └── welcome-screen/  # Welcome screen
├── stores/              # Zustand store (app-store.ts) and selectors
├── hooks/               # Custom React hooks
├── theme/               # Theme provider, dark/light theme definitions
├── types/               # Shared TypeScript types
├── commands/            # Slash command system
├── utils/               # Pure utility functions
├── contexts/            # React contexts
├── storybook/           # Terminal storybook runner
└── test-utils/          # Test helpers and mocks
e2e_tests/               # End-to-end tests using PTY
integ_tests/             # Integration tests
scripts/                 # Build, dev, and analysis scripts
```

---

## Component Patterns

### UI Primitives (`src/components/ui/`)

Each UI primitive lives in its own folder with this structure:

```
src/components/ui/<component-name>/
├── ComponentName.tsx          # Component implementation
├── ComponentName.stories.tsx  # Storybook stories
└── index.ts                   # Public exports
```

**Conventions:**

1. **Use `React.memo`** for components that receive stable props (e.g., `Divider`, `Alert`, `Tool`).
2. **Use the `useTheme` hook** from `../hooks/useThemeContext.js` to access colors — never hardcode color values.
3. **Use the `<Text>` wrapper** from `../ui/text/Text.js` instead of Ink's `<Text>` directly. This wrapper strips styling props and forces all styling through chalk functions for consistency.
4. **Color access pattern:** Call `getColor(colorPath)` which returns a chalk chain. Use it as a function: `getColor('primary')('some text')`. Access the raw hex via `.hex`.
5. **Export from `index.ts`:** Every component folder has an `index.ts` that re-exports the public API.

**Example — Creating a new UI component:**

```tsx
// src/components/ui/badge/Badge.tsx
import React from 'react';
import { Box } from 'ink';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface BadgeProps {
  label: string;
  color?: string; // Theme color path like 'success', 'error'
}

export const Badge = React.memo(function Badge({ label, color = 'primary' }: BadgeProps) {
  const { getColor } = useTheme();
  return (
    <Box>
      <Text>{getColor(color)(label)}</Text>
    </Box>
  );
});
```

```ts
// src/components/ui/badge/index.ts
export { Badge, type BadgeProps } from './Badge.js';
```

### Chat Components (`src/components/chat/`)

Chat components are domain-specific and organized by function:

- **`message/`** — Message rendering (user, agent, streaming, thinking)
- **`tools/`** — Tool call visualizations (Read, Write, Grep, Glob, Shell, WebSearch, WebFetch, Code)
- **`prompt-bar/`** — Input area (PromptInput, PromptBar, ContextBar, SnackBar, FileChip, PastedChip)
- **`status-bar/`** — Status indicator wrapper with dot icons
- **`notification-bar/`** — Transient notifications

**Tool component pattern:**

Tool components follow a consistent structure. They receive tool-specific props, parse the result, and render inside a `StatusBar` using `StatusInfo` for the header.

```tsx
export const MyTool = React.memo(function MyTool({ status, isFinished, isStatic, result }: MyToolProps) {
  const { getColor } = useTheme();
  const { expanded, expandHint } = useExpandableOutput({ totalItems, previewCount, isStatic });

  return (
    <StatusBar status={status}>
      <Box flexDirection="column">
        <StatusInfo title={isFinished ? 'Used' : 'Using'} target="my_tool" shimmer={!isFinished} />
        {/* tool-specific content */}
      </Box>
    </StatusBar>
  );
});
```

### Layout Components (`src/components/layout/`)

- **`InlineLayout`** — Main chat layout. Composes ConversationView, PromptBar, CommandMenu, panels, and alerts. Wires up keypress handling and store selectors.
- **`ExpandedLayout`** — Alternative layout for expanded views.
- **`AppContainer`** — Root wrapper providing ThemeProvider and store context.

---

## State Management

### Zustand Store (`src/stores/app-store.ts`)

The app uses a single Zustand store created with `createStore`. It holds all application state: messages, input buffer, processing flags, UI state, theme, commands, and agent metadata.

**Key patterns:**

1. **Store is provided via React context** (`AppStoreContext`), not a global singleton. Components access it via `useAppStore(selector)`.
2. **Selectors** are defined in `src/stores/selectors.ts` using `useShallow` to prevent unnecessary re-renders. Group related state into selector hooks (e.g., `useProcessingState`, `useUIState`, `useNotificationState`).
3. **Message types** use a discriminated union on `MessageRole` (User, Model, ToolUse, System).
4. **Tool results** use a discriminated union on `status` ('success' | 'error' | 'cancelled').

**When updating the store:**

- Add new state fields and their setters together.
- Add a corresponding selector in `selectors.ts` if the state is consumed by components.
- Keep actions (functions that modify state) inside the store definition using `set()`.

---

## Hooks

Custom hooks live in `src/hooks/`. Key hooks:

| Hook | Purpose |
|------|---------|
| `useTheme` (from `useThemeContext.ts`) | Access theme colors via `getColor(path)` |
| `useTextStyle(styleName)` | Get a chalk chain for a named text style (label, selectedLabel) |
| `useColor(truecolor, color256, named)` | Get a chalk function for a specific terminal color |
| `useKeypress` | Register keyboard shortcuts with modifier support |
| `useExpandableOutput` | Manage collapsible output sections in tool components |
| `useConversationContent` | Derive renderable conversation content from store messages |
| `useTerminalSize` | Track terminal dimensions |
| `useKiro` | Access the Kiro client instance |

**When creating a new hook:**

- Place it in `src/hooks/`.
- Name it `use<Purpose>.ts`.
- If it accesses the store, use `useAppStore` with a focused selector.
- Return a plain object or tuple — avoid returning the entire store.

---

## Theming

### Architecture

The theme system supports truecolor, 256-color, and named-color terminals with automatic fallback.

1. **Theme definitions** (`src/theme/kiroDark.ts`, `kiroLight.ts`) define colors as `TerminalColor` objects with three tiers: `{ truecolor, color256, named }`.
2. **`ThemeProvider`** (`src/theme/ThemeProvider.tsx`) wraps the app and provides `ThemeContext` with a `getColor(path)` helper.
3. **`getColor(path)`** resolves a dot-separated color path (e.g., `'syntax.keyword'`, `'error'`, `'diff.added.bar'`) to a chalk chain that auto-selects the best color for the terminal.
4. **Theme auto-detection** uses `detectTerminalTheme()` to pick light or dark theme.

### Color Definitions (`src/types/themeTypes.ts`)

```ts
interface TerminalColor {
  truecolor?: string;   // Hex color for truecolor terminals
  color256?: number;     // 256-color palette index
  named?: ChalkColorName; // Basic 16-color name
}
```

**When adding a new color:**

1. Add the `TerminalColor` entry to the `Theme.colors` interface in `src/types/themeTypes.ts`.
2. Define values in both `kiroDark.ts` and `kiroLight.ts`.
3. Access it via `getColor('yourNewColor')` in components.

---

## Slash Commands

### Architecture (`src/commands/`)

Commands are advertised by the backend and dispatched locally.

- **`types.ts`** — `CommandContext` interface with all the actions a command handler can invoke (show alerts, set loading, clear messages, etc.).
- **`dispatcher.ts`** — Routes commands to local effects or backend execution.
- **`effects.ts`** — Handles commands that have local side effects (e.g., `/clear`, `/compact`).
- **`local-effects.ts`** — Pure local commands that don't need backend communication.
- **`index.ts`** — Public API: `executeCommand(input, ctx)` and `executeCommandWithArg(name, value, ctx)`.

**Command flow:**

1. User types `/command args` in the prompt.
2. `parseCommand()` extracts the command name and args.
3. `findCommand()` matches by exact name or prefix against the backend-provided command list.
4. `dispatch()` routes to local effect handlers or sends to backend via `kiro.executeCommand()`.

---

## Utilities (`src/utils/`)

Utilities are pure functions (no React dependencies) organized by domain:

| File | Purpose |
|------|---------|
| `colorUtils.ts` | Terminal color detection and chalk chain creation |
| `markdown.ts` | Inline markdown parsing |
| `message-parser.ts` | Streaming markdown chunk parsing and rendering |
| `input-editing.ts` | Text input cursor movement and editing operations |
| `error-guidance.ts` | User-friendly error messages and recovery suggestions |
| `file-search.ts` | File reference expansion and gitignore-aware search |
| `tool-result.ts` | Tool result parsing and summary extraction |
| `command-history.ts` | Persistent command history (singleton) |
| `logger.ts` | Leveled logging utility |
| `terminal-theme.ts` | Terminal dark/light theme detection |
| `terminal-capabilities.ts` | Terminal color support detection |
| `synchronized-output.ts` | Synchronized terminal output for flicker-free rendering |
| `string.ts` | String manipulation helpers |
| `git.ts` | Git branch detection |

**When adding a utility:**

- Keep it in `src/utils/` as a standalone module.
- Export functions, not classes (unless state is needed, like `CommandHistory`).
- Add tests in `src/utils/__tests__/`.

---

## Stories (Terminal Storybook)

The project uses a custom terminal-based storybook (`src/storybook/`). Stories are standard CSF (Component Story Format) files.

**Story file pattern:**

```ts
// src/components/ui/badge/Badge.stories.ts
import { Badge } from './Badge.js';

const meta = {
  component: Badge,
  parameters: {
    storyOrder: ['Default', 'Error', 'Success'],
  },
};

export default meta;

export const Default = {
  args: { label: 'New', color: 'primary' },
};

export const Error = {
  args: { label: 'Failed', color: 'error' },
};
```

**Registration:** Add the import to `src/storybook/stories.ts` and include it in the `storyModules` array.

---

## Testing

### Unit Tests

- Located alongside source files or in `__tests__/` directories.
- Use Vitest.
- Test utilities and pure logic directly; test components via integration or e2e tests.

### Integration Tests (`integ_tests/`)

- Test the TUI lifecycle using `TestCase` builder from `src/test-utils/TestCase.ts`.
- Spawn the actual TUI process and interact via PTY.

### E2E Tests (`e2e_tests/`)

- Full end-to-end tests using `E2ETestCase` from `e2e_tests/E2ETestCase.ts`.
- Test real agent interactions, tool messages, slash commands, input latency, and memory leaks.
- Types for agent communication are in `e2e_tests/types/agent.ts`.

---

## Quality Standards

1. **Type safety** — No `any` types except for chalk chain returns (which are inherently dynamic). Use discriminated unions for variant types.
2. **Minimal re-renders** — Use `React.memo`, `useMemo`, `useCallback`, and focused Zustand selectors with `useShallow`.
3. **Theme compliance** — All colors must come from the theme. Never hardcode hex values or chalk colors in components.
4. **Consistent text styling** — Use `<Text>` wrapper + chalk functions. Never use Ink's `color`/`bold`/`italic` props directly.
5. **Clean exports** — Every component folder has an `index.ts`. Import from the folder, not the file.
6. **Terminal compatibility** — Always provide all three color tiers (`truecolor`, `color256`, `named`) for new theme colors.
7. **No side effects in components** — Side effects go in hooks or the store. Components are pure renderers.
8. **Expandable output** — Tool components with variable-length output must use `useExpandableOutput` for collapse/expand behavior.

---

## Updating Existing Components

1. **Read the component first.** Understand its props, how it uses the theme, and what store state it consumes.
2. **Check the selectors.** If you need new store state, add it to the store and create/update a selector in `selectors.ts`.
3. **Follow the existing pattern.** If the component uses `React.memo`, keep it. If it uses `useTheme`, use `getColor` the same way.
4. **Update stories.** If you change props or behavior, update the corresponding `.stories.tsx` file.
5. **Run the storybook** to visually verify changes in the terminal.
6. **Run tests** to ensure nothing breaks.
