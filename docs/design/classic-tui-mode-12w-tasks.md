# Classic TUI Mode — 12-Week Task Canvas

Status: Planning  
Date: 2026-05-11  
Design: [classic-tui-mode.md](classic-tui-mode.md)  
LLD: [classic-tui-mode-lld.md](classic-tui-mode-lld.md)  
Raw Rendering: [classic-tui-mode-raw-rendering.md](classic-tui-mode-raw-rendering.md)

---

## Directory Structure

Code is split across two packages from the start:

`packages/tui-components` — presentational components only (no store, no hooks). Classic components live here from day one alongside the existing modern components. This package owns the unified Storybook.

`packages/tui` — layout roots and wiring (store → components). `ClassicLayout` lives here because it reads the Zustand store and calls `rawWrite`.

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

The rule: nothing in `tui-components/classic/` or `render/` may import from `zustand` or any store file. Violations are caught by a lint rule added in Week 13.

---

Status markers: `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked (add note below task)

Mark tasks as you go. Each week ends with a manual smoke test — do not skip it. The smoke test is the acceptance gate for the week. If it fails, fix before moving on.

---

## Feature Inventory (V1 parity checklist)

Before the weekly breakdown, here is the complete feature surface that classic mode must cover to be 1:1 with V1. Every item maps to at least one task below.

**Conversation flow**
- User message echoed to scrollback with `> ` prefix
- Agent response streamed to live region, flushed to scrollback on paragraph boundary or turn end
- Thinking/spinner shown while agent is working, cleared when first token arrives
- Turn usage summary (tokens in/out, cost) printed after each agent turn
- Compaction summary printed when context is compacted

**Tool calls**
- One-line inline display: `⚙ tool_name: description … running`
- Status updates in place while running (spinner)
- Flushed to scrollback as `⚙ tool_name: description … done (Xs)` or `… failed`
- Shell tool live output: streamed lines printed below the tool call line, bounded
- Subagent tool calls prefixed: `⚙ [agent-name] tool_name: …`
- Write/diff tool: shows file path and line count, not full diff

**Approvals**
- Inline prompt: `Allow <tool>: <description>?\n  [y] Yes, once  [n] No  [t] Trust all`
- Single keypress (`y`/`n`/`t`), no Enter required
- Trust-all-tools gate on startup (inline, same format)
- Approval result flushed to scrollback

**Input**
- Inline prompt `> ` after last output, not pinned
- Full Emacs keybindings (Ctrl+A/E/K/U/W, Alt+F/B, etc.) via existing `PromptInput`
- Command history (up/down arrows) via existing `PromptInput`
- Slash command trigger on `/` — inline single-line autocomplete suggestion
- `@` file mention trigger — inline single-line autocomplete suggestion
- Bracketed paste support (existing)
- Ctrl+C cancels running turn; second Ctrl+C exits
- Ctrl+Z suspends process

**Slash commands — output format**
- `/help` → two-column table of commands + descriptions
- `/clear` → clears scrollback and resets conversation
- `/context` → table of context files with sizes and token counts
- `/tools` → table of tool names, source, status
- `/mcp` → table of MCP servers with status and tool count
- `/model` → current model, selection menu inline
- `/agent` → current agent, selection menu inline
- `/usage` → billing summary table
- `/hooks` → table of hooks
- `/compact` → triggers compaction, prints summary
- `/chat save|load|list` → session management, inline output
- `/quit` / `/exit` → exits
- `/editor` → opens `$EDITOR`, submits on close
- `/paste` → pastes image from clipboard
- `/prompts` → lists available prompts
- `/knowledge` → knowledge base management
- `/todos` → todo list management
- `/theme` → inline theme selection
- `/settings` → inline settings menu
- `/stats` → request stats table
- `/keybindings` → keybindings table
- `/code` → code intelligence status
- `/tangent` → tangent mode toggle (if enabled)
- `/plan` → switches to planner agent
- Backend-registered commands (prompts, skills) → forwarded as messages

**Notifications and errors**
- MCP server failure notification printed inline on startup
- OAuth pending notification printed inline
- Rate limit / error messages printed inline
- Agent error printed inline with guidance text

**Session lifecycle**
- Welcome line on startup: `Kiro vX.Y.Z — classic mode`
- Changelog announcement printed inline (if new version)
- Session resume: history replayed to scrollback on startup
- Non-interactive mode (`--no-interactive`): output to stdout, exit after turn

**Pipe / non-TTY**
- When stdout is not a TTY: no spinners, no colors (respect `NO_COLOR`)
- Output usable with `less`, `tee`, pipes

---

## Week 1 — Foundation: Mode Resolution, Routing, and Package Skeleton

**Goal:** `KIRO_UI_MODE=classic kiro chat` starts without crashing and routes to a placeholder. `packages/tui-components` exists with a working Storybook that shows existing modern stories. All existing tests pass.

**Demo:** Run `KIRO_CLASSIC_UI_ENABLED=1 KIRO_UI_MODE=classic bun run dev` — shows "Classic mode — coming soon" placeholder. Run `bun run storybook` from `packages/tui-components` — shows existing modern component stories. Show benchmark results table and the chosen rendering path.

- [ ] **W1-0** Add feature flag gate — `CHAT_CLASSIC_UI_ENABLED` setting + `KIRO_CLASSIC_UI_ENABLED` env var. When false (default), `uiMode` is always `'modern'` and `ClassicLayout` is never rendered. This must be the first task so every subsequent commit is safe to merge to main.
- [ ] **W1-BENCH** Design and run rendering strategy benchmark — compare React/Twinki (A), rawWrite (B), Node.js stdio (C); write `docs/design/classic-rendering-benchmark-results.json`; record chosen path in state file
- [ ] **W1-1** Add `CHAT_UI_MODE: 'chat.ui.mode'` to `packages/tui/src/constants/settings.ts`
- [ ] **W1-2** Add `readStringSetting(key, default)` to `cli-settings.ts` + unit test
- [ ] **W1-3** Add `--classic` / `--modern` flags to `cli-args.ts` + unit test
- [ ] **W1-4** Add `resolveUiMode()` to `index.tsx`; classic forces `effectiveWrapDisabled=true`; store `uiMode` in Zustand initial state (read-only)
- [ ] **W1-5** Add `classicMode: boolean` to `ThemeProvider` and `ThemeContextValue`; expose via `useTheme()`; add test
- [ ] **W1-6** Create `packages/tui-components/` workspace package:
  - `package.json` with `name: "@kiro/tui-components"`, `private: true`, deps: `twinki`, `chalk`, `react`
  - `tsconfig.json` extending the root config
  - `src/index.ts` (empty for now)
  - `src/classic/` directory (empty)
  - Move `src/storybook/` from `packages/tui` into `packages/tui-components/src/storybook/` — update imports to use `@kiro/tui-components` paths; verify `bun run storybook` works from the new location
  - Add `"storybook": "bun run src/storybook/run-storybook.tsx"` script
  - Add `"@kiro/tui-components": "workspace:*"` to `packages/tui/package.json` dependencies
- [ ] **W1-7** Create `packages/tui/src/components/layout/classic/` with placeholder `ClassicLayout.tsx` that renders `<Text>Classic mode</Text>`; export from `index.ts`
- [ ] **W1-8** Add classic routing branch to `AppContainer.tsx`: `mode==='inline' && uiMode==='classic'` → `<ClassicLayout />`
- [ ] **W1-9** Smoke test: `KIRO_UI_MODE=classic bun run dev` starts, shows placeholder; `bun run storybook` from `packages/tui-components` shows existing modern stories; `bun test` passes

---

## Week 2 — Raw Write Infrastructure and Message Rendering

**Goal:** Finalized messages write directly to terminal scrollback via `rawWrite`. The React tree for classic mode stays at ≤10 nodes.

**Demo:** Send a message in classic mode. The agent response appears in terminal scrollback (scrollable, copy-pasteable). A turn summary line prints after the response. Show `getMetrics().yogaNodeCount ≤ 10` in the console.

- [ ] **W2-1** Add `rawWrite(lines: string[]): void` to `Instance` interface and object in `packages/twinki/packages/twinki/src/reconciler/render.ts` (calls `tui.writeStaticLines(lines); tui.requestRender()`)
- [ ] **W2-2** Implement `renderToLines.ts` — pure functions, no React:
  - `renderUserMessageToLines(text, theme): string[]` → `> text` in primary color
  - `renderAgentMessageToLines(content, theme): string[]` → markdown → ANSI via `markdown.ts` + chalk
  - `renderSystemMessageToLines(content, success, theme): string[]` → dimmed system text
  - `renderTurnSummaryToLines(summary, theme): string[]` → token/cost line
  - Unit tests for each
- [ ] **W2-3** Implement `findNewlyFinalizedMessages(state, prev)` using `computeFlushSet` — pure function, unit tests
- [ ] **W2-4** Wire store subscription in `ClassicLayout`: on finalized messages → `renderToLines` → `rawWrite`
- [ ] **W2-5** Wire turn summary to `rawWrite` — fires when `lastTurnSummary` changes in store; format `↳ 123 in · 456 out · $0.002`
- [ ] **W2-6** Wire compaction summary to `rawWrite` — `Context compacted: N tokens freed.`
- [ ] **W2-7** Implement `renderToolCallToLine(tool, elapsed, theme): string` — `⚙ tool_name: desc … done (0.3s)` or `… failed`; unit test
- [ ] **W2-8** Implement `renderApprovalToLines(question, answer, theme): string[]`; unit test
- [ ] **W2-9** Week 2 smoke test: send message; response in scrollback; turn summary printed; React tree ≤10 nodes (`getMetrics().yogaNodeCount`)

---

## Week 3 — Live Region: Thinking, Streaming, Flush

**Goal:** The live region shows the thinking spinner and streaming text. Content never exceeds the viewport. Paragraph-based flush works.

**Demo:** Send a prompt that produces a long response (>20 lines). Show the thinking spinner appearing, then text streaming in the live region (≤10 lines visible at a time), then the full response in scrollback after the turn ends. Scroll up to verify all content is there.

- [ ] **W3-1** Implement `useClassicFlush.ts`:
  - Tracks `liveContent: string` for the current streaming message
  - Detects paragraph boundaries: `\n\n`, completed code fence (` ``` ` on own line), completed table
  - On boundary: calls `onFlush(content)` callback, clears flushed portion from `liveContent`
  - On turn end: flushes remaining `liveContent` unconditionally
  - Bounds live region to `MAX_LIVE_LINES = Math.min(terminalHeight - 2, 10)`: flushes oldest complete line when exceeded
  - Unit tests for all boundary cases
