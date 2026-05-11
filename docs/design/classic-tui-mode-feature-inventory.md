# Classic TUI Mode — Exhaustive Feature Inventory

Status: Active
Date: 2026-05-11
Companion to: [classic-tui-mode-12w-tasks.md](classic-tui-mode-12w-tasks.md)

This document maps every feature of the app to its classic mode implementation, organized by component category. Use it to verify coverage and identify gaps.

---

## 1. SLASH COMMANDS

### 1.1 Core Output Commands (Week 7)

| Command | V2 Modern Behavior | Classic Mode Behavior | Task |
|---------|-------------------|----------------------|------|
| `/help` | Opens HelpPanel overlay | `rawWrite(formatHelp(commands))` — two-column table | W7-1, W7-2 |
| `/context [show\|add\|remove\|clear]` | Opens ContextPanel overlay | `rawWrite(formatContext(data))` — path/tokens/% table | W7-3 |
| `/tools [trust-all\|trust\|untrust\|reset]` | Opens ToolsPanel overlay | `rawWrite(formatTools(tools))` — name/source/status table | W7-4 |
| `/mcp [list\|add\|remove]` | Opens McpPanel overlay | `rawWrite(formatMcp(servers))` — name/status/tools table | W7-5 |
| `/usage` | Opens UsagePanel overlay | `rawWrite(formatUsage(data))` — billing summary lines | W7-6 |
| `/hooks` | Opens HooksPanel overlay | `rawWrite(formatHooks(hooks))` — trigger/command/matcher table | W7-7 |
| `/stats [N\|save]` | Opens StatsPanel overlay | `rawWrite(formatStats(stats))` — request stats table | W7-8 |
| `/keybindings` | Opens KeybindingsPanel | `rawWrite(formatKeybindings())` — key/action table | W7-9 |

### 1.2 Session & Agent Commands (Week 8)

| Command | V2 Modern Behavior | Classic Mode Behavior | Task |
|---------|-------------------|----------------------|------|
| `/clear` | Clears messages in store | Clears messages + scrollback; prints `Conversation cleared.` | W8-1 |
| `/quit` / `/exit` | `process.exit(0)` | Same | W8-2 |
| `/model [name]` | Opens selection menu overlay | Inline numbered list; keypress selects; confirmation printed | W8-3 |
| `/agent [name\|create\|edit\|swap]` | Opens selection menu overlay | Inline numbered list; keypress selects; confirmation printed | W8-4 |
| `/chat save [path]` | Backend executes; alert shown | Prints confirmation inline | W8-5 |
| `/chat load [path]` | Backend executes; history loaded | History replayed to scrollback | W8-6 |
| `/chat list` | Opens session picker overlay | Prints session list inline | W8-7 |
| `/chat new [prompt]` | Creates new session | Same as `/clear` + optional prompt | W8-1 |
| `/compact [target]` | Backend executes; alert shown | Prints `Compacting…` then summary | W8-8 |
| `/plan [prompt]` | Swaps to planner agent | Prints `Switched to Plan agent.` | W8-9 |
| `/guide [question]` | Swaps to guide agent | Same pattern as `/plan` | W8-4 (generic) |

### 1.3 Editor & Autocomplete Commands (Week 9)

| Command | V2 Modern Behavior | Classic Mode Behavior | Task |
|---------|-------------------|----------------------|------|
| `/editor [text]` | Opens `$EDITOR` | Same — opens editor, submits on close | W9-1 |
| `/reply` | Opens `$EDITOR` with last response | Same — opens editor pre-filled | W9-1 (same path) |
| `/paste` | Reads clipboard image | Same — attaches to next message | W9-2 |
| `/theme` | Opens theme selection menu | Inline numbered list; preview; confirmation | W9-5 |
| `/settings [theme\|keybindings]` | Opens settings menu/panel | Inline menu; arrow nav; Enter to toggle | W9-6 |
| `/prompts [name]` | Opens prompts selection menu | Prints available prompts inline | W9-7 |

### 1.4 Inline Autocomplete (Week 9)

