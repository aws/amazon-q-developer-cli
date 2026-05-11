---
description: Classic TUI Mode interactive project driver. Run this to get a menu of actions based on current project state. Detects stage automatically and guides you through the right next step.
---

# Classic TUI Mode — Interactive Project Driver

## Entry Point

When this prompt is run, the agent MUST:

1. Read `docs/design/classic-tui-state.json`. If it does not exist, create it:
   ```json
   { "stage": "W1-BENCH", "week": 1, "renderingPath": null, "completedTasks": [], "notes": [] }
   ```
2. Compute a one-line status summary from the state file.
3. Present the **Main Menu** below. Wait for the user to choose.

---

## Main Menu

Present this every time the prompt is invoked (after showing the status line):

```
Classic TUI Mode — Project Driver
Status: Week [N] · Stage [stage] · Path [renderingPath or "TBD"] · [X/Y tasks done]

What would you like to do?

  1. Work on next task          (continue from where you left off)
  2. Check project status       (full progress report)
  3. Review a design document   (browse design docs)
  4. Review a component design  (deep-dive a specific component)
  5. Run a specific task        (jump to any task by ID)
  6. Mark a task complete       (record completed work)
  7. Add a note                 (log a decision or deviation)
  8. Show acceptance criteria   (project-level done checklist)

Enter a number:
```

Wait for input. Route to the handler below that matches the choice.

---

## Handler 1 — Work on Next Task

Read `docs/design/classic-tui-state.json`. Find the current `stage`. Match it to the task list below and present the task with its full acceptance criteria.

After presenting the task, ask:
> Ready to start, or do you want to see the implementation details first?

If "details": read the relevant section of `docs/design/classic-tui-mode-lld.md` and present it, then ask again.

If "ready": present the task instructions. After the user confirms completion:
- Mark the task `[x]` in `docs/design/classic-tui-mode-8w-tasks.md`
- Add the task ID to `completedTasks` in the state file
- Advance `stage` to the next task ID
- Return to the Main Menu

---

## Handler 2 — Check Project Status

Read `docs/design/classic-tui-mode-8w-tasks.md` and `docs/design/classic-tui-state.json`.

Present:
```
PROJECT STATUS — Classic TUI Mode
══════════════════════════════════════════════════════

Current week:     [N] of 12 (+ 3 future)
Current stage:    [stage]
Rendering path:   [A/B/C or TBD]
Tasks completed:  [X] of 124
Weeks complete:   [N]

WEEK PROGRESS
─────────────────────────────────────────────────────
Week 1  Foundation & Benchmark     [X/11]  [bar]
Week 2  Rendering & Live Region    [X/16]  [bar]
Week 3  Input & Tool Calls         [X/16]  [bar]
Week 4  Approvals & Slash (core)   [X/14]  [bar]
Week 5  Slash (session/editor/@)   [X/19]  [bar]
Week 6  Notifications & Pipe       [X/19]  [bar]
Week 7  E2E Tests & Hardening      [X/10]  [bar]
Week 8  Package Extraction         [X/13]  [future]

OPEN DECISIONS
─────────────────────────────────────────────────────
[List any null fields from state file]

RECENT NOTES
─────────────────────────────────────────────────────
[Last 3 entries from state.notes]
```

Then return to Main Menu.

---

## Handler 3 — Review a Design Document

Present:
```
Which design document?

  1. High-level design          docs/design/classic-tui-mode.md
  2. Low-level design (LLD)     docs/design/classic-tui-mode-lld.md
  3. Raw rendering analysis     docs/design/classic-tui-mode-raw-rendering.md
  4. tui-components package     docs/design/tui-components-package.md
  5. Full task canvas           docs/design/classic-tui-mode-8w-tasks.md
  6. Benchmark results          docs/design/classic-rendering-benchmark-results.json
```

Read the chosen file and present its contents. Then return to Main Menu.

---

## Handler 4 — Review a Component Design

