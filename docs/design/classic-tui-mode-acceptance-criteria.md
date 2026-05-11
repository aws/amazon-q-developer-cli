# Classic TUI Mode — Feature-Level Acceptance Criteria

Status: Active
Date: 2026-05-11
Companion to: [classic-tui-mode-12w-tasks.md](classic-tui-mode-12w-tasks.md)

This document defines **when each feature deliverable is DONE** — not at the task level, but at the feature level. Use this to validate completeness, detect alignment issues, and determine when UX feedback is needed.

---

## How to Use This Document

1. **After completing a week's tasks**: check the relevant deliverable(s) below
2. **For each criterion**: mark ✅ (passes), ❌ (fails), or ⚠️ (needs UX feedback)
3. **If any criterion is ⚠️**: stop and get alignment before proceeding
4. **A deliverable is DONE** only when all its criteria are ✅

---

## Deliverable 1: Conversation Flow

**Owner:** Weeks 2–3, finalized Week 10
**Validates:** Message rendering, streaming, flush, live region, turn summaries

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 1.1 | User message echoed with `> ` prefix in primary color | Send message; check scrollback | No |
| 1.2 | Agent response streams token-by-token in live region | Watch streaming; tokens appear incrementally | No |
| 1.3 | Live region never exceeds `min(termHeight-2, 10)` lines | Long response; count visible lines during stream | No |
| 1.4 | Paragraph flush on `\n\n`, completed code fence, completed table | Unit test `paragraphFlush.test.ts` passes | No |
| 1.5 | Code fence never split mid-flush (opening ``` and closing ``` in same flush or deferred) | Unit test + manual: ask for code, watch flush | **Yes** — if code appears garbled |
| 1.6 | Full response in scrollback after turn ends | Scroll up after turn; complete response visible | No |
| 1.7 | Thinking spinner shown before first token, cleared on first token | Ask question; observe spinner → text transition | **Yes** — spinner duration feel |
| 1.8 | Markdown rendered correctly: bold, code, tables, lists, headings | Ask for formatted response; compare to V1 | **Yes** — rendering quality |
| 1.9 | Turn summary `↳ N in · M out · $X.XX` printed after each turn | Complete a turn; check summary line | No |
| 1.10 | Compaction summary `Context compacted: N tokens freed.` | Trigger compaction; check output | No |
| 1.11 | React tree ≤15 Yoga nodes during conversation | `getMetrics().yogaNodeCount` assertion | No |
| 1.12 | No memory growth over 10 turns | Profile 10-turn session | No |

**DONE gate:** All 12 criteria ✅. Criteria 1.5, 1.7, 1.8 require UX sign-off.

---

## Deliverable 2: Tool Calls

**Owner:** Week 5
**Validates:** Tool display, spinner, live output, flush

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 2.1 | Running tool: `⚙ tool_name: desc … ⠋ running` with cycling spinner | Trigger tool; observe | **Yes** — format/truncation |
| 2.2 | Completed tool: `⚙ tool_name: desc … done (Xs)` flushed to scrollback | Wait for tool finish; check scrollback | No |
| 2.3 | Failed tool: `⚙ tool_name: desc … failed` in red/error color | Trigger failing tool | No |
| 2.4 | Subagent tool: `⚙ [agent-name] tool_name: desc …` | Trigger subagent; observe prefix | **Yes** — readability |
| 2.5 | Shell live output: last N lines below tool line, bounded | Run shell command with output | **Yes** — how many lines feel right |
| 2.6 | Write/diff: `✎ path/to/file (+N/-M lines)` — no full diff | Trigger file write; check display | **Yes** — enough info? |
| 2.7 | Multiple concurrent tools stack (one line each) | Trigger parallel tools | No |
| 2.8 | Tool description truncated sensibly for long descriptions | Trigger tool with long desc | **Yes** — truncation point |

**DONE gate:** All 8 criteria ✅. Criteria 2.1, 2.4, 2.5, 2.6, 2.8 require UX sign-off on format.