| Trigger | V2 Modern Behavior | Classic Mode Behavior | Task |
|---------|-------------------|----------------------|------|
| `/` at start of input | CommandMenu dropdown overlay | Single-line suggestion below prompt: `→ /context` | W9-3 |
| `@` inline | File picker overlay (searchFilesAbortable) | Single-line file path suggestion below prompt | W9-4 |
| Tab on suggestion | Accepts suggestion | Same — accepts suggestion | W9-3, W9-4 |
| Esc on suggestion | Dismisses | Same — dismisses | W9-3, W9-4 |

### 1.5 Additional Commands (Weeks 10-11)

| Command | V2 Modern Behavior | Classic Mode Behavior | Task |
|---------|-------------------|----------------------|------|
| `/knowledge [show\|add\|remove\|update\|clear\|cancel]` | Opens KnowledgePanel | `rawWrite(formatKnowledge(entries))` | W11-7 |
| `/todos [list\|resume\|clear-finished]` | Activity tray | `rawWrite(formatTodos(todos))` | W11-8 |
| `/code [status\|init\|logs\|overview]` | Opens CodePanel | `rawWrite(formatCode(data))` | W11-6 |
| `/logdump` | Creates zip; shows alert | Creates zip; prints path inline | W11-10 |
| `/changelog` | Opens ChangelogPanel | Prints release notes inline | W11-10 |
| `/copy` | Copies to clipboard; shows alert | Same — copies; prints confirmation | N/A (works) |
| `/transcript` | Opens `$PAGER` | Same — opens pager | N/A (works) |
| `/session-id` | Shows alert with ID | Prints ID inline | N/A (works) |
| `/feedback` | Opens URL | Same — opens URL | N/A (works) |
| `/spawn <task>` | Creates sub-session | **Out of scope** (V2 crew mode only) | — |
| `/switch [id]` | Switches session view | **Out of scope** (V2 crew mode only) | — |
| `/tui` | Opens TUI changelog panel | **Out of scope** (V2-specific) | — |

### 1.6 Dynamic Commands (Runtime)

| Source | V2 Modern Behavior | Classic Mode Behavior | Task |
|--------|-------------------|----------------------|------|
| MCP prompt commands (`/<name>`) | Forwarded as message to agent | Same — forwarded as message | W8-8 (backend-registered) |
| Skill commands | Forwarded as message | Same | W8-8 |

---

## 2. TOOLS

### 2.1 Tool Display (Week 5)

| Tool State | Display Format | Location | Task |
|------------|---------------|----------|------|
| Running | `⚙ tool_name: desc … ⠋ running` | Live region | W5-1 |
| Running (subagent) | `⚙ [agent] tool_name: desc … ⠋ running` | Live region | W5-1 |
| Completed | `⚙ tool_name: desc … done (0.3s)` | Scrollback (rawWrite) | W5-2 |
| Failed | `⚙ tool_name: desc … failed` | Scrollback (rawWrite) | W5-2 |
| Shell with output | Tool line + last N output lines below | Live region | W5-3 |
| Write/diff | `✎ path/to/file (+N/-M lines)` | Scrollback (rawWrite) | W5-4 |
| Multiple concurrent | Stacked (one line each) | Live region | W5-5 |

### 2.2 Tool Inventory (All Available Tools)

| Tool | Category | Approval Behavior | Classic Display Notes |
|------|----------|-------------------|---------------------|
| `fs_read` | File I/O | Allow within CWD; Ask outside | No special display |
| `fs_write` | File I/O | Ask (shows diff context) | `✎ path (+N/-M)` |
| `execute_bash` | Shell | Ask (shows command) | Live output streaming |
| `grep` | Search | Allow (read-only) | No special display |
| `glob` | Search | Allow (read-only) | No special display |
| `code` | Intelligence | Allow read; Ask write | No special display |
| `web_fetch` | Network | Ask | No special display |
| `web_search` | Network | Ask | No special display |
| `use_aws` | Cloud | Ask (shows service:op) | No special display |
| `subagent` | Orchestration | Ask (shows agent name) | `[agent]` prefix |
| `introspect` | Context | Always allow | No special display |
| `knowledge` | Search | Ask unless allowed | No special display |
| `task`/`todo_list` | Planning | Always allow | No special display |
| `tool_search` | Discovery | Always allow | No special display |
| MCP tools | External | Ask unless server allowed | Shows server name |