- [ ] **W3-2** Implement `ClassicLiveRegion.tsx`:
  - Thinking state: `<Spinner /> thinking…` (reuse existing `Spinner`)
  - Streaming state: `<Text wrap="overflow">{liveContent}</Text>` (plain text; markdown rendered at flush time)
  - Idle: renders nothing (zero lines)
  - Reads `isProcessing`, `isThinking` from store; uses `useClassicFlush`
- [ ] **W3-3** Wire `ClassicLiveRegion` into `ClassicLayout`; connect `onFlush` callback to `rawWrite`
- [ ] **W3-4** Add `ClassicLiveRegion` stories to `tui-components` storybook: thinking, streaming, idle states
- [ ] **W3-5** Week 3 smoke test: long response stays ≤10 lines in live region; full response in scrollback after turn; thinking spinner appears and disappears

---

## Week 4 — Input: Prompt, History, Emacs Keybindings

**Goal:** The user can type, edit, and submit messages. Emacs keybindings and history work identically to V1.

**Demo:** Type a message using Ctrl+A/E/K/U to edit, submit it, see `> message` echoed in scrollback. Press up-arrow to recall previous input. Press Ctrl+C during a running turn to cancel it. Press Ctrl+Z to suspend, then `fg` to resume.

- [ ] **W4-1** Implement `ClassicPromptLine.tsx`:
  - Renders `> ` prefix + `<PromptInput />` inline (not wrapped in `PromptBar`)
  - Hidden while `isProcessing` is true
  - On submit: calls `sendMessage`, user text is echoed to scrollback via the store subscription in `ClassicLayout`