Present:
```
Which component?

  1.  ClassicLayout             (root wiring, store subscription, rawWrite)
  2.  ClassicLiveRegion         (spinner / streaming text / idle)
  3.  ClassicPromptLine         (inline > prompt, PromptInput)
  4.  ClassicToolCall           (one-line tool display, spinner, live output)
  5.  ClassicApproval           (inline y/n/t keypress prompt)
  6.  ClassicInlineAutocomplete (single-line / and @ suggestion)
  7.  useClassicFlush           (paragraph boundary detection, rawWrite callback)
  8.  renderToLines             (pure render functions)
  9.  ClassicCommandFormatter   (slash command output formatters)
  10. paragraphFlush            (boundary detection logic)
```

Read `docs/design/classic-tui-mode-lld.md` and extract the relevant step for the chosen component. Present it with the file location, props/signature, acceptance criteria, and storybook stories needed.

Then return to Main Menu.

---

## Handler 5 — Run a Specific Task

Present:
```
Enter a task ID (e.g. W1-1, W3-2, W7-5) or type a keyword to search:
```

Match the input to the task list below. Present the full task with acceptance criteria. After completion, update state and canvas. Return to Main Menu.

---

## Handler 6 — Mark a Task Complete

Present:
```
Enter the task ID to mark complete (e.g. W2-3):
```

Mark it `[x]` in `docs/design/classic-tui-mode-8w-tasks.md`. Add to `completedTasks` in state file. If this was the last task in the current week, advance `week` and set `stage` to the first task of the next week. Return to Main Menu.

---

## Handler 7 — Add a Note

Present:
```
Enter your note (decision, deviation, or observation):
```

Append `{ "date": "[today]", "text": "[note]" }` to `state.notes`. Save state file. Return to Main Menu.

---

## Handler 8 — Show Acceptance Criteria

Read `docs/design/classic-tui-mode-8w-tasks.md` Completion Criteria section. Present the full project-level checklist with current pass/fail status for each item based on `completedTasks`. Return to Main Menu.

---

## Exhaustive Task List

This is the canonical list of all 115 tasks. Each has an ID, a one-line description, and acceptance criteria. The `stage` field in the state file always matches one of these IDs.

### Week 1 — Foundation, Benchmark, Package Skeleton

**W1-0** Add feature flag gate *(must be first — enables safe merges to main)*
- AC: `Settings.CHAT_CLASSIC_UI_ENABLED === 'chat.classicUiEnabled'`
- AC: `KIRO_CLASSIC_UI_ENABLED=1` env var will enable classic mode (fully wired in W1-4)
- AC: Default is `false` — classic code path is dead without the flag
- AC: `bun test` passes

**W1-BENCH** Design and run rendering strategy benchmark
- AC: Script at `packages/tui/scripts/benchmark-classic-rendering.ts` runs without errors
- AC: Prints table comparing approaches A (React/Twinki), B (rawWrite), C (Node.js stdio)
- AC: Writes `docs/design/classic-rendering-benchmark-results.json` with `decision.chosen`
- AC: User confirms or overrides the chosen path
- AC: `renderingPath` recorded in state file

**W1-1** Add `CHAT_UI_MODE` settings constant
- AC: `Settings.CHAT_UI_MODE === 'chat.ui.mode'` in `constants/settings.ts`
- AC: `bun test` passes

**W1-2** Add `readStringSetting` utility
- AC: Returns setting value or default; unit test covers present/absent/invalid
- AC: `bun test` passes

**W1-3** Add `--classic` / `--modern` CLI flags
- AC: `--classic` → `cliArgs.uiMode === 'classic'`; `--modern` → `'modern'`
- AC: Unit test; `bun test` passes

**W1-4** Add `resolveUiMode()` to `index.tsx`
- AC: Resolution order: CLI flag → `KIRO_UI_MODE` → setting → `'modern'`
- AC: Classic forces `effectiveWrapDisabled = true`
- AC: `uiMode` accessible via `useAppStore(s => s.uiMode)`

**W1-5** Add `classicMode` to `ThemeProvider` and `ThemeContextValue`
- AC: `useTheme().classicMode` is `true` in classic, `false` in modern
- AC: Existing ThemeProvider tests pass

**W1-6** Create `packages/tui-components` workspace package
- AC: `bun run storybook` from `packages/tui-components` starts without errors
- AC: Existing modern stories visible in browser
- AC: `bun test` passes across workspace

**W1-7** Create `ClassicLayout` placeholder
- AC: File exists at `components/layout/classic/ClassicLayout.tsx`
- AC: Renders `<Text>Classic mode — coming soon</Text>`

