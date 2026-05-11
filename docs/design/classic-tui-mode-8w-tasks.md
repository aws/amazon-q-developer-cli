# Classic TUI Mode — 8-Week Task Canvas

Status: Planning  
Date: 2026-05-11  
Design: [classic-tui-mode.md](classic-tui-mode.md)  
LLD: [classic-tui-mode-lld.md](classic-tui-mode-lld.md)  
Raw Rendering: [classic-tui-mode-raw-rendering.md](classic-tui-mode-raw-rendering.md)

---

## Directory Structure

```
packages/tui-components/src/
  classic/          ClassicToolCall, ClassicApproval, ClassicLiveRegion,
                    ClassicPromptLine, ClassicInlineAutocomplete
  storybook/        Storybook.tsx, stories.ts (Shared / Modern / Classic sections)

packages/tui/src/components/layout/classic/
  ClassicLayout.tsx         store wiring root
  hooks/useClassicFlush.ts  liveContent + rawWrite callback
  render/                   pure functions → future packages/chat-classic
```

Rule: nothing in `tui-components/classic/` or `render/` may import from `zustand` or any store file.

---

Status markers: `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked

---

## Week 1 — Foundation, Benchmark, Package Skeleton

**Goal:** Mode resolution, feature flag, routing, benchmark, and `packages/tui-components` skeleton.

**Demo:** `KIRO_CLASSIC_UI_ENABLED=1 KIRO_UI_MODE=classic bun run dev` shows placeholder. `bun run storybook` from `tui-components` shows modern stories. Benchmark results table printed.

- [ ] **W1-0** Feature flag gate: `CHAT_CLASSIC_UI_ENABLED` setting + `KIRO_CLASSIC_UI_ENABLED` env var. Default false. Must be first task.
- [ ] **W1-BENCH** Design and run rendering strategy benchmark (A/B/C); write `classic-rendering-benchmark-results.json`; record chosen path
- [ ] **W1-1** Add `CHAT_UI_MODE: 'chat.ui.mode'` to `constants/settings.ts`
- [ ] **W1-2** Add `readStringSetting(key, default)` to `cli-settings.ts` + unit test
- [ ] **W1-3** Add `--classic` / `--modern` CLI flags to `cli-args.ts` + unit test
- [ ] **W1-4** Add `resolveUiMode()` to `index.tsx`; classic forces `effectiveWrapDisabled=true`; store `uiMode` in Zustand
- [ ] **W1-5** Add `classicMode: boolean` to `ThemeProvider` + `ThemeContextValue`; expose via `useTheme()`; test
- [ ] **W1-6** Create `packages/tui-components/` workspace package with storybook
- [ ] **W1-7** Create `ClassicLayout` placeholder in `packages/tui/src/components/layout/classic/`
- [ ] **W1-8** Wire classic routing in `AppContainer.tsx`
- [ ] **W1-9** Smoke test: placeholder renders; storybook works; `bun test` passes

---

## Week 2 — Rendering Infrastructure: rawWrite, Messages, Live Region, Streaming

**Goal:** Full conversation loop works: messages render to scrollback via `rawWrite`, live region shows thinking/streaming, paragraph flush keeps viewport bounded.

**Demo:** Send a message → thinking spinner → streaming (≤10 lines) → full response in scrollback. Turn summary prints. Scroll up to verify all content. `getMetrics().yogaNodeCount ≤ 10`.

- [ ] **W2-1** Add `rawWrite(lines)` to Twinki `Instance` *(skip if path A)*
- [ ] **W2-2** Implement `renderUserMessageToLines(text, theme): string[]` — `> text` in primary color; unit test
- [ ] **W2-3** Implement `renderAgentMessageToLines(content, theme): string[]` — markdown → ANSI; unit tests
- [ ] **W2-4** Implement `renderSystemMessageToLines(content, success, theme): string[]`; unit test
- [ ] **W2-5** Implement `renderTurnSummaryToLines(summary, theme): string[]` — `↳ 123 in · 456 out`; unit test
- [ ] **W2-6** Implement `renderToolCallToLine(tool, elapsed, theme): string`; unit test
- [ ] **W2-7** Implement `renderApprovalToLines(question, answer, theme): string[]`; unit test
- [ ] **W2-8** Implement `findNewlyFinalizedMessages(state, prev)` using `computeFlushSet`; unit tests
- [ ] **W2-9** Wire store subscription in `ClassicLayout`: finalized messages → `renderToLines` → `rawWrite`
- [ ] **W2-10** Wire turn summary + compaction summary to `rawWrite`
- [ ] **W2-11** Implement `paragraphFlush.ts` boundary detection (blank line, code fence, table); unit tests
- [ ] **W2-12** Implement `useClassicFlush` hook: tracks `liveContent`, calls `onFlush` on boundary, bounds to `MAX_LIVE_LINES`
- [ ] **W2-13** Implement `ClassicLiveRegion.tsx`: thinking → spinner; streaming → `<Text wrap="overflow">`; idle → nothing
- [ ] **W2-14** Wire `ClassicLiveRegion` into `ClassicLayout`; connect `onFlush` to `rawWrite`
- [ ] **W2-15** Add `ClassicLiveRegion` stories to storybook (thinking, streaming, idle)
- [ ] **W2-16** Smoke test: long response stays ≤10 lines live; full response in scrollback; turn summary prints

---

## Week 3 — Input, Keybindings, Tool Calls

**Goal:** Full interactive input with Emacs keybindings, history, cancel/suspend. Tool calls display inline with live output.

**Demo:** Type with Ctrl+A/E/K/U, submit, see echo. Up-arrow recalls history. Ctrl+C cancels turn. Tool call shows `⚙ read_file … ⠋ running` → `… done (0.2s)`. Shell command shows live output. Write tool shows `✎ path (+N/-M)`.

- [ ] **W3-1** Implement `ClassicPromptLine.tsx`: `> ` + `<PromptInput />`; hidden while `isProcessing`; story
- [ ] **W3-2** Wire `ClassicPromptLine` into `ClassicLayout`
- [ ] **W3-3** Verify Ctrl+A / Ctrl+E — move to line start/end
- [ ] **W3-4** Verify Ctrl+K / Ctrl+U — kill to end/start
- [ ] **W3-5** Verify Ctrl+W — kill word backward
- [ ] **W3-6** Verify Alt+F / Alt+B — forward/backward word
- [ ] **W3-7** Verify Ctrl+Y — yank from kill ring
- [ ] **W3-8** Verify up/down arrow history
- [ ] **W3-9** Wire Ctrl+C cancel (first cancels turn, second exits)
- [ ] **W3-10** Wire Ctrl+Z suspend (`fg` resumes)
- [ ] **W3-11** Verify bracketed paste
- [ ] **W3-12** Implement `ClassicToolCall.tsx` (live): `⚙ name: desc … ⠋ running`; subagent prefix; spinner; stories
- [ ] **W3-13** Implement shell tool live output: last N lines below tool call line, bounded
- [ ] **W3-14** Implement write/diff tool display: `✎ path (+N/-M lines)`
- [ ] **W3-15** Wire tool call lifecycle: running → live region; finished → `rawWrite`; multiple tools stack
- [ ] **W3-16** Smoke test: Emacs editing, history, cancel, suspend; tool calls with spinner and elapsed time

---

## Week 4 — Approvals, Trust Gate, Core Slash Commands

**Goal:** Approvals work with single keypress. Trust gate works inline. `/help`, `/tools`, `/context`, `/mcp`, `/usage`, `/hooks`, `/stats`, `/keybindings` all produce inline text.

**Demo:** Trigger approval → `Allow bash: …? [y/n/t]` → press `y` → continues. Startup trust gate inline. `/help` shows two-column table. `/tools` shows tool list. All output in scrollback.

- [ ] **W4-1** Implement `ClassicApproval.tsx`: inline prompt, `y`/`n`/`t` keypress, no Enter; stories
- [ ] **W4-2** Wire `ClassicApproval` into `ClassicLayout`: replaces live region when `pendingApproval` set
- [ ] **W4-3** After response: `rawWrite(renderApprovalToLines(...))`
- [ ] **W4-4** Implement inline trust-all-tools gate: `[y] Yes  [a] Always  [n] No`
- [ ] **W4-5** Implement `formatHelp(commands): string[]` — two-column table; unit test
- [ ] **W4-6** Implement `formatContext(data): string[]` — file | tokens | %; unit test
- [ ] **W4-7** Implement `formatTools(tools): string[]` — name | source | status; unit test
- [ ] **W4-8** Implement `formatMcp(servers): string[]` — name | status | tools; unit test
- [ ] **W4-9** Implement `formatUsage(data): string[]` — billing summary; unit test
- [ ] **W4-10** Implement `formatHooks(hooks): string[]` — trigger | command | matcher; unit test
- [ ] **W4-11** Implement `formatStats(stats, summary): string[]` — request stats table; unit test
- [ ] **W4-12** Implement `formatKeybindings(): string[]` — key | action; unit test
- [ ] **W4-13** Wire `CommandContext` override in `ClassicLayout`: `setShow*Panel` → `rawWrite(format*(data))`
- [ ] **W4-14** Smoke test: approval flow; trust gate; `/help`, `/context`, `/tools`, `/mcp`, `/usage`, `/hooks`, `/stats`, `/keybindings`

---

## Week 5 — Session Commands, Editor, Autocomplete

**Goal:** All remaining slash commands work: `/clear`, `/model`, `/agent`, `/chat`, `/compact`, `/plan`, `/editor`, `/paste`, `/theme`, `/settings`, `/prompts`, `/tangent`. Inline autocomplete for `/` and `@`.

**Demo:** `/clear` resets. `/model` shows numbered list, keypress selects. `/editor` opens $EDITOR. Type `/con` → see `→ /context` suggestion. Type `@src` → see file suggestion. `/theme` shows presets.

- [ ] **W5-1** Wire `/clear`: `clearMessages()` + `instance.clear()` + print `Conversation cleared.`
- [ ] **W5-2** Wire `/quit` and `/exit`
- [ ] **W5-3** Wire `/model` inline selection: numbered list, keypress selects, confirmation printed
- [ ] **W5-4** Wire `/agent` inline selection: same pattern as `/model`
- [ ] **W5-5** Wire `/chat save`: prints confirmation inline
- [ ] **W5-6** Wire `/chat load`: loads session, history replayed to scrollback
- [ ] **W5-7** Wire `/chat list`: prints session list inline
- [ ] **W5-8** Wire `/compact`: prints `Compacting…` then summary
- [ ] **W5-9** Wire `/plan`: switches to planner agent, prints confirmation
- [ ] **W5-10** Wire `/tangent`: toggle tangent mode, print status
- [ ] **W5-11** Wire backend-registered commands (prompts, skills): forwarded as messages
- [ ] **W5-12** Wire `/editor`: opens `$EDITOR`, submits on close
- [ ] **W5-13** Wire `/paste`: reads clipboard image, attaches to next message
- [ ] **W5-14** Implement inline slash autocomplete: `/` trigger → single-line suggestion using `fuzzyScore.ts`
- [ ] **W5-15** Implement inline `@` file mention autocomplete: single-line suggestion using `file-search.ts`
- [ ] **W5-16** Wire `/theme` inline selection: numbered list, preview, confirmation
- [ ] **W5-17** Wire `/settings` inline menu: print settings, arrow keys, Enter to toggle
- [ ] **W5-18** Wire `/prompts`: print available prompts inline (`formatPrompts`)
- [ ] **W5-19** Smoke test: all commands work; autocomplete for `/` and `@`; `/editor` and `/theme`

---

## Week 6 — Notifications, Lifecycle, Pipe Safety, Polish

**Goal:** Welcome line, changelog, session resume, MCP/OAuth/error notifications, non-interactive mode, pipe-safe output, resize handling, remaining commands.

**Demo:** Welcome line on startup. MCP failure warning inline. Resume session → history in scrollback. `NO_COLOR=1 kiro chat --classic "hello" --no-interactive | cat` → clean text. Resize during streaming → no corruption.

- [ ] **W6-1** Welcome line: `Kiro vX.Y.Z — classic mode  /help for commands`; suppressed when `!isTTY`
- [ ] **W6-2** Changelog announcement: print inline after welcome if new version
- [ ] **W6-3** Session history replay: on `--resume`, history events → `rawWrite` in order
- [ ] **W6-4** MCP server failure notification: `⚠ MCP server 'name' failed: reason`; once per failure
- [ ] **W6-5** OAuth pending notification: `⚠ OAuth required for 'name'. Visit: <url>  [c] Copy URL`
- [ ] **W6-6** Rate limit / error messages: `agentError` printed inline with guidance; cleared after
- [ ] **W6-7** Non-interactive mode: verify auto-submit, exit after turn, error on approval
- [ ] **W6-8** Suppress spinners when `!isTTY`
- [ ] **W6-9** Suppress colors when `NO_COLOR` set (chalk handles automatically)
- [ ] **W6-10** Suppress welcome line and prompt when `!isTTY`
- [ ] **W6-11** Verify `less` compatibility: `kiro chat --classic "hello" | less`
- [ ] **W6-12** Verify `tee` compatibility: `--no-interactive | tee out.txt`
- [ ] **W6-13** Long line soft-wrap: `wrap="overflow"` + `wideLines: true`
- [ ] **W6-14** Resize handling: `onResize` redraws live region without corruption
- [ ] **W6-15** Wire `/code`: `formatCode(data)` inline
- [ ] **W6-16** Wire `/knowledge`: `formatKnowledge(entries)` inline
- [ ] **W6-17** Wire `/todos`: `formatTodos(todos)` inline
- [ ] **W6-18** Wire `/logdump` and `/changelog`: verify work without panels
- [ ] **W6-19** Smoke test: welcome line; MCP warning; resume; `--no-interactive | cat` clean; resize

---

## Week 7 — E2E Tests, Regression, Hardening

**Goal:** All E2E tests pass. Zero regressions in modern mode. Performance verified.

**Demo:** `bun run test:e2e` — all 7 classic tests green. `bun test` — full workspace passes. Modern mode side-by-side unchanged. `yogaNodeCount ≤ 15` during 10-turn conversation.

- [ ] **W7-1** E2E: `classic-mode-basic.test.ts` — response in scrollback; no StatusBar chrome; prompt inline; welcome line
- [ ] **W7-2** E2E: `classic-mode-streaming.test.ts` — live region ≤10 lines; paragraph flush; code fence not split
- [ ] **W7-3** E2E: `classic-mode-tools.test.ts` — spinner; elapsed time; shell output; failed tool
- [ ] **W7-4** E2E: `classic-mode-approvals.test.ts` — inline prompt; `y` continues; `n` denies; trust gate
- [ ] **W7-5** E2E: `classic-mode-slash-commands.test.ts` — `/help`, `/clear`, `/context`, `/tools`, `/mcp`
- [ ] **W7-6** E2E: `classic-mode-input.test.ts` — Ctrl+A/E/K/U; history; autocomplete; Ctrl+C
- [ ] **W7-7** E2E: `classic-mode-pipe.test.ts` — `--no-interactive` exits; piped output clean
- [ ] **W7-8** Full regression: `bun test` across workspace — zero failures
- [ ] **W7-9** Manual regression: modern mode visually unchanged
- [ ] **W7-10** Performance: `yogaNodeCount ≤ 15`; no memory growth over 10 turns

---

## Week 8 (Future) — Package Extraction: tui-components + chat-classic

**Goal:** Move shared/modern components to `tui-components`. Extract pure render functions to `chat-classic`. Lint rules enforced.

**Demo:** `bun run storybook` from `tui-components` shows Shared / Modern / Classic sections. All imports resolve. Lint rule catches any `zustand` import in `tui-components` or `chat-classic`.

- [ ] **W8-1** Move shared primitives to `tui-components/src/shared/`: spinner, divider, icon, text, chip, alert, table, status, card, hint
- [ ] **W8-2** Add re-export stubs in `packages/tui` for each moved component
- [ ] **W8-3** Move modern components to `tui-components/src/modern/`: status-bar, message, tools, prompt-bar, notification-bar, radio, menu, brand, welcome-screen
- [ ] **W8-4** Add re-export stubs for modern components
- [ ] **W8-5** Update storybook registry: Shared / Modern / Classic sections
- [ ] **W8-6** Delegate `dev:storybook` script from `packages/tui` to `tui-components`
- [ ] **W8-7** Add lint rule to `tui-components`: no `zustand`, `../stores/`, `../kiro` imports
- [ ] **W8-8** Create `packages/chat-classic/` package: `package.json`, `tsconfig.json`, `src/index.ts`
- [ ] **W8-9** Move `renderToLines.ts`, `ClassicCommandFormatter.ts`, `paragraphFlush.ts` to `chat-classic/src/`
- [ ] **W8-10** Move unit tests to `chat-classic/__tests__/`
- [ ] **W8-11** Wire `@kiro/chat-classic` workspace dependency; update imports
- [ ] **W8-12** Add lint rule to `chat-classic`: no `react`, `zustand`, `@kiro/tui` imports
- [ ] **W8-13** Full test suite passes — zero failures

---

## Completion Criteria

1. All tasks above marked `[x]`
2. `KIRO_CLASSIC_UI_ENABLED=1 KIRO_UI_MODE=classic kiro chat` starts a working session with V1 feature parity
3. `kiro chat` (no flag) starts modern mode, visually and behaviorally unchanged
4. All 7 E2E test files pass
5. All existing tests pass (zero regressions)
6. `NO_COLOR=1 kiro chat --classic "hello" --no-interactive | cat` produces clean plain text
7. Live React tree ≤15 Yoga nodes during a conversation
8. `bun run storybook` from `tui-components` shows Shared / Modern / Classic sections
9. `tui-components/classic/` and `chat-classic/` have no store imports (lint enforced)
10. `docs/design/classic-rendering-benchmark-results.json` exists; implementation matches chosen path

---

## Notes and Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-05-11 | rawWrite bypass for committed content | Keeps React tree ≤10 nodes; matches V1 stdout model |
| 2026-05-11 | Markdown rendered at flush time, not in live region | One-time render on finalization; live region is plain text |
| 2026-05-11 | Inline single-line autocomplete for `/` and `@` | No overlay panels in classic mode |
| 2026-05-11 | Feature flag permanent opt-in | Always configurable; safe to merge at any stage |
| 2026-05-11 | Compressed from 12 weeks to 8 | More tasks per week; same total scope |