- [ ] **W4-2** Wire `ClassicPromptLine` into `ClassicLayout` below `ClassicLiveRegion`
- [ ] **W4-3** Verify Ctrl+A (line start), Ctrl+E (line end) — cursor moves correctly
- [ ] **W4-4** Verify Ctrl+K (kill to end), Ctrl+U (kill to start) — text deleted, stored in kill ring
- [ ] **W4-5** Verify Ctrl+W (kill word back) — previous word deleted
- [ ] **W4-6** Verify Alt+F / Alt+B (forward/backward word) — cursor moves by word boundary
- [ ] **W4-7** Verify Ctrl+Y (yank) — last killed text inserted at cursor
- [ ] **W4-8** Verify up/down arrow history — up cycles to previous input; down returns toward current
- [ ] **W4-9** Wire Ctrl+C cancel / exit — first cancels running turn; second exits process
- [ ] **W4-10** Wire Ctrl+Z suspend — process suspended; `fg` resumes correctly
- [ ] **W4-11** Verify bracketed paste — multi-line paste inserts correctly without triggering commands
- [ ] **W4-12** Verify Ctrl+L clear screen — redraws live region without losing scrollback
- [ ] **W4-13** Week 4 smoke test: full Emacs editing session; history cycling; cancel; suspend/resume; paste

---

## Week 5 — Tool Calls: Inline Display and Live Output

