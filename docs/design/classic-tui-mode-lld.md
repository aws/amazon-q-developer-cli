# Classic TUI Mode — Low-Level Design

Status: Draft  
Date: 2026-05-11

This document translates the high-level design into concrete implementation decisions. It is the reference an implementer should read before writing any code.

---

## File Map

Classic-mode code is split across two packages from the start. Presentational components (no store access) live in `packages/tui-components`. The layout root and hooks (which wire the store to those components) live in `packages/tui`.

```
packages/twinki/packages/twinki/src/reconciler/render.ts
  MODIFY — add rawWrite(lines: string[]): void to Instance

packages/tui-components/                    CREATE — new workspace package
  package.json                              name: "@kiro/tui-components", private: true
  src/
    classic/
      ClassicToolCall.tsx                   one-line tool call, props only
      ClassicApproval.tsx                   inline approval prompt, props only
      ClassicLiveRegion.tsx                 spinner | streaming text | idle
      ClassicPromptLine.tsx                 > prefix + PromptInput slot
      ClassicInlineAutocomplete.tsx         single-line / and @ suggestion
    storybook/
      run-storybook.tsx                     entry: bun run storybook
      Storybook.tsx                         browser (Shared / Modern / Classic sections)
      stories.ts                            registry including classic stories

packages/tui/src/
  constants/settings.ts                    MODIFY — add CHAT_UI_MODE
  index.tsx                                MODIFY — resolve uiMode, pass to ThemeProvider + render
  theme/ThemeProvider.tsx                  MODIFY — accept classicMode prop, expose via useTheme()
  components/layout/AppContainer.tsx       MODIFY — add classic branch

  components/layout/classic/              CREATE — wiring layer only (store → tui-components)
    index.ts                              re-exports ClassicLayout
    ClassicLayout.tsx                     root: store subscription + rawWrite + live region routing
    hooks/
      useClassicFlush.ts                  liveContent split, paragraph boundary, rawWrite callback
    render/                               pure functions — extraction target → packages/chat-classic
      renderToLines.ts                    message/tool/approval/summary → ANSI string[]
      ClassicCommandFormatter.ts          slash command result → ANSI string[]
      paragraphFlush.ts                   paragraph boundary detection
    __tests__/
      renderToLines.test.ts
      ClassicCommandFormatter.test.ts
      paragraphFlush.test.ts

packages/tui/e2e_tests/
  classic-mode-basic.test.ts
  classic-mode-streaming.test.ts
  classic-mode-tools.test.ts
  classic-mode-approvals.test.ts
  classic-mode-slash-commands.test.ts
  classic-mode-input.test.ts
  classic-mode-pipe.test.ts
```

### Layer boundaries

```
packages/tui-components/classic/     ← props only, no store, no hooks
        ↑ imported by
packages/tui/layout/classic/         ← reads store, calls rawWrite, passes props down
        ↑ imported by
packages/tui/AppContainer            ← routes to ClassicLayout when uiMode==='classic'
```

`render/` inside `packages/tui/layout/classic/` is the second extraction target (→ `packages/chat-classic`). It has no React or store imports — pure functions only.

`ClassicLayout` is the only export from `classic/index.ts`. `AppContainer` imports exactly one thing from this directory.



---

## Step 0 — Feature Flag Gate (`settings.ts` + `index.tsx`)

This step must be done first. It ensures every subsequent merge is safe to ship.

### `settings.ts`

Add one constant alongside the existing ones:

```ts
CHAT_CLASSIC_UI_ENABLED: 'chat.classicUiEnabled',
```

### `index.tsx`

Before the `resolveUiMode()` call, add:

```ts
const classicUiEnabled =
  process.env.KIRO_CLASSIC_UI_ENABLED === '1' ||
  readBoolSetting(Settings.CHAT_CLASSIC_UI_ENABLED, false);

const uiMode = classicUiEnabled ? resolveUiMode() : 'modern';
```

That is the entire gate. When `classicUiEnabled` is false, `uiMode` is always `'modern'` and `ClassicLayout` is never imported or rendered. The classic code path is completely dead.

**Enable for development:**
```bash
KIRO_CLASSIC_UI_ENABLED=1 bun run dev
# or persist:
q settings chat.classicUiEnabled true
```

The flag is permanent — classic mode is always opt-in. There is no GA removal step.

---

## Step 1 — Mode Resolution (`index.tsx` + `settings.ts`)

### `settings.ts`

Add one constant:

```ts
CHAT_UI_MODE: 'chat.ui.mode',   // values: 'modern' | 'classic'
```

### `index.tsx`

After the existing `wrapDisabled` resolution block, add:

```ts
type UiMode = 'modern' | 'classic';

function resolveUiMode(): UiMode {
  const fromEnv = process.env.KIRO_UI_MODE;
  if (fromEnv === 'classic' || fromEnv === 'modern') return fromEnv;
  const fromCli = cliArgs.uiMode; // see cli-args.ts note below
  if (fromCli === 'classic' || fromCli === 'modern') return fromCli;
  const fromSetting = readStringSetting(Settings.CHAT_UI_MODE, 'modern');
  return fromSetting === 'classic' ? 'classic' : 'modern';
}

const uiMode = resolveUiMode();
// Classic mode forces wrapDisabled — overflow wrapping is required for scrollback correctness.
const effectiveWrapDisabled = wrapDisabled || uiMode === 'classic';
```

Replace the existing `wrapDisabled` references in the `render()` call and `ThemeProvider` with `effectiveWrapDisabled`. Pass `uiMode` to `ThemeProvider` as a new `classicMode` prop.

### `cli-args.ts`

Add `--classic` and `--modern` flags to `parseCliArgs()`. They set `cliArgs.uiMode`. This is a small addition to the existing yargs/meow parsing — follow the existing pattern for boolean flags.

### `readStringSetting`

`cli-settings.ts` currently only has `readBoolSetting`. Add `readStringSetting(key, defaultValue)` following the same pattern.

---

## Step 2 — Theme Context (`ThemeProvider.tsx`)

Add `classicMode: boolean` to `ThemeProviderProps` and to `ThemeContextValue`. Expose it via `useTheme()` the same way `wrapDisabled` is exposed. Components can then check `const { classicMode } = useTheme()` to conditionally skip chrome.

No other changes to the theme system.

---

## Step 3 — AppContainer Routing (`AppContainer.tsx`)

The existing routing block:

```tsx
{mode === 'inline' && <InlineLayout />}
{mode === 'expanded' && <ExpandedLayout />}
{mode === 'crew-monitor' && <CrewMonitorScreen />}
{mode === 'session-view' && <SessionViewScreen />}
```

Becomes:

```tsx
{mode === 'inline' && uiMode === 'modern' && <InlineLayout />}
{mode === 'inline' && uiMode === 'classic' && <ClassicLayout />}
{mode === 'expanded' && <ExpandedLayout />}
{mode === 'crew-monitor' && <CrewMonitorScreen />}
{mode === 'session-view' && <SessionViewScreen />}
```

`uiMode` is passed down from `index.tsx` via a new context or prop. The simplest approach is a React context `UiModeContext` with a single string value, created in `index.tsx` and consumed in `AppContainer`. Alternatively, store it in the Zustand store as a read-only field set at startup. The Zustand approach is simpler because `AppContainer` already reads from the store.

Recommended: add `uiMode: UiMode` to the app store's initial state (set once at startup, never mutated).

---

## Step 4 — `ClassicLayout.tsx`

This is the root component for classic mode. It composes the three sub-components and handles the trust-all-tools gate inline.

```tsx
export const ClassicLayout: React.FC = () => {
  const trustAllToolsRequested = useAppStore(s => s.trustAllToolsRequested);
  const trustAllToolsConfirmed = useAppStore(s => s.trustAllToolsConfirmed);

  return (
    <>
      <ClassicConversation />
      {trustAllToolsRequested && !trustAllToolsConfirmed
        ? <ClassicTrustGate />          // inline trust gate (see Step 7)
        : <ClassicStreamingRegion />}
      <ClassicPromptLine />
    </>
  );
};
```

The layout renders three things sequentially. Twinki renders them top-to-bottom. `ClassicConversation` produces `<Static>` items. `ClassicStreamingRegion` and `ClassicPromptLine` are in the live region.

---

## Step 5 — `renderToLines.ts` and the `rawWrite` path

Rather than managing a `<Static>` React array, classic mode writes finalized content directly to terminal scrollback via `instance.rawWrite()`. This bypasses the React reconciler entirely for committed content, keeping the live React tree to 5–10 nodes.

### 5a — Add `rawWrite` to Twinki `Instance`

In `packages/twinki/packages/twinki/src/reconciler/render.ts`, add to the `Instance` interface and object:

```ts
// Interface
rawWrite(lines: string[]): void;

// Implementation (inside the instance object literal)
rawWrite(lines: string[]) {
  tui.writeStaticLines(lines);
  tui.requestRender();
},
```

This is the only change to Twinki. `writeStaticLines` already handles cursor repositioning correctly — it writes above the live region without disturbing `previousLines`.

### 5b — `renderToLines.ts`

A pure TypeScript function (no React, no hooks) that converts a finalized message to ANSI-encoded line strings:

```ts
export function renderMessageToLines(message: FinalizedMessage, theme: ThemeContextValue): string[] 
export function renderToolCallToLine(tool: ToolCallInfo, theme: ThemeContextValue): string
export function renderApprovalToLines(question: string, answer: string, theme: ThemeContextValue): string[]
export function renderCommandOutputToLines(output: string): string[]
```