**W1-8** Wire classic routing in `AppContainer`
- AC: `KIRO_UI_MODE=classic bun run dev` shows placeholder
- AC: `bun run dev` shows modern mode unchanged

**W1-9** Week 1 smoke test
- AC: All of W1-1 through W1-8 verified end-to-end
- AC: `bun test` passes

### Week 2 — Raw Write Infrastructure

**W2-1** Add `rawWrite` to Twinki `Instance` *(skip if path A)*
- AC: `instance.rawWrite(['line'])` callable; calls `writeStaticLines` + `requestRender`
- AC: Twinki test suite passes

**W2-2** Implement `renderUserMessageToLines`
- AC: Returns `['> text']` in primary color; unit test

**W2-3** Implement `renderAgentMessageToLines`
- AC: Markdown → ANSI string[]; handles code fences, tables, bold, inline code; unit tests

**W2-4** Implement `renderSystemMessageToLines`
- AC: Dimmed text; success vs error color; unit test

**W2-5** Implement `renderTurnSummaryToLines`
- AC: Format `↳ 123 in · 456 out · $0.002`; unit test

**W2-6** Implement `findNewlyFinalizedMessages`
- AC: Uses `computeFlushSet`; returns only messages that crossed from dynamic to static since last call; unit tests

**W2-7** Wire store subscription in `ClassicLayout`
- AC: On finalized message → `renderToLines` → `rawWrite`
- AC: User message echoed with `> ` prefix

**W2-8** Wire turn summary to `rawWrite`
- AC: Summary line printed after each agent turn

**W2-9** Week 2 smoke test
- AC: Send message; response in scrollback; React tree ≤10 nodes (`getMetrics().yogaNodeCount`)

### Week 3 — Live Region and Streaming

**W3-1** Implement `paragraphFlush.ts` boundary detection
- AC: Detects `\n\n`, completed code fence, completed table; unit tests for all cases

**W3-2** Implement `useClassicFlush` hook
- AC: Tracks `liveContent`; calls `onFlush` on boundary; flushes remainder on turn end
- AC: Bounds live region to `MAX_LIVE_LINES`; unit tests

**W3-3** Implement `ClassicLiveRegion` component
- AC: Thinking → `<Spinner /> thinking…`; streaming → `<Text wrap="overflow">`; idle → nothing
- AC: Story in `tui-components` storybook for each state

**W3-4** Wire `ClassicLiveRegion` into `ClassicLayout`
- AC: `onFlush` callback calls `rawWrite`; live region updates on each token

**W3-5** Week 3 smoke test
- AC: Long response stays ≤10 lines in live region; full response in scrollback after turn

### Week 4 — Input and Keybindings

**W4-1** Implement `ClassicPromptLine` component
- AC: Renders `> ` + `<PromptInput />`; hidden while `isProcessing`
- AC: Story in storybook

**W4-2** Wire `ClassicPromptLine` into `ClassicLayout`
- AC: Prompt appears below live region after agent finishes

**W4-3** Verify Ctrl+A — move to line start
- AC: Cursor moves to position 0

**W4-4** Verify Ctrl+E — move to line end
- AC: Cursor moves to end of input

**W4-5** Verify Ctrl+K — kill to end of line
- AC: Text from cursor to end deleted; stored in kill ring

**W4-6** Verify Ctrl+U — kill to start of line
- AC: Text from start to cursor deleted

**W4-7** Verify Ctrl+W — kill word backward
- AC: Previous word deleted

**W4-8** Verify Alt+F / Alt+B — forward/backward word
- AC: Cursor moves by word boundary

**W4-9** Verify Ctrl+Y — yank from kill ring
- AC: Last killed text inserted at cursor

**W4-10** Verify up/down arrow history
- AC: Up cycles to previous input; down returns toward current

**W4-11** Wire Ctrl+C cancel / exit
- AC: First Ctrl+C cancels running turn; second exits process

**W4-12** Wire Ctrl+Z suspend
- AC: Process suspended; `fg` resumes correctly

**W4-13** Week 4 smoke test
- AC: Full Emacs editing session; history cycling; cancel; suspend/resume

### Week 5 — Tool Calls