**Goal:** Tool calls display as one-line inline entries. Status updates in place while running. Shell live output streams below the tool line. All flush to scrollback on completion.

**Demo:** Ask the agent to read a file — see `⚙ read_file src/main.rs … ⠋ running` with a spinning indicator, then `… done (0.2s)` flushed to scrollback. Ask it to run a shell command — see live output streaming below the tool line. Ask it to write a file — see `✎ path/to/file (+5/-2 lines)` summary.

- [ ] **W5-1** Implement `ClassicToolCall.tsx` (live region component):
  - Format: `⚙ tool_name: brief_description … ⠋ running`
  - Subagent format: `⚙ [agent-name] tool_name: brief_description … ⠋ running`
  - Spinner character cycles while `isFinished === false`
  - Receives `ToolCallInfo` from store; renders in live region while running
- [ ] **W5-2** Add `renderToolCallToLine(tool, elapsed, theme): string` to `renderToLines.ts`:
  - Format: `⚙ tool_name: description … done (0.3s)` or `… failed`
  - Called when tool finishes; result written via `rawWrite`
- [ ] **W5-3** Shell tool live output: when `liveOutput` array is non-empty on a running tool, render the last N lines below the tool call line in the live region (bounded to `MAX_LIVE_LINES - 1`). Lines are plain text, no prefix.
- [ ] **W5-4** Write/diff tool display: show `✎ path/to/file (+N/-M lines)` instead of full diff content. Extract from tool `content` field.
- [ ] **W5-5** Wire tool call lifecycle into `ClassicLayout` store subscription:
  - New tool call (not finished) → render `ClassicToolCall` in live region
  - Tool call finishes → `rawWrite(renderToolCallToLine(...))`, remove from live region
  - Multiple concurrent tool calls: stack them in the live region (each on its own line)
- [ ] **W5-6** Add `ClassicToolCall` stories to storybook: running, done, failed, subagent, shell-with-output
- [ ] **W5-7** Week 5 smoke test: tool call shows spinner; flushes with elapsed time; shell output streams; write/diff shows path+line count

---

## Week 6 — Approvals and Trust Gate

**Goal:** Approval requests render inline. Single keypress responds. Trust-all-tools gate works inline on startup.

**Demo:** Trigger a tool that requires approval — see the inline prompt `Allow bash: …? [y/n/t]`. Press `y` — conversation continues immediately (no Enter needed). Restart without `--trust-all-tools` — see the trust gate prompt inline on startup.

- [ ] **W6-1** Implement `ClassicApproval.tsx`:
  - Renders in live region when `pendingApproval` is set in store
  - Format:
    ```
    Allow bash: rm -rf /tmp/x?
      [y] Yes, once   [n] No   [t] Trust all tools
    ```
  - Uses `useKeypress` to capture `y`, `n`, `t` (no Enter required)
  - On `y`: calls store `approveOnce` action
  - On `n`: calls store `denyApproval` action
  - On `t`: calls store `trustAllTools` action
  - After response: `rawWrite(renderApprovalToLines(question, answer, theme))` to flush to scrollback
- [ ] **W6-2** Implement inline trust-all-tools gate in `ClassicLayout`:
  - When `trustAllToolsRequested && !trustAllToolsConfirmed`: render inline prompt instead of `ClassicLiveRegion`
  - Format: `Trust all tools for this session?\n  [y] Yes  [a] Always  [n] No`
  - `y` → `confirmTrustAllTools()`; `a` → `saveTrustGateAccepted(kiro)` then `confirmTrustAllTools()`; `n` → exit
  - Reuses `trust-gate-state.ts` unchanged
- [ ] **W6-3** Add `renderApprovalToLines(question, answer, theme): string[]` to `renderToLines.ts`
- [ ] **W6-4** Wire `ClassicApproval` into `ClassicLayout`: when `pendingApproval` is set, replace `ClassicLiveRegion` with `ClassicApproval`
- [ ] **W6-5** Smoke test: trigger an approval, press `y`, verify conversation continues; trigger trust gate on startup, verify inline prompt appears

---

## Week 7 — Slash Commands: Core Output Formatters

**Goal:** The most-used slash commands produce correct inline text output. No panel overlays open in classic mode.

**Demo:** Run `/help` — see a two-column table of commands inline. Run `/tools` — see tool names with status. Run `/context` — see context files with token counts. Run `/mcp` — see server status. All output is in scrollback, no panels open.

The V2 command dispatcher already handles execution and returns structured data. Classic mode intercepts the `setShow*Panel` calls and routes them to `rawWrite` instead. The mechanism: `ClassicLayout` provides a `CommandContext` override where all `setShow*Panel` functions call `rawWrite(formatPanel(...))` instead of updating panel state.

