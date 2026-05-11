# Classic TUI Mode — Task Canvas

Status: In Progress  
Last Updated: 2026-05-11  
Design Doc: [docs/design/classic-tui-mode.md](classic-tui-mode.md)  
Low-Level Design: [docs/design/classic-tui-mode-lld.md](classic-tui-mode-lld.md)

---

## How to Use This Canvas

Each task has a status marker:

- `[ ]` Not started
- `[~]` In progress
- `[x]` Done
- `[!]` Blocked — see note

When you start a task, change `[ ]` to `[~]`. When you finish it, change to `[x]`. Add a one-line note under any blocked task explaining what is needed.

Before starting a week, read the "Guidance" section for that week. It explains what to build, what to reuse, and what to watch out for. The LLD document has the concrete implementation details for each step.

---

## Week 1 — Foundation: Mode Resolution and Routing

**Goal:** By end of week, running `KIRO_UI_MODE=classic kiro chat` launches the app without crashing and routes to a placeholder `ClassicLayout` component. Modern mode is completely unchanged.

**Guidance:**
Read LLD Steps 1–4 before starting. The work is entirely additive — you are not modifying any existing behavior, only adding new code paths. The most important thing to get right is the `resolveUiMode()` function and the `AppContainer` routing branch. Test by running the TUI manually with `KIRO_UI_MODE=classic` and verifying it starts without errors.

### Tasks

- [ ] **W1-1** Add `CHAT_UI_MODE` constant to `packages/tui/src/constants/settings.ts`
  - Add `CHAT_UI_MODE: 'chat.ui.mode'` alongside the existing constants.
  - No other changes to this file.

- [ ] **W1-2** Add `readStringSetting` to `packages/tui/src/utils/cli-settings.ts`
  - Follow the same pattern as `readBoolSetting`.
  - Returns the string value of a setting key, or the provided default.
  - Add a unit test in `cli-settings.test.ts`.

- [ ] **W1-3** Add `--classic` / `--modern` CLI flags to `packages/tui/src/utils/cli-args.ts`
  - Add `uiMode?: 'classic' | 'modern'` to the `CliArgs` type.
  - Parse `--classic` as `uiMode: 'classic'` and `--modern` as `uiMode: 'modern'`.
  - Add a test case in `cli-args.test.ts`.

- [ ] **W1-4** Add `resolveUiMode()` to `packages/tui/src/index.tsx`
  - Resolution order: CLI flag → `KIRO_UI_MODE` env var → `readStringSetting(Settings.CHAT_UI_MODE, 'modern')`.
  - Classic mode forces `effectiveWrapDisabled = true`.
  - Store `uiMode` in the Zustand store initial state as a read-only field (set once, never mutated).
  - See LLD Step 1 for the exact code shape.

- [ ] **W1-5** Add `classicMode: boolean` to `ThemeProvider` and `ThemeContextValue`
  - Follow the exact same pattern as `wrapDisabled` — it is already in `ThemeProvider.tsx`.
  - Pass `classicMode={uiMode === 'classic'}` from `index.tsx`.
  - Add a test in `ThemeProvider.test.ts` verifying the value is exposed via `useTheme()`.

- [ ] **W1-6** Create `packages/tui/src/components/layout/classic/` directory with a placeholder `ClassicLayout.tsx`
  - The placeholder renders `<Text>Classic mode — coming soon</Text>`.
  - Export it from `classic/index.ts`.

- [ ] **W1-7** Add the classic routing branch to `AppContainer.tsx`
  - Read `uiMode` from the Zustand store.
  - When `mode === 'inline' && uiMode === 'classic'`, render `<ClassicLayout />`.
  - When `mode === 'inline' && uiMode === 'modern'`, render `<InlineLayout />` (existing behavior).
  - No other changes to `AppContainer.tsx`.

- [ ] **W1-8** Manual smoke test
  - Run `KIRO_UI_MODE=classic bun packages/tui/src/index.tsx` (or equivalent dev command).
  - Verify the placeholder text appears and the process does not crash.
  - Run `bun test` and verify all existing tests still pass.

---

## Week 2 — Raw Write Path and Live Region

**Goal:** By end of week, classic mode writes finalized content directly to terminal scrollback via `rawWrite`, and the live React region shows the thinking indicator and streaming text. The viewport never overflows.

**Guidance:**
Read the raw rendering research doc (`docs/design/classic-tui-mode-raw-rendering.md`) before starting. The key insight is that `ClassicConversation` is NOT a React component — it is a Zustand store subscription that calls `instance.rawWrite()` when messages are finalized. The React tree for classic mode only ever contains the live region (spinner, streaming text, prompt). Start with the Twinki change (W2-1), then `renderToLines.ts` (W2-2), then wire them together in `ClassicLayout` (W2-3), then add the live region (W2-4).