### 2.3 Tool Settings (Agent Config)

| Setting | Effect on Classic Mode |
|---------|----------------------|
| `shell.allowedCommands` | Auto-allow matching commands (no approval prompt) |
| `shell.deniedCommands` | Auto-deny (tool fails with reason) |
| `shell.autoAllowReadonly` | Read-only commands auto-allowed |
| `fs_read.allowedPaths` | Paths auto-allowed without prompt |
| `fs_read.deniedPaths` | Paths auto-denied |
| `fs_write.allowedPaths` | Write paths auto-allowed |
| `fs_write.deniedPaths` | Write paths auto-denied |
| `use_aws.allowedServices` | Services auto-allowed |
| `use_aws.deniedServices` | Services auto-denied |
| `subagent.trustedAgents` | Agents auto-allowed |
| `subagent.availableAgents` | Agents available for spawning |

---

## 3. PERMISSIONS & APPROVALS

### 3.1 Approval Flow (Week 6)

| Step | What Happens | Classic Mode Display |
|------|-------------|---------------------|
| Tool requires approval | Permission eval returns `Ask` | Show `ClassicApproval` in live region |
| Prompt displayed | Question + options | `Allow <tool>: <desc>?\n  [y] Yes, once  [n] No  [t] Trust all` |
| User presses `y` | AllowOnce | Tool executes; approval flushed to scrollback |
| User presses `n` | RejectOnce | Denial sent to LLM; approval flushed |
| User presses `t` | Trust scope menu | Inline numbered list of scope options |
| Scope: "This file" | AllowAlwaysToolArgs | Path added to allowed_read/write_paths |
| Scope: "This directory" | AllowAlwaysToolArgs | Directory added to allowed paths |
| Scope: "Entire tool" | AllowAlwaysTool | Tool added to trusted_tools for session |

### 3.2 Trust-All Gate (Week 6)

| Step | What Happens | Classic Mode Display |
|------|-------------|---------------------|
| Session starts with untrusted tools | Gate triggered | `Trust all tools for this session?\n  [y] Yes  [a] Always  [n] No` |
| User presses `y` | Trust for session | `trust_all_tools = true`; gate dismissed |
| User presses `a` | Trust permanently | Preference saved; gate never shown again |
| User presses `n` | Decline | Process exits |

### 3.3 Permission Categories

| Category | Tools | Behavior |
|----------|-------|----------|
| Always Allow | introspect, task, tool_search, summary, grep*, glob* | Never shows approval prompt |
| Conditional Allow | fs_read (CWD), execute_bash (readonly+flag), use_aws (readonly+flag) | Auto-allowed under specific conditions |
| Requires Approval | fs_write, execute_bash, use_aws, web_*, code (write), subagent, MCP | Shows ClassicApproval |
| Auto-Denied | Commands matching denied patterns, paths matching denied paths | Tool fails with reason; no prompt |
| Session-Trusted | Tools in `runtime_permissions.trusted_tools` | Auto-allowed after user trusts |
| Session-Denied | Tools in `runtime_permissions.denied_tools` | Auto-denied after user denies |

---

## 4. MODES

### 4.1 UI Modes

| Mode | How Activated | Classic Mode Behavior | Task |
|------|--------------|----------------------|------|
| Classic | `--classic` flag / `KIRO_UI_MODE=classic` / setting | Full classic layout | W1-1 to W1-8 |
| Modern (default) | No flag / `--modern` / default | Unchanged — not affected | W1-8 |
| Non-interactive | `--no-interactive` / stdin not TTY | Auto-submit; exit after turn; no spinners | W11-1 to W11-4 |
| Pipe | stdout not TTY | No colors, no spinners, no welcome, no prompt | W11-2 to W11-6 |

### 4.2 Agent Modes

| Mode | How Activated | Classic Mode Behavior | Task |
|------|--------------|----------------------|------|
| Default agent | Startup | Normal conversation | — |
| Plan agent | `/plan` or `Shift+Tab` | Prints `Switched to Plan agent.`; conversation continues | W8-9 |
| Guide agent | `/guide` | Same pattern as plan | W8-4 |
| Custom agent | `/agent swap <name>` | Prints confirmation; conversation continues | W8-4 |