- [ ] **W7-1** Implement `ClassicCommandFormatter.ts` — pure functions:
  - `formatHelp(commands): string[]` → two-column table using `table-layout.ts`
  - `formatContext(data): string[]` → table: file path | tokens | % of context
  - `formatTools(tools): string[]` → table: name | source | status
  - `formatMcp(servers): string[]` → table: name | status | tools
  - `formatUsage(data): string[]` → billing summary lines
  - `formatHooks(hooks): string[]` → table: trigger | command | matcher
  - `formatStats(stats, summary): string[]` → request stats table
  - `formatKeybindings(): string[]` → keybindings table
  - Unit tests for each formatter
- [ ] **W7-2** Wire `/help` in classic mode: intercept `setShowHelpPanel` → `rawWrite(formatHelp(commands))`
- [ ] **W7-3** Wire `/context` → `rawWrite(formatContext(data))`
- [ ] **W7-4** Wire `/tools` → `rawWrite(formatTools(tools))`
- [ ] **W7-5** Wire `/mcp` → `rawWrite(formatMcp(servers))`
- [ ] **W7-6** Wire `/usage` → `rawWrite(formatUsage(data))`
- [ ] **W7-7** Wire `/hooks` → `rawWrite(formatHooks(hooks))`
- [ ] **W7-8** Wire `/stats` → `rawWrite(formatStats(stats, summary))`
- [ ] **W7-9** Wire `/keybindings` → `rawWrite(formatKeybindings())`
- [ ] **W7-10** Smoke test: run each command, verify inline text output with correct columns and data

---

## Week 8 — Slash Commands: Session, Model, Agent, Clear

**Goal:** Session management, model/agent switching, and `/clear` work correctly in classic mode.

**Demo:** Run `/clear` — scrollback resets. Run `/model` — see numbered list, press a number to switch, see confirmation. Run `/chat save` — see "Session saved" inline. Run `/compact` — see "Compacting…" then summary. Run `/plan` — see "Switched to Plan agent."

- [ ] **W8-1** `/clear`: calls `clearMessages()` on store; calls `instance.clear()` to reset scrollback; prints `Conversation cleared.` via `rawWrite`
- [ ] **W8-2** `/quit` / `/exit`: calls `onExit()` and `process.exit(0)`. Already handled by `app-keypress-dispatch.ts` — verify works in classic mode.
- [ ] **W8-3** `/model`: inline selection — print current model, then numbered list of available models; user types number + Enter to select. Use `useKeypress` for single-char selection when list ≤9 items, otherwise require Enter. Print confirmation via `rawWrite`.
- [ ] **W8-4** `/agent`: same pattern as `/model` — inline numbered list, selection via keypress.
- [ ] **W8-5** `/chat save|load|list`: backend executes; result printed via `rawWrite(addSystemMessage(...))`. The `addSystemMessage` store action already adds a system message — the store subscription in `ClassicLayout` picks it up and calls `rawWrite(renderSystemMessageToLines(...))`.
- [ ] **W8-6** `/compact`: triggers compaction; prints `Compacting conversation…` while running; prints summary when done via `rawWrite`.
- [ ] **W8-7** `/plan`: switches to planner agent; prints `Switched to Plan agent.` via `rawWrite`.
- [ ] **W8-8** Backend-registered commands (prompts, skills): forwarded as messages via `sendMessage` — no special handling needed, already works.
- [ ] **W8-9** Wire `/tangent` — toggle tangent mode if `chat.enableTangentMode` is true; print status inline
- [ ] **W8-10** Week 8 smoke test: `/clear`, `/model`, `/agent`, `/chat save/load/list`, `/compact`, `/plan`, `/tangent` all work

---

## Week 9 — Slash Commands: Editor, Paste, Inline Autocomplete

**Goal:** Editor-based input, image paste, and inline slash/@ autocomplete work in classic mode.

**Demo:** Type `/con` — see `→ /context` suggestion below the prompt. Type `@src` — see file path suggestion. Run `/editor` — $EDITOR opens, type a message, close — message is submitted. Run `/theme` — see numbered list of presets, select one, colors change.