---

## Deliverable 3: Permissions & Approvals

**Owner:** Week 6
**Validates:** Approval prompt, keypress handling, trust flow, trust gate

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 3.1 | Approval format: `Allow <tool>: <desc>?\n  [y] Yes, once  [n] No  [t] Trust all` | Trigger approval; check format | **Yes** — wording |
| 3.2 | Single keypress `y`/`n`/`t` responds (no Enter required) | Press key; observe immediate response | No |
| 3.3 | `y` → tool executes, conversation continues | Press y; verify execution | No |
| 3.4 | `n` → denial sent to LLM, LLM asks follow-up | Press n; verify follow-up | No |
| 3.5 | `t` → granular trust scope menu appears (inline numbered list) | Press t; verify scope options | **Yes** — scope menu UX |
| 3.6 | Trust scope: "This file", "This directory", "Entire tool" options | Check menu items | **Yes** — option labels |
| 3.7 | Trust-all gate on startup: `[y] Yes  [a] Always  [n] No` | Start with untrusted tools | **Yes** — gate wording |
| 3.8 | `a` on trust gate persists preference (doesn't ask again) | Press a; restart; verify no gate | No |
| 3.9 | Approval result flushed to scrollback after response | Check scrollback after approval | No |
| 3.10 | `/tools` shows per-tool trust status | Run `/tools`; check status column | No |

**DONE gate:** All 10 criteria ✅. Criteria 3.1, 3.5, 3.6, 3.7 require UX sign-off on wording/format.

---

## Deliverable 4: Input & Keybindings

**Owner:** Week 4
**Validates:** Prompt, editing, history, cancel, suspend

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 4.1 | Inline `> ` prompt appears after agent finishes | Complete turn; check prompt | No |
| 4.2 | Prompt hidden while agent is processing | Start turn; verify no prompt visible | No |
| 4.3 | Ctrl+A → cursor to position 0 | Type text; Ctrl+A; verify | No |
| 4.4 | Ctrl+E → cursor to end | Type text; Ctrl+E; verify | No |
| 4.5 | Ctrl+K → kill to end of line | Type; move cursor; Ctrl+K; verify | No |
| 4.6 | Ctrl+U → kill to start of line | Type; move cursor; Ctrl+U; verify | No |
| 4.7 | Ctrl+W → kill word backward | Type words; Ctrl+W; verify | No |
| 4.8 | Alt+F / Alt+B → word navigation | Type words; Alt+F/B; verify cursor | No |
| 4.9 | Ctrl+Y → yank killed text | Kill text; Ctrl+Y; verify insertion | No |
| 4.10 | Up/Down → history cycling | Submit messages; up arrow; verify | No |
| 4.11 | Ctrl+R → reverse history search | Ctrl+R; type; verify match | No |
| 4.12 | Ctrl+C (1st) → cancel running turn | Start turn; Ctrl+C; verify cancel | No |
| 4.13 | Ctrl+C (2nd) → exit process | Ctrl+C twice; verify exit | No |
| 4.14 | Ctrl+Z → suspend; `fg` resumes | Ctrl+Z; fg; verify state preserved | No |
| 4.15 | Bracketed paste handled correctly | Paste multi-line; verify | No |

**DONE gate:** All 15 criteria ✅. No UX feedback needed (matches V1 exactly).

---

## Deliverable 5: Slash Commands

**Owner:** Weeks 7–9, 11
**Validates:** All commands produce inline output, no panels

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 5.1 | `/help` → two-column table (name + description) | Run; check format | **Yes** — column widths |
| 5.2 | `/context` → table (path \| tokens \| %) | Run; check format | **Yes** — column format |
| 5.3 | `/tools` → table (name \| source \| status) | Run; check format | No |
| 5.4 | `/mcp` → table (name \| status \| tools) | Run; check format | No |
| 5.5 | `/usage` → billing summary lines | Run; check format | No |
| 5.6 | `/hooks` → table (trigger \| command \| matcher) | Run; check format | No |
| 5.7 | `/stats` → request stats table | Run; check format | No |
| 5.8 | `/keybindings` → key/action table | Run; check format | No |
| 5.9 | `/clear` → clears scrollback + prints confirmation | Run; verify clean state | No |
| 5.10 | `/model` → inline numbered list; keypress selects | Run; select; verify | **Yes** — selection UX |
| 5.11 | `/agent` → inline numbered list; keypress selects | Run; select; verify | No (same as /model) |
| 5.12 | `/chat save/load/list` → inline output | Run each; verify | No |
| 5.13 | `/compact` → progress + summary | Run; verify | No |
| 5.14 | `/editor` → opens editor; submits on close | Run; edit; verify submission | No |
| 5.15 | `/paste` → attaches image | Run with image in clipboard | No |
| 5.16 | `/` autocomplete → single-line suggestion below prompt | Type `/con`; verify suggestion | **Yes** — suggestion format |
| 5.17 | `@` autocomplete → file path suggestion | Type `@src`; verify suggestion | **Yes** — suggestion format |
| 5.18 | `/theme` → inline selection with preview | Run; select; verify | **Yes** — preview format |
| 5.19 | `/settings` → inline menu | Run; navigate; verify | **Yes** — menu UX |
| 5.20 | `/prompts` → lists prompts inline | Run; verify | No |
| 5.21 | `/knowledge` → KB entries inline | Run; verify | No |
| 5.22 | `/todos` → todo list inline | Run; verify | No |
| 5.23 | `/code` → code status inline | Run; verify | No |
| 5.24 | No panel overlays ever open in classic mode | Run all commands; verify no overlays | No |
| 5.25 | Dynamic prompt commands forwarded as messages | Register prompt; run `/<name>`; verify | No |

**DONE gate:** All 25 criteria ✅. Criteria 5.1, 5.2, 5.10, 5.16, 5.17, 5.18, 5.19 require UX sign-off.

---

## Deliverable 6: Session Lifecycle & Notifications

**Owner:** Week 10
**Validates:** Welcome, changelog, resume, errors, warnings

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 6.1 | Welcome: `Kiro vX.Y.Z — classic mode  /help for commands` | Start fresh; check first line | **Yes** — wording |
| 6.2 | Welcome suppressed when `!isTTY` | Pipe output; verify no welcome | No |
| 6.3 | Changelog announcement after welcome (if new version) | Simulate new version; check | No |
| 6.4 | History replay on `--resume` produces correct scrollback | Resume session; scroll up; verify | No |
| 6.5 | MCP failure: `⚠ MCP server 'name' failed: reason` (once) | Configure failing server; start | **Yes** — warning format |
| 6.6 | OAuth pending: `⚠ OAuth required … [c] Copy URL` | Configure OAuth server; start | **Yes** — notification format |
| 6.7 | Rate limit / error messages inline with guidance | Trigger rate limit; check | No |
| 6.8 | Context usage warning at 60% | Fill context to 60%; check warning | **Yes** — threshold/format |

**DONE gate:** All 8 criteria ✅. Criteria 6.1, 6.5, 6.6, 6.8 require UX sign-off.

---

## Deliverable 7: Non-Interactive & Pipe Safety

**Owner:** Week 11
**Validates:** Scripting, piping, clean output

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 7.1 | `--no-interactive` auto-submits, exits after turn | Run with flag; verify | No |
| 7.2 | `--no-interactive` errors on approval request | Trigger approval in non-interactive; verify error | No |
| 7.3 | No spinners when `!isTTY` | Pipe output; grep for spinner chars | No |
| 7.4 | No colors when `NO_COLOR=1` | Set env; pipe; verify plain text | No |
| 7.5 | No welcome/prompt when piped | Pipe; verify only response lines | No |
| 7.6 | `\| less` works without corruption | Pipe to less; verify | No |
| 7.7 | `\| tee out.txt` captures output | Pipe to tee; check file | No |
| 7.8 | Long lines soft-wrap correctly | Wide output; verify wrap | No |
| 7.9 | Terminal resize reflows live region | Resize during stream; verify | No |

**DONE gate:** All 9 criteria ✅. No UX feedback needed (behavior is deterministic).

---

## Deliverable 8: Modern Mode Unchanged (Regression)

**Owner:** Week 12
**Validates:** Zero impact on existing users

| # | Criterion | How to Verify | UX Feedback Needed? |
|---|-----------|---------------|---------------------|
| 8.1 | `kiro chat` (no flag) starts modern mode | Start without flags; verify | No |
| 8.2 | Zero visual differences in modern mode | Side-by-side comparison | No |
| 8.3 | Zero behavioral differences in modern mode | Run common workflows | No |
| 8.4 | All existing tests pass (`bun test`) | Run full suite | No |
| 8.5 | All E2E tests pass (`bun run test:e2e`) | Run E2E suite | No |
| 8.6 | Twinki test suite passes | Run Twinki tests | No |

**DONE gate:** All 6 criteria ✅. No UX feedback needed.

---

## Alignment Detection Rules

Use these rules to determine when work needs to STOP for alignment:

### 🛑 STOP — Needs Alignment (block until resolved)

1. **Design deviation**: Implementation requires changing a component's props/API from what the LLD specifies
2. **New dependency**: Need to add a package not in the design docs
3. **Store schema change**: Need to modify Zustand store shape beyond what's planned
4. **ACP protocol change**: Need new message types or fields in the Rust backend
5. **Breaking existing tests**: Any existing test fails due to classic mode changes
6. **Performance regression**: Modern mode gets slower due to classic mode code

### ⚠️ PAUSE — Needs UX Feedback (can continue other tasks)

1. **Text formatting**: Any user-visible string format (approval wording, table columns, etc.)
2. **Timing/animation**: Spinner speed, flush frequency, debounce values
3. **Truncation**: How to shorten long tool descriptions, file paths, error messages
4. **Color choices**: Which colors for which states (beyond what theme provides)
5. **Information density**: How much to show in tables, summaries, notifications

### ✅ PROCEED — No Alignment Needed

1. **Internal implementation**: How code is structured (as long as APIs match LLD)
2. **Test implementation**: How tests are written
3. **Performance optimization**: Making things faster without changing behavior
4. **Bug fixes**: Fixing issues that clearly violate acceptance criteria

---

## UX Feedback Checklist

Items that need UX sign-off before the feature is considered complete:

### Format & Wording (collect during Weeks 5-10)
- [ ] Tool call one-line format: `⚙ name: desc … status`
- [ ] Tool description truncation length
- [ ] Shell live output line count (how many lines to show)
- [ ] Write/diff summary format: `✎ path (+N/-M)`
- [ ] Approval prompt wording
- [ ] Trust scope menu labels
- [ ] Trust-all gate wording
- [ ] Welcome line text
- [ ] MCP failure warning format
- [ ] OAuth notification format
- [ ] Context usage warning threshold and format
- [ ] `/help` table column widths
- [ ] `/context` table format
- [ ] `/model` and `/agent` selection UX
- [ ] Slash autocomplete suggestion format
- [ ] `@` file autocomplete suggestion format
- [ ] `/theme` preview format
- [ ] `/settings` inline menu navigation

### Behavior (collect during Weeks 3-6)
- [ ] Paragraph flush timing (feels natural vs choppy)
- [ ] Spinner-to-text transition (smooth vs jarring)
- [ ] Live region max lines (10 enough? too many?)
- [ ] Approval keypress responsiveness

---

## Validation Workflow

After completing each week:

1. Run the weekly smoke test (last task of each week)
2. Check all criteria for the relevant deliverable(s)
3. Mark each criterion ✅/❌/⚠️
4. If any ❌: fix before moving to next week
5. If any ⚠️: log in state file notes; continue if non-blocking
6. Update `docs/design/classic-tui-state.json` with progress