These functions use chalk directly (via `theme.getColor(...)`) and the existing `markdown.ts` utilities for agent message content. They do not use React or Yoga.

### 5c — Store subscription in `ClassicLayout`

`ClassicLayout` subscribes to the Zustand store and calls `rawWrite` when messages are finalized:

```ts
// Inside ClassicLayout, after getting the Instance via useTwinkiContext()
useEffect(() => {
  return useAppStore.subscribe((state, prev) => {
    const newlyFinalized = findNewlyFinalizedMessages(state, prev);
    for (const msg of newlyFinalized) {
      const lines = renderMessageToLines(msg, theme);
      instance.rawWrite(lines);
    }
  });
}, [instance, theme]);
```

`findNewlyFinalizedMessages` uses `computeFlushSet` to determine which messages crossed from dynamic to static since the last render. This is the same logic as `ConversationView` in modern mode, just applied outside of React rendering.

---

## Step 6 — `ClassicLiveRegion.tsx` and `useClassicFlush.ts`

### `useClassicFlush.ts`

This hook manages the split between flushed (rawWrite) and live (React) content within a single streaming agent message.

```ts
interface ClassicFlushState {
  liveContent: string;   // content in the live React region
}
```

On each streaming token, the hook appends to `liveContent`. It then checks for a paragraph boundary:

- A blank line (`\n\n`) — flush everything up to and including the blank line.
- A completed code fence — content ends with ` ``` ` on its own line.
- A completed table — content ends with a non-table-row line after a sequence of table rows.

When a boundary is found, the content up to the boundary is passed to `rawWrite` (via a callback from `ClassicLayout`) and removed from `liveContent`. The live region then contains only the content since the last flush.

When the message finishes (agent turn ends), the remaining `liveContent` is flushed via `rawWrite` regardless of paragraph boundaries.

The live region is also bounded by `MAX_LIVE_LINES = Math.min(terminalHeight - 2, 10)`. If `liveContent` would exceed this many lines, the oldest complete line is flushed even without a paragraph boundary.

### `ClassicLiveRegion.tsx`

Renders the currently-active live element. Exactly one of these is shown at a time:

- Thinking: `<Spinner /> thinking…` using the existing `Spinner` component.
- Streaming: `<Text wrap="overflow">{liveContent}</Text>` — plain text, no MarkdownRenderer in the live region (markdown is rendered by `renderToLines` when flushed).
- Nothing (between turns): renders nothing.

The component reads `isProcessing`, `isThinking`, and streaming content from the store. It uses `useClassicFlush` to get `liveContent` and the flush callback.

---

## Step 7 — `ClassicPromptLine.tsx`

Renders the inline input prompt. This is the `PromptInput` component rendered without the `PromptBar` wrapper.

```tsx
export const ClassicPromptLine: React.FC = () => {
  const isProcessing = useAppStore(s => s.isProcessing);
  if (isProcessing) return null;  // hide prompt while agent is working

  return (
    <Box>
      <Text color={theme.colors.primary}>{"> "}</Text>
      <PromptInput />
    </Box>
  );
};
```

`PromptInput` already handles slash commands, `@` mentions, history, and basic editing. It is reused as-is. The only difference from modern mode is that it is not wrapped in `PromptBar` and is not pinned at the bottom of the viewport.

When the user submits, the store's `sendMessage` action is called (same as modern mode). The submitted text is added to the conversation as a user message, which `ClassicConversation` picks up and renders into Static on the next render cycle.

---

## Step 8 — `ClassicToolCall.tsx`

Renders a single tool call as one line of text. The line updates in place while the tool is running, then flushes to Static when done.

```
⚙ read_file src/main.rs … running
⚙ read_file src/main.rs … done (0.3s)
⚙ bash: echo hello … failed
```

For subagent tool calls, prefix with the agent name:

```
⚙ [planner] read_file src/plan.md … done
```

The component receives a `ToolCallStatus` from the store. While `status === 'running'`, it renders in the live region with a spinner character. When `status === 'done'` or `'failed'`, it is flushed to Static.

---

## Step 9 — `ClassicApproval.tsx`

Renders an inline approval prompt when `pendingApproval` is set in the store.

```
Allow bash: rm -rf /tmp/x?
  [y] Yes, once   [n] No   [t] Trust all tools