- [ ] **W9-1** `/editor`: calls `openEditorSync()` (existing utility); on close, submits content as message. Already implemented in `effects.ts` — verify works in classic mode (no panel needed).
- [ ] **W9-2** `/paste`: calls clipboard read; if image found, attaches to next message. Already implemented in `effects.ts` — verify works.
- [ ] **W9-3** Inline slash command autocomplete: when user types `/`, show a single line below the prompt with the best matching command name (e.g., `  → /context`). Updates as user types. Implemented as a small component in the live region, below `ClassicPromptLine`. Uses existing `fuzzyScore.ts` for matching.
- [ ] **W9-4** Inline `@` file mention autocomplete: when user types `@`, show a single line with the best matching file path. Uses existing `file-search.ts`. Same pattern as slash autocomplete.
- [ ] **W9-5** `/theme`: inline theme selection — print numbered list of presets; user selects; print preview line showing colors; confirm. Uses existing theme system.
- [ ] **W9-6** `/settings`: inline settings menu — print current settings; user navigates with arrow keys; Enter to toggle/edit. Reuse `settings-subcommands.ts` logic, render output via `rawWrite`.
- [ ] **W9-7** `/prompts` command: print available prompts inline. Add `formatPrompts(prompts): string[]`.
- [ ] **W9-8** Week 9 smoke test: `/editor`, `/paste`, autocomplete for `/` and `@`, `/theme`, `/settings`, `/prompts` all work

---

## Week 10 — Notifications, Errors, Session Lifecycle

**Goal:** All inline notifications, error messages, and session lifecycle events (welcome, history replay, changelog) work correctly.

**Demo:** Start classic mode — see welcome line `Kiro vX.Y.Z — classic mode`. Start with a broken MCP server — see `⚠ MCP server 'name' failed` inline. Resume a saved session — see history replayed to scrollback. After a turn, see `↳ 123 in · 456 out · $0.002` summary line.

- [ ] **W10-1** Welcome line on startup: `rawWrite([chalk.dim('Kiro v' + version + ' — classic mode  /help for commands')])`. Printed once before the first prompt. Uses `version.ts`.
- [ ] **W10-2** Changelog announcement: if a new version announcement exists (from `getActiveAnnouncement()`), print it as plain text via `rawWrite` after the welcome line. No branded `WelcomeScreen` component.
- [ ] **W10-3** Session history replay: on resume, history events are replayed via the store's `createStreamEventHandler`. The store subscription in `ClassicLayout` picks up the replayed messages and calls `rawWrite` for each. Verify the replay produces correct scrollback output.
- [ ] **W10-4** MCP server failure notification: when `pendingMcpFailures` is non-empty in the store, print inline: `⚠ MCP server 'name' failed to start: reason`. Printed once per failure, not repeated.
- [ ] **W10-5** OAuth pending notification: when `pendingOAuthServers` is non-empty, print inline: `⚠ OAuth required for 'name'. Visit: <url>  [c] Copy URL`. Keypress `c` copies URL to clipboard.
- [ ] **W10-6** Rate limit / error messages: when `agentError` is set in the store, print inline with guidance text via `rawWrite`. Clear after printing (same as modern mode behavior).
- [ ] **W10-7** Turn usage summary: after each agent turn, print token/cost summary line. Add `renderTurnSummaryToLines(summary, theme): string[]` to `renderToLines.ts`. Wire into store subscription — fires when `lastTurnSummary` changes.
- [ ] **W10-8** Compaction summary: when a compaction event arrives, print `Context compacted: N tokens freed.` via `rawWrite`.
- [ ] **W10-9** Smoke test: start with a failed MCP server, verify inline warning; resume a session, verify history appears in scrollback; verify turn summary prints after each response

---

## Week 11 — Non-Interactive Mode, Pipe Safety, Polish

**Goal:** Classic mode works correctly when piped or used non-interactively. Output is clean for scripting.

**Demo:** Run `NO_COLOR=1 kiro chat --classic "explain hello world" --no-interactive | cat` — see clean plain text output with no ANSI codes, no spinners, no prompt. Run `kiro chat --classic "hello" | less` — output is readable in less. Resize the terminal during streaming — no corruption.

- [ ] **W11-1** Non-interactive mode (`--no-interactive`): classic mode already routes through the same `cliArgs.noInteractive` path in `index.tsx`. Verify: auto-submit input, exit after turn, error on approval request. No new code needed — verify existing path works with classic mode.
- [ ] **W11-2** Pipe-safe output: when `!process.stdout.isTTY`:
  - Suppress all spinners (thinking indicator, tool call spinner)
  - Suppress colors when `NO_COLOR` env var is set (chalk already respects this)
  - Suppress the welcome line
  - Suppress the input prompt
  - Output is pure text: agent response lines only
  - Add `isTTY` guard in `ClassicLiveRegion` and `ClassicPromptLine`