**W5-1** Implement `ClassicToolCall` component (live)
- AC: Format `⚙ tool_name: desc … ⠋ running`; spinner cycles
- AC: Subagent format `⚙ [agent] tool_name: …`
- AC: Story in storybook: running, done, failed, subagent

**W5-2** Implement `renderToolCallToLine` (flush)
- AC: Format `⚙ tool_name: desc … done (0.3s)` or `… failed`; unit test

**W5-3** Implement shell tool live output
- AC: Last N lines of `liveOutput` shown below tool call line; bounded to `MAX_LIVE_LINES - 1`

**W5-4** Implement write/diff tool display
- AC: Shows `✎ path/to/file (+N/-M lines)` not full diff content

**W5-5** Wire tool call lifecycle into `ClassicLayout`
- AC: Running tool → live region; finished → `rawWrite`; multiple tools stack

**W5-6** Wire compaction summary
- AC: `Context compacted: N tokens freed.` printed via `rawWrite`

**W5-7** Week 5 smoke test
- AC: Tool call shows spinner; flushes with elapsed time; shell output streams

### Week 6 — Approvals and Trust Gate

**W6-1** Implement `ClassicApproval` component
- AC: Format `Allow bash: cmd?\n  [y] Yes, once  [n] No  [t] Trust all`
- AC: Single keypress `y`/`n`/`t` responds without Enter
- AC: Story in storybook: pending, answered-yes, answered-no

**W6-2** Implement `renderApprovalToLines`
- AC: Returns question + answer lines; unit test

**W6-3** Wire `ClassicApproval` into `ClassicLayout`
- AC: Replaces live region when `pendingApproval` is set
- AC: After response, flushes to scrollback via `rawWrite`

**W6-4** Implement inline trust-all-tools gate
- AC: Format `Trust all tools?\n  [y] Yes  [a] Always  [n] No`
- AC: `y` → `confirmTrustAllTools()`; `a` → save + confirm; `n` → exit

**W6-5** Week 6 smoke test
- AC: Approval flow end-to-end; trust gate on startup

### Week 7 — Slash Commands: Core Formatters

**W7-1** Implement `formatHelp`
- AC: Two-column table; command name left, description right; unit test

**W7-2** Implement `formatContext`
- AC: Table: file path | tokens | % of context; unit test

**W7-3** Implement `formatTools`
- AC: Table: name | source | status (allowed/requires-approval/denied); unit test

**W7-4** Implement `formatMcp`
- AC: Table: name | status | tool count; unit test

**W7-5** Implement `formatUsage`
- AC: Plan name, billing cycle, usage breakdown lines; unit test

**W7-6** Implement `formatHooks`
- AC: Table: trigger | command | matcher; unit test

**W7-7** Implement `formatStats`
- AC: Table: request_id | duration | ttfc | tokens | status; summary line; unit test

**W7-8** Implement `formatKeybindings`
- AC: Table: key | action; covers all Emacs bindings + Kiro-specific; unit test

**W7-9** Wire all formatters to `CommandContext` override in `ClassicLayout`
- AC: Each `setShow*Panel` call routes to `rawWrite(format*(data))` instead of panel state

**W7-10** Week 7 smoke test
- AC: Run `/help`, `/context`, `/tools`, `/mcp`, `/usage`, `/hooks`, `/stats`, `/keybindings` — all produce inline text

### Week 8 — Slash Commands: Session, Model, Agent

**W8-1** Wire `/clear`
- AC: Clears messages; resets scrollback; prints `Conversation cleared.`

**W8-2** Wire `/quit` and `/exit`
- AC: Exits process cleanly

**W8-3** Wire `/model` inline selection
- AC: Prints current model + numbered list; keypress selects; confirmation printed

**W8-4** Wire `/agent` inline selection
- AC: Same pattern as `/model`

**W8-5** Wire `/chat save`
- AC: Saves session; prints confirmation inline

**W8-6** Wire `/chat load`
- AC: Loads session; history replayed to scrollback

**W8-7** Wire `/chat list`
- AC: Prints session list inline

**W8-8** Wire `/compact`
- AC: Prints `Compacting…` while running; prints summary when done

**W8-9** Wire `/plan`
- AC: Switches to planner agent; prints `Switched to Plan agent.`