### Tasks

- [ ] **W2-1** Add `rawWrite` to Twinki `Instance`
  - In `packages/twinki/packages/twinki/src/reconciler/render.ts`, add `rawWrite(lines: string[]): void` to the `Instance` interface.
  - Implement it: `rawWrite(lines) { tui.writeStaticLines(lines); tui.requestRender(); }`.
  - This is the only change to Twinki. Two lines of code.
  - Verify the existing Twinki tests still pass.

- [ ] **W2-2** Implement `renderToLines.ts`
  - Pure TypeScript functions (no React, no hooks):
    - `renderUserMessageToLines(text, theme): string[]` — `> text` in primary color.
    - `renderAgentMessageToLines(content, theme): string[]` — markdown rendered to ANSI strings using `markdown.ts` utilities + chalk.
    - `renderToolCallToLine(tool, theme): string` — `⚙ tool_name: description … done`.
    - `renderApprovalToLines(question, answer, theme): string[]` — question + answer.
    - `renderCommandOutputToLines(output): string[]` — plain text lines.
  - Write unit tests for each function. These are pure functions — easy to test.

- [ ] **W2-3** Wire `rawWrite` into `ClassicLayout`
  - Get the `Instance` via `useTwinkiContext()` (which returns `{ tui, exit, adjustStaticCursor }`). Note: `rawWrite` is on `Instance`, not on `tui` directly — access it via the instance ref stored during mount.
  - Subscribe to the Zustand store. When `computeFlushSet` identifies newly finalized messages, call `renderToLines` and then `instance.rawWrite(lines)`.
  - Verify that sending a message and getting a response writes the content to scrollback.

- [ ] **W2-4** Implement `useClassicFlush.ts`
  - Manages the live region content during streaming.
  - Detects paragraph boundaries: blank line, completed code fence, completed table.
  - On boundary: calls `rawWrite` with the flushed content, clears it from `liveContent`.
  - Bounds the live region to `MAX_LIVE_LINES = Math.min(terminalHeight - 2, 10)`.
  - Write unit tests for boundary detection (pure logic, no React needed).

- [ ] **W2-5** Implement `ClassicLiveRegion.tsx`
  - Thinking: `<Spinner /> thinking…`.
  - Streaming: `<Text wrap="overflow">{liveContent}</Text>` (plain text, not MarkdownRenderer — markdown is rendered at flush time by `renderToLines`).
  - Idle: renders nothing.
  - Uses `useClassicFlush` for `liveContent`.

- [ ] **W2-6** Wire `ClassicLiveRegion` into `ClassicLayout`
  - Replace the placeholder with `<ClassicLiveRegion />`.
  - Verify the thinking indicator appears and streaming text shows in the live region.

- [ ] **W2-7** Manual end-to-end test
  - Start classic mode, send a message, verify the response streams in the live region and then appears in scrollback when the turn ends.
  - Verify the live region never exceeds 10 lines.
  - Verify copy-paste from scrollback works (no ANSI artifacts, no cursor repositioning garbage).

---

## Week 3 — Input, Tool Calls, and Approvals

**Goal:** By end of week, the user can type and submit messages, tool calls display inline, and approval requests work with a single keypress.

**Guidance:**
Read LLD Steps 7, 8, and 9. `ClassicPromptLine` reuses `PromptInput` directly — the hard work of input editing, history, and slash command triggering is already done. `ClassicToolCall` is a simple one-line renderer. `ClassicApproval` is the most complex piece because it needs to handle the keypress and call the right store action.

### Tasks

- [ ] **W3-1** Implement `ClassicPromptLine.tsx`
  - Renders `> ` prefix followed by `<PromptInput />`.
  - Hidden while `isProcessing` is true.
  - See LLD Step 7.

- [ ] **W3-2** Wire `ClassicPromptLine` into `ClassicLayout`
  - Add it below `ClassicStreamingRegion`.
  - Verify the user can type and submit a message.

- [ ] **W3-3** Implement `ClassicToolCall.tsx`
  - Renders one line: `⚙ tool_name: brief_description … [running|done|failed]`.
  - For subagent tools: `⚙ [agent-name] tool_name: …`.
  - While running: spinner character before the status word.
  - When done/failed: flush to Static via `ClassicConversation`.
  - See LLD Step 8.

- [ ] **W3-4** Wire tool calls into `ClassicConversation`
  - Add `ToolUse` messages to the `ConversationItem` type.
  - Render them using `ClassicToolCall`.
  - Verify tool calls appear inline and flush to scrollback when complete.