### 4.3 Session States

| State | Classic Mode Display |
|-------|---------------------|
| Idle (waiting for input) | `> ` prompt visible; live region empty |
| Processing (agent thinking) | Prompt hidden; `⠋ thinking…` spinner in live region |
| Streaming (tokens arriving) | Prompt hidden; streaming text in live region |
| Tool running | Prompt hidden; tool call line(s) in live region |
| Approval pending | Prompt hidden; approval prompt in live region |
| Compacting | Prompt hidden; `Compacting…` in live region |
| Trust gate | Prompt hidden; trust gate prompt in live region |

### 4.4 Conversation Flow States

| Event | Classic Mode Action |
|-------|-------------------|
| User submits message | Echo `> message` to scrollback via rawWrite |
| Agent starts thinking | Show spinner in live region |
| First token arrives | Replace spinner with streaming text |
| Paragraph boundary hit | Flush completed paragraph to scrollback; keep remainder in live region |
| Code fence completes | Flush entire fence to scrollback |
| Turn ends | Flush remaining live content to scrollback; show turn summary; show prompt |
| Tool call starts | Show tool line in live region with spinner |
| Tool call ends | Flush tool result line to scrollback |
| Compaction triggered | Print summary to scrollback |
| Error occurs | Print error inline to scrollback |

---

## 5. NOTIFICATIONS & LIFECYCLE

### 5.1 Startup Sequence (Week 10)

| Order | Event | Classic Mode Display | Condition |
|-------|-------|---------------------|-----------|
| 1 | Welcome line | `Kiro vX.Y.Z — classic mode  /help for commands` | `isTTY` |
| 2 | Changelog | New version announcement text | New version available |
| 3 | MCP failures | `⚠ MCP server 'name' failed: reason` (one per failure) | Any MCP server failed |
| 4 | OAuth pending | `⚠ OAuth required for 'name'. Visit: <url>  [c] Copy URL` | OAuth needed |
| 5 | Trust gate | `Trust all tools? [y] Yes  [a] Always  [n] No` | Untrusted tools present |
| 6 | History replay | Previous messages rendered to scrollback | `--resume` flag |
| 7 | Prompt | `> ` appears | Always (after above complete) |

### 5.2 Runtime Notifications

| Event | Classic Mode Display | Task |
|-------|---------------------|------|
| Rate limit hit | Error message inline with retry guidance | W10-6 |
| Agent error | Error message inline | W10-6 |
| Context at 60% | `⚠ Context usage: 60% — consider /compact` | Gap (add to W10) |
| Turn complete | `↳ N in · M out · $X.XX` | W10-7 |
| Compaction done | `Context compacted: N tokens freed.` | W10-8 |

### 5.3 Exit Conditions

| Trigger | Behavior |
|---------|----------|
| `/quit` or `/exit` | Clean exit |
| Ctrl+C × 2 | Clean exit |
| `n` on trust gate | Exit |
| `--no-interactive` turn complete | Exit |
| Ctrl+D on empty prompt | Exit |

---

## 6. KEYBOARD SHORTCUTS

### 6.1 Editing (from PromptInput — Week 4)

| Key | Action | Source |
|-----|--------|--------|
| Ctrl+A | Move to line start | input-editing.ts |
| Ctrl+E | Move to line end | input-editing.ts |
| Ctrl+K | Kill to end of line | input-editing.ts |
| Ctrl+U | Kill to start of line | input-editing.ts |
| Ctrl+W | Kill word backward | input-editing.ts |
| Alt+F | Forward word | input-editing.ts |
| Alt+B | Backward word | input-editing.ts |
| Ctrl+Y | Yank (paste killed text) | input-editing.ts |
| Ctrl+T | Transpose characters | input-editing.ts |
| Alt+D | Kill word forward | input-editing.ts |
| Ctrl+R | Reverse history search | input-editing.ts |

### 6.2 Navigation & Control (Week 4)