**W8-10** Week 8 smoke test
- AC: `/clear`, `/model`, `/agent`, `/chat save/load/list`, `/compact`, `/plan` all work

### Week 9 — Slash Commands: Editor, Paste, Autocomplete

**W9-1** Wire `/editor`
- AC: Opens `$EDITOR`; on close submits content as message

**W9-2** Wire `/paste`
- AC: Reads clipboard image; attaches to next message

**W9-3** Implement inline slash autocomplete
- AC: Typing `/con` shows `→ /context` below prompt; updates as user types
- AC: Uses `fuzzyScore.ts`; single line only

**W9-4** Implement inline `@` file mention autocomplete
- AC: Typing `@src` shows best matching file path; uses `file-search.ts`

**W9-5** Wire `/theme` inline selection
- AC: Numbered list of presets; preview line; confirmation

**W9-6** Wire `/settings` inline menu
- AC: Prints current settings; arrow key navigation; Enter to toggle/edit

**W9-7** Wire `/prompts`
- AC: Prints available prompts inline

**W9-8** Week 9 smoke test
- AC: `/editor`, `/paste`, autocomplete, `/theme`, `/settings`, `/prompts` all work

### Week 10 — Notifications and Session Lifecycle

**W10-1** Welcome line on startup
- AC: `Kiro vX.Y.Z — classic mode  /help for commands` printed once; suppressed when `!isTTY`

**W10-2** Changelog announcement
- AC: New version announcement printed inline after welcome line if available

**W10-3** Session history replay
- AC: On `--resume`, history events replayed to scrollback in correct order

**W10-4** MCP server failure notification
- AC: `⚠ MCP server 'name' failed: reason` printed once per failure on startup

**W10-5** OAuth pending notification
- AC: `⚠ OAuth required for 'name'. Visit: <url>  [c] Copy URL` printed; `c` copies

**W10-6** Rate limit / error messages
- AC: `agentError` printed inline with guidance text; cleared after printing

**W10-7** Wire `/knowledge`
- AC: Prints knowledge base entries inline via `formatKnowledge`

**W10-8** Wire `/todos`
- AC: Prints todo list inline via `formatTodos`

**W10-9** Week 10 smoke test
- AC: Welcome line; MCP failure warning; OAuth notification; error message; history replay

### Week 11 — Pipe Safety and Polish

**W11-1** Non-interactive mode verification
- AC: `--no-interactive` auto-submits, exits after turn, errors on approval

**W11-2** Suppress spinners when `!isTTY`
- AC: No spinner characters in piped output

**W11-3** Suppress colors when `NO_COLOR` set
- AC: Plain text output; chalk respects `NO_COLOR` automatically

**W11-4** Suppress welcome line and prompt when `!isTTY`
- AC: Only agent response lines in piped output

**W11-5** Verify `less` compatibility
- AC: `kiro chat --classic "hello" | less` works without corruption

**W11-6** Verify `tee` compatibility
- AC: `kiro chat --classic "hello" --no-interactive | tee out.txt` captures output

**W11-7** Long line soft-wrap
- AC: Lines > terminal width soft-wrap in scrollback; copy-paste preserves logical lines

**W11-8** Resize handling
- AC: Terminal resize reflows live region without corruption

**W11-9** Wire `/code`
- AC: Prints code intelligence status inline via `formatCode`

**W11-10** Wire `/logdump` and `/changelog`
- AC: Both work without panel overlays

**W11-11** Week 11 smoke test
- AC: `kiro chat --classic "hello" --no-interactive | cat` produces clean text with `NO_COLOR=1`

### Week 12 — E2E Tests and Hardening

**W12-1** E2E: `classic-mode-basic.test.ts`
- AC: Response in scrollback; no StatusBar chrome; prompt inline; welcome line once

**W12-2** E2E: `classic-mode-streaming.test.ts`
- AC: Live region ≤10 lines; full response in scrollback; paragraph flush mid-stream; code fence not split

**W12-3** E2E: `classic-mode-tools.test.ts`
- AC: Tool spinner; flush with elapsed time; shell live output; failed tool

**W12-4** E2E: `classic-mode-approvals.test.ts`
- AC: Inline prompt; `y` continues; `n` denies; trust gate