- [ ] **W3-5** Implement `ClassicApproval.tsx`
  - Renders the approval question and options inline.
  - Uses `useKeypress` to capture `y`, `n`, `t`.
  - Calls the appropriate store action on keypress.
  - Flushes the question + answer to Static as a `ConversationItem` of type `'approval'`.
  - See LLD Step 9.

- [ ] **W3-6** Wire `ClassicApproval` into `ClassicLayout`
  - When `pendingApproval` is set in the store, render `ClassicApproval` instead of `ClassicStreamingRegion`.
  - Verify the approval flow works end-to-end.

- [ ] **W3-7** Inline trust-all-tools gate
  - Replace the `TrustAllToolsGate` overlay with an inline prompt in classic mode.
  - Reuse the same store actions (`confirmTrustAllTools`, `saveTrustGateAccepted`).
  - The prompt format: `Trust all tools for this session? [y] Yes  [a] Always  [n] No`.

- [ ] **W3-8** Manual end-to-end test
  - Verify the full conversation loop: type → submit → tool call → approval → response → scrollback.
  - Verify `Ctrl+C` cancels a running turn (existing `cancelMessage` action).

---

## Week 4 — Slash Commands and Polish

**Goal:** By end of week, all slash commands work in classic mode and produce inline text output. The feature is ready for review.

**Guidance:**
Read LLD Step 10. `ClassicCommandFormatter` is a pure function — no React, no store access. It takes the structured result from the command dispatcher and returns a formatted string. Start with `/help` (most important) and `/clear`, then add the others. The E2E tests in Step 11 are the acceptance criteria for the whole feature.

### Tasks

- [ ] **W4-1** Implement `ClassicCommandFormatter.ts`
  - Pure function: `formatCommandOutput(result: CommandResult): string`.
  - `/help` → two-column table using `table-layout.ts`.
  - `/tools` → list of tool names and descriptions.
  - `/context` → list of context files with sizes.
  - `/mcp` → list of MCP servers with status.
  - `/settings` → key-value pairs.
  - `/clear` → empty string (store handles the clear).
  - Write unit tests for each formatter.

- [ ] **W4-2** Wire command output into `ClassicConversation`
  - When a slash command completes, push a `ConversationItem` of type `'command'` with the formatted output.
  - Render it as plain `<Text>` in the Static list.
  - Verify `/help` output appears inline.

- [ ] **W4-3** Verify `/clear` works in classic mode
  - `/clear` should clear the Static items array and reset the conversation.
  - Use `adjustStaticCursor()` after clearing to keep Twinki's cursor in sync.

- [ ] **W4-4** Welcome line
  - On startup, push a single welcome line to Static: `Kiro v{version} — classic mode. Type /help for commands.`
  - Use `version.ts` for the version string.
  - Only shown once, not on `/clear`.

- [ ] **W4-5** Pipe-safe degradation
  - When `process.stdout.isTTY` is false, suppress spinners and colors (respect `NO_COLOR`).
  - Verify `kiro chat --classic "hello" --no-interactive | cat` produces clean output.

- [ ] **W4-6** Write E2E test: `classic-mode-basic.test.ts`
  - Start TUI with `KIRO_UI_MODE=classic`.
  - Send a message, verify response in scrollback.
  - Verify no StatusBar chrome.
  - Verify prompt appears inline after response.

- [ ] **W4-7** Write E2E test: `classic-mode-tools.test.ts`
  - Trigger a tool call, verify the one-line format.
  - Verify tool result is in scrollback after completion.

- [ ] **W4-8** Write E2E test: `classic-mode-approvals.test.ts`
  - Trigger an approval, send `y`, verify conversation continues.

- [ ] **W4-9** Write E2E test: `classic-mode-slash-commands.test.ts`
  - Send `/help`, verify inline output with expected command names.
  - Send `/clear`, verify conversation is cleared.

- [ ] **W4-10** Final regression check
  - Run the full existing test suite (`bun test`, `bun run e2e`).
  - Verify zero regressions in modern mode.
  - Verify all four new E2E tests pass.

---

## Completion Criteria

The feature is complete when all of the following are true:

1. All tasks above are marked `[x]`.
2. `KIRO_UI_MODE=classic kiro chat` starts a working classic-mode session.
3. `kiro chat` (no flag) starts modern mode unchanged.
4. All four new E2E tests pass.
5. All existing tests pass.
6. The output of `kiro chat --classic "hello" --no-interactive | cat` is clean plain text with no ANSI escape sequences (when `NO_COLOR=1` is set).

---

## Notes and Decisions Log

Use this section to record decisions made during implementation that deviate from or extend the design documents.

| Date | Decision | Rationale |
|------|----------|-----------|
| — | — | — |