> _
```

Uses `useKeypress` to capture `y`, `n`, `t` without requiring Enter. On keypress, calls the appropriate store action (`approveOnce`, `denyApproval`, `trustAllTools`). The approval question and answer are then flushed to Static as a `ConversationItem` of type `'approval'`.

---

## Step 10 — `ClassicCommandFormatter.ts`

A pure function (no React) that converts slash command results to plain text strings.

```ts
export function formatCommandOutput(result: CommandResult): string
```

Each command type has a formatter:

- `/help` → two-column table of command names and descriptions, using `table-layout.ts`.
- `/tools` → list of tool names with one-line descriptions.
- `/context` → list of context files with sizes.
- `/mcp` → list of MCP servers with status.
- `/clear` → no output (the store handles clearing the message list).
- `/settings` → key-value pairs.

The formatted string is pushed into the conversation as a `ConversationItem` of type `'command'` and rendered in `ClassicConversation` as plain `<Text>`.

---

## Step 11 — E2E Tests

Four test files, each focused on one concern. Follow the existing pattern in `e2e_tests/` using `E2ETestCase` and `AcpTestHelper`.

**`classic-mode-basic.test.ts`**
- Starts TUI with `KIRO_UI_MODE=classic`.
- Sends a message, verifies the response appears in scrollback.
- Verifies no StatusBar chrome (no colored left-border characters).
- Verifies the prompt appears inline after the response.

**`classic-mode-tools.test.ts`**
- Sends a message that triggers a tool call.
- Verifies the tool call line appears with the correct format.
- Verifies the tool result is in scrollback after completion.

**`classic-mode-approvals.test.ts`**
- Triggers an approval request.
- Sends `y` keypress.
- Verifies the approval is recorded and the conversation continues.

**`classic-mode-slash-commands.test.ts`**
- Sends `/help`.
- Verifies the output appears inline (not as a panel overlay).
- Verifies the output contains expected command names.

---

## Dependency Graph (implementation order)

```
Step 1 (settings + mode resolution)
  → Step 2 (ThemeProvider classicMode prop)
    → Step 3 (AppContainer routing)
      → Step 4 (ClassicLayout skeleton)
        → Step 5 (ClassicConversation + Static items)
        → Step 6 (useClassicFlush + ClassicStreamingRegion)
        → Step 7 (ClassicPromptLine)
        → Step 8 (ClassicToolCall)
        → Step 9 (ClassicApproval)
        → Step 10 (ClassicCommandFormatter)
          → Step 11 (E2E tests)
```

Steps 5–10 can be developed in parallel once Step 4 is in place. Each is independently testable with a unit test before the E2E tests in Step 11.

---

## Key Invariants to Preserve

1. `<Static>` items are append-only. Never remove or reorder items. Use `adjustStaticCursor()` when trimming the front.
2. The live region contains at most one active element at a time (streaming OR thinking OR prompt OR approval — never two simultaneously).
3. `PromptInput` is hidden while `isProcessing` is true. The user cannot submit a new message while the agent is working.
4. `wrapDisabled` is always `true` in classic mode. Never render with word-wrap in classic mode.
5. Modern mode (`InlineLayout` and all its children) is never modified. The only changes to existing files are additive.

---

## Future: Package Extraction (`packages/chat-classic`)

The `classic/render/` sub-layer is designed to be extracted into a standalone package once the feature is stable. This section describes what that extraction looks like so the initial implementation is written with the right boundaries from day one.

### Target package structure

```
packages/chat-classic/
  package.json          name: "@kiro/chat-classic", private: true
  tsconfig.json
  src/
    render/             moved verbatim from packages/tui/src/components/layout/classic/render/
      renderToLines.ts
      ClassicCommandFormatter.ts
      paragraphFlush.ts
    index.ts            exports all public render functions
  __tests__/            moved verbatim from classic/__tests__/
```

### What moves and what stays

`render/` moves entirely. It has no dependencies on `react`, `twinki`, `zustand`, or any `@kiro/tui` internal. Its only dependencies are `chalk` and the existing `markdown.ts` / `table-layout.ts` utilities — both of which would also move or be re-exported.

`layout/` stays in `packages/tui`. The React components depend on the Zustand store, `PromptInput`, `useKeypress`, and other TUI-specific infrastructure that is not extractable without a much larger refactor.

`hooks/` stays in `packages/tui` for the same reason.

### Why this boundary matters now

Writing `render/` as pure functions from the start — no React context, no store access, no side effects — costs nothing during implementation and makes the extraction a mechanical file move later. The alternative (mixing rendering logic into React components) would require a refactor to extract.

The concrete rule for implementers: if a function in `render/` needs to import anything from `../../../stores/` or `react`, it belongs in `layout/` or `hooks/` instead.

### Extraction trigger

Extract when classic mode has been in production for one release cycle and the render functions are stable. The extraction enables:

- Other consumers (e.g., a future `kiro pipe` command, a test harness, a non-Twinki renderer) to use the same formatting logic without depending on the full TUI package.
- Independent versioning and testing of the render layer.
- Cleaner dependency graph: `packages/tui` depends on `packages/chat-classic`, not the other way around.