- [ ] **W11-3** `less` / `tee` compatibility: verify `kiro chat --classic "hello" | cat` produces clean output; verify `kiro chat --classic "hello" | less` works; verify `kiro chat --classic "hello" --no-interactive | tee out.txt` captures output
- [ ] **W11-4** Long line handling: very long agent response lines (>terminal width) soft-wrap correctly in scrollback. Verify `wrap="overflow"` is set on all `<Text>` in the live region. Verify `wideLines: true` is passed to `render()` (already done via `effectiveWrapDisabled`).
- [ ] **W11-5** Resize handling: when terminal is resized, the live region reflows correctly. Verify `onResize` callback in `ClassicLayout` clears and redraws the live region.
- [ ] **W11-6** `/code` command: print code intelligence status inline via `rawWrite(formatCode(data))`. Add `formatCode(data): string[]` to `ClassicCommandFormatter.ts`.
- [ ] **W11-7** `/knowledge` command: print knowledge base entries inline. Add `formatKnowledge(entries): string[]`.
- [ ] **W11-8** `/todos` command: print todo list inline. Add `formatTodos(todos): string[]`.
- [ ] **W11-9** `/prompts` command: print available prompts inline. Add `formatPrompts(prompts): string[]`.
- [ ] **W11-10** `/logdump` and `/changelog`: these open files or print text — verify they work without panel overlays.
- [ ] **W11-11** Smoke test: `kiro chat --classic "hello" --no-interactive | cat` produces clean text; resize terminal during streaming, verify no corruption; run all remaining slash commands

---

## Week 12 — E2E Tests, Regression, and Hardening

**Goal:** All E2E tests pass. Zero regressions in modern mode. Feature is ready for review.

**Demo:** Run `bun run test:e2e` — all 7 new classic-mode E2E tests pass green. Run `bun test` — full workspace passes. Run `kiro chat` (modern mode) side-by-side with classic mode — modern is visually identical to before. Show `getMetrics().yogaNodeCount ≤ 15` during a 10-turn conversation with tool calls.

- [ ] **W12-1** E2E test: `classic-mode-basic.test.ts`
  - Start with `KIRO_UI_MODE=classic`
  - Send message, verify response in scrollback
  - Verify no StatusBar chrome (no colored left-border characters in output)
  - Verify prompt appears inline after response
  - Verify welcome line appears once
- [ ] **W12-2** E2E test: `classic-mode-streaming.test.ts`
  - Long response (>10 lines): verify live region stays ≤10 lines during streaming
  - Verify full response is in scrollback after turn ends
  - Verify paragraph-based flush: blank line in response triggers flush mid-stream
  - Verify code fence is not split across flush boundary
- [ ] **W12-3** E2E test: `classic-mode-tools.test.ts`
  - Trigger tool call, verify one-line format with spinner
  - Verify tool flushes to scrollback with elapsed time on completion
  - Trigger shell tool with output, verify live output streams below tool line
  - Trigger failed tool, verify `… failed` in scrollback
- [ ] **W12-4** E2E test: `classic-mode-approvals.test.ts`
  - Trigger approval, verify inline prompt format
  - Press `y`, verify conversation continues
  - Press `n`, verify denial recorded
  - Trigger trust gate on startup, verify inline prompt
- [ ] **W12-5** E2E test: `classic-mode-slash-commands.test.ts`
  - `/help` → inline table with command names
  - `/clear` → scrollback cleared, conversation reset
  - `/context` → inline table with context files
  - `/tools` → inline table with tool names
  - `/mcp` → inline table with server status
- [ ] **W12-6** E2E test: `classic-mode-input.test.ts`
  - Emacs keybindings: Ctrl+A, Ctrl+E, Ctrl+K, Ctrl+U
  - History: up/down arrows cycle previous inputs
  - Slash autocomplete: `/con` shows `/context` suggestion
  - Ctrl+C cancels running turn
- [ ] **W12-7** E2E test: `classic-mode-pipe.test.ts`
  - `--no-interactive` mode: output to stdout, exit after turn
  - Piped output: no spinners, no prompt, clean text
- [ ] **W12-8** Regression: run full existing test suite (`bun test`, `bun run e2e`)
  - Zero failures in modern mode tests
  - Zero failures in Twinki tests
- [ ] **W12-9** Manual regression: run modern mode (`kiro chat`) and verify it is completely unchanged — no visual differences, no behavioral differences
- [ ] **W12-10** Performance check: in classic mode, verify `getMetrics().yogaNodeCount` stays ≤15 during a full conversation with tool calls; verify no memory growth over a 10-turn session

---

## Completion Criteria

The feature is complete when all of the following are true:

1. All tasks above are marked `[x]`.
2. `KIRO_UI_MODE=classic kiro chat` starts a working classic-mode session with all V1 features.
3. `kiro chat` (no flag) starts modern mode, visually and behaviorally unchanged.
4. All 7 new E2E test files pass.
5. All existing tests pass (zero regressions).
6. `kiro chat --classic "hello" --no-interactive | cat` produces clean plain text (with `NO_COLOR=1`).
7. The live React tree never exceeds 15 Yoga nodes during a conversation.

---

## Notes and Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-05-11 | Use `rawWrite` (direct `writeStaticLines`) instead of `<Static>` array for committed content | Keeps React tree at ≤10 nodes; reconciler only runs when live region changes; matches V1's direct stdout model |
| 2026-05-11 | Markdown rendered at flush time by `renderToLines.ts`, not in live region | Live region uses plain `<Text wrap="overflow">` for streaming; markdown rendering happens once when content is finalized |
| 2026-05-11 | Inline single-line autocomplete for `/` and `@` instead of `CommandMenu` overlay | Classic mode has no overlay panels; single-line suggestion is sufficient and scrollback-safe |
| 2026-05-11 | `/model` and `/agent` use inline numbered list + keypress selection | Matches V1 pattern; no panel overlay needed |

---

## Week 13 (Future) — `packages/tui-components`: Migrate Shared Primitives + Add Lint Guard

**Trigger:** Classic mode has shipped in one production release.

**Goal:** Move the store-free shared primitives (Spinner, Divider, Icon, etc.) into `packages/tui-components`. Re-export from `packages/tui` for backward compatibility. Add a lint rule that prevents store imports in `tui-components`. Storybook now runs from `tui-components` and shows Shared + Modern + Classic sections.

- [ ] **W13-1** Move `shared/` primitives from `packages/tui/src/components/ui/` into `packages/tui-components/src/shared/`: `spinner/`, `divider/`, `icon/`, `text/`, `chip/`, `alert/`, `table/`, `status/`, `card/`, `hint/`
- [ ] **W13-2** Add re-export stubs in `packages/tui` for each moved component: `export { Spinner } from '@kiro/tui-components'`
- [ ] **W13-3** Add lint rule (ESLint `no-restricted-imports`) to `packages/tui-components` that errors on any import from `zustand`, `../stores/`, or `../kiro`
- [ ] **W13-4** Update `stories.ts` in `tui-components` to import shared stories from their new locations; verify Storybook shows all three sections (Shared / Modern / Classic)
- [ ] **W13-5** Run full test suite — zero failures expected

---

## Week 14 (Future) — `packages/tui-components`: Migrate Modern Components

**Goal:** Move modern presentational components into `packages/tui-components/src/modern/`. Re-export from `packages/tui`. The storybook in `tui-components` is now the canonical one; the old `packages/tui` storybook script becomes an alias.

- [ ] **W14-1** Move `modern/` components: `status-bar/`, `message/`, `tools/`, `prompt-bar/`, `notification-bar/`, `radio/`, `menu/`, `brand/`, `welcome-screen/`
- [ ] **W14-2** Add re-export stubs in `packages/tui` for each moved component
- [ ] **W14-3** Update `packages/tui/package.json` `dev:storybook` script to delegate to `packages/tui-components`: `"dev:storybook": "bun --cwd ../tui-components run storybook"`
- [ ] **W14-4** Run full test suite — zero failures expected

---

## Week 15 (Future) — `packages/chat-classic`: Extract Pure Render Functions

**Trigger:** `render/` functions in `packages/tui/layout/classic/render/` are stable for one release cycle.

**Goal:** Extract `render/` into `packages/chat-classic`. Zero behavior change. Enables other consumers (future `kiro pipe`, test harnesses) to use the formatting logic without the full TUI.

- [ ] **W15-1** Create `packages/chat-classic/` with `package.json` (`name: "@kiro/chat-classic"`, `private: true`), `tsconfig.json`, `src/index.ts`
- [ ] **W15-2** Move `classic/render/renderToLines.ts`, `ClassicCommandFormatter.ts`, `paragraphFlush.ts` into `packages/chat-classic/src/`
- [ ] **W15-3** Move `classic/__tests__/` into `packages/chat-classic/__tests__/`
- [ ] **W15-4** Add `"@kiro/chat-classic": "workspace:*"` to `packages/tui/package.json` dependencies
- [ ] **W15-5** Update imports in `classic/hooks/` and `classic/ClassicLayout.tsx` to import from `@kiro/chat-classic`
- [ ] **W15-6** Add lint rule to `packages/chat-classic` preventing imports from `react`, `zustand`, or `@kiro/tui`
- [ ] **W15-7** Run full test suite — zero failures expected