| Key | Action | Source |
|-----|--------|--------|
| Up arrow | Previous history entry | PromptInput |
| Down arrow | Next history entry | PromptInput |
| Enter | Submit message | PromptInput |
| Shift+Enter | Insert newline | PromptInput |
| Ctrl+C (1st) | Cancel running turn | ClassicLayout |
| Ctrl+C (2nd) | Exit process | ClassicLayout |
| Ctrl+Z | Suspend process | ClassicLayout |
| Ctrl+D | Exit (on empty input) | ClassicLayout |
| Tab | Accept autocomplete suggestion | ClassicInlineAutocomplete |
| Esc | Dismiss autocomplete | ClassicInlineAutocomplete |

### 6.3 Approval Keys (Week 6)

| Key | Action | Context |
|-----|--------|---------|
| y | Allow once | ClassicApproval |
| n | Deny | ClassicApproval |
| t | Trust (show scope menu) | ClassicApproval |
| 1-9 | Select scope option | Trust scope menu |
| a | Always trust (trust gate) | Trust gate |
| c | Copy URL | OAuth notification |

---

## 7. STORYBOOK STORIES REQUIRED

### 7.1 Classic Components (in `packages/tui-components`)

| Component | Stories | Week |
|-----------|---------|------|
| ClassicLiveRegion | thinking, streaming, idle | 3 |
| ClassicPromptLine | active, hidden (processing) | 4 |
| ClassicToolCall | running, done, failed, subagent, with-live-output | 5 |
| ClassicApproval | pending, answered-yes, answered-no, trust-scope-menu | 6 |
| ClassicInlineAutocomplete | slash-suggestion, file-suggestion, no-match | 9 |

### 7.2 Render Function Unit Tests (in `packages/tui/layout/classic/__tests__/`)

| File | Covers | Week |
|------|--------|------|
| renderToLines.test.ts | User/agent/system/summary/tool/approval rendering | 2, 5, 6 |
| ClassicCommandFormatter.test.ts | All 8+ formatters | 7, 11 |
| paragraphFlush.test.ts | Boundary detection: `\n\n`, code fence, table | 3 |

---

## 8. E2E TEST COVERAGE MAP

| Test File | Features Covered | Week |
|-----------|-----------------|------|
| classic-mode-basic.test.ts | Welcome, message echo, response in scrollback, prompt inline, no chrome | 12 |
| classic-mode-streaming.test.ts | Live region bounds, paragraph flush, code fence integrity | 12 |
| classic-mode-tools.test.ts | Tool spinner, elapsed time, shell output, failed tool | 12 |
| classic-mode-approvals.test.ts | Approval prompt, y/n/t responses, trust gate | 12 |
| classic-mode-slash-commands.test.ts | /help, /clear, /context, /tools, /mcp inline output | 12 |
| classic-mode-input.test.ts | Emacs keys, history, autocomplete, Ctrl+C cancel | 12 |
| classic-mode-pipe.test.ts | Non-interactive, piped output, no spinners/colors | 12 |

---

## 9. IDENTIFIED GAPS & RECOMMENDATIONS

| # | Gap | Impact | Recommendation |
|---|-----|--------|----------------|
| 1 | Granular trust scope selection not detailed in W6-1 | Users can't choose file vs dir vs tool trust | Add sub-steps: inline numbered scope menu after `t` |
| 2 | Context usage warning at 60% not in any task | Users don't know they're running low | Add to W10: print `⚠ Context usage: 60%` |
| 3 | `/chat new` not explicitly covered | Users expect it | Verify `/clear` covers this or add task |
| 4 | `/reply` not explicitly in W9 | Editor pre-filled with last response | Verify same code path as `/editor` |
| 5 | Ctrl+R reverse search not verified in W4 | Power users expect it | Add verification to W4-3 |
| 6 | Ctrl+D exit on empty input not in W4 | Common terminal behavior | Add to W4-11 |
| 7 | Shift+Tab for plan mode toggle | V2 has this | Decide: support in classic or skip |
| 8 | `/tangent` (V1 only) | Not in V2 | Skip — not in V2 parity target |
| 9 | `/checkpoint` (V1 only) | Not in V2 | Skip — not in V2 parity target |