**W12-5** E2E: `classic-mode-slash-commands.test.ts`
- AC: `/help`, `/clear`, `/context`, `/tools`, `/mcp` all produce inline text

**W12-6** E2E: `classic-mode-input.test.ts`
- AC: Ctrl+A/E/K/U; history; slash autocomplete; Ctrl+C cancel

**W12-7** E2E: `classic-mode-pipe.test.ts`
- AC: `--no-interactive` exits after turn; piped output is clean text

**W12-8** Full regression: `bun test` across workspace
- AC: Zero failures in modern mode; zero failures in Twinki

**W12-9** Manual regression: modern mode unchanged
- AC: No visual or behavioral differences in `kiro chat`

**W12-10** Performance check
- AC: `getMetrics().yogaNodeCount` ≤15 during conversation; no memory growth over 10 turns

### Week 13 (Future) — tui-components: Shared Primitives

**W13-1** Move shared primitives to `packages/tui-components/src/shared/`
- AC: `spinner/`, `divider/`, `icon/`, `text/`, `chip/`, `alert/`, `table/`, `status/`, `card/`, `hint/` moved

**W13-2** Add re-export stubs in `packages/tui`
- AC: All existing imports in `packages/tui` still resolve

**W13-3** Add lint rule: no store imports in `tui-components`
- AC: ESLint errors on `zustand`, `../stores/`, `../kiro` imports in `tui-components`

**W13-4** Update storybook stories registry
- AC: Storybook shows Shared / Modern / Classic sections

**W13-5** Full test suite passes
- AC: Zero failures

### Week 14 (Future) — tui-components: Modern Components

**W14-1** Move modern presentational components
- AC: `status-bar/`, `message/`, `tools/`, `prompt-bar/`, `notification-bar/`, `radio/`, `menu/`, `brand/`, `welcome-screen/` moved

**W14-2** Add re-export stubs in `packages/tui`
- AC: All existing imports still resolve

**W14-3** Delegate `dev:storybook` script
- AC: `bun run dev:storybook` in `packages/tui` delegates to `packages/tui-components`

**W14-4** Full test suite passes
- AC: Zero failures

### Week 15 (Future) — chat-classic: Extract Render Functions

**W15-1** Create `packages/chat-classic` package
- AC: `package.json`, `tsconfig.json`, `src/index.ts` exist

**W15-2** Move `renderToLines.ts`, `ClassicCommandFormatter.ts`, `paragraphFlush.ts`
- AC: Files at `packages/chat-classic/src/`

**W15-3** Move unit tests
- AC: Tests at `packages/chat-classic/__tests__/`; all pass

**W15-4** Wire workspace dependency
- AC: `@kiro/chat-classic` in `packages/tui/package.json`; `bun install` succeeds

**W15-5** Update imports in `ClassicLayout` and hooks
- AC: No direct `../render/` imports remain in `packages/tui`

**W15-6** Add lint rule: no React/zustand imports in `chat-classic`
- AC: ESLint errors on `react`, `zustand`, `@kiro/tui` imports

**W15-7** Full test suite passes
- AC: Zero failures

---

## Project-Level Acceptance Criteria

The project is complete when ALL of the following are true:

- [ ] All 124 tasks marked `[x]` in `docs/design/classic-tui-mode-8w-tasks.md`
- [ ] `KIRO_UI_MODE=classic kiro chat` starts a working session with V1 feature parity
- [ ] `kiro chat` starts modern mode with zero visual or behavioral changes
- [ ] All 7 E2E test files pass: `bun run test:e2e` in `packages/tui`
- [ ] `bun test` passes across the entire workspace
- [ ] `kiro chat --classic "hello" --no-interactive | cat` produces clean plain text (`NO_COLOR=1`)
- [ ] Live React tree ≤15 Yoga nodes during a conversation (`getMetrics().yogaNodeCount`)
- [ ] `bun run storybook` from `packages/tui-components` shows Shared / Modern / Classic sections
- [ ] Every classic component has storybook stories for all visual states
- [ ] `packages/tui-components/src/classic/` has no `zustand` or store imports (lint rule enforced)
- [ ] `docs/design/classic-rendering-benchmark-results.json` exists; implementation matches `decision.chosen`
- [ ] `state.renderingPath` is set and matches the benchmark decision
