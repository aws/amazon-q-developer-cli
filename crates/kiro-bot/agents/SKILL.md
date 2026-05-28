---
name: kiro-help-workflow
description: Per-turn procedure the kiro-help Slack bot follows for every kiro-related question. Defines the order of tool calls (retrieve first, classify, search issues for bugs, cite, answer concisely) so quality and shape are consistent across DMs and channels.
---

# kiro-help workflow

Run this on EVERY user message that touches kiro, kiro-cli, kiro-bot, a kiro command/flag, an error message, a slash command, a configuration key, an MCP server, or any feature/bug report. Off-topic chitchat is the only legitimate exception.

## Steps

1. **Classify** the user's intent in one word — `question`, `bug-report`, `feature-request`, `setup`, `error`, `meta` (about the bot itself), or `off-topic`. Pick exactly one. If `off-topic`, skip the rest and politely redirect. If `meta`, answer from the prompt.

2. **Locate** with `search_kiro_knowledge`. The point of retrieval is to find which files and which feature names are relevant — not to draft the answer from docs.
   - Pass a focused query (a 4–8 word reframing of the user's prompt).
   - Top N = 5.
   - For `bug-report` or `error`, ALSO call `search_github_issues` in parallel with the same focused query.
   - If retrieval returns 0 useful chunks, fall back to navigating the source tree from the "Source-tree priorities" section below, plus any concrete anchors the user gave you (symbol, error string, flag name).

3. **Read the source.** For any `question`, `setup`, `error`, or `bug-report` intent, you MUST open the relevant code with `read` before drafting. Docs lag the code; the source is ground truth.
   - Drive `read` from concrete anchors: a path retrieval returned, the symbol the user named, the exact error string they pasted, the slash command/flag in question.
   - Prefer the V2 + TUI paths in "Source-tree priorities" below. The V1 binary (`crates/chat-cli/`) is still maintained — read it when the user explicitly asks about classic OR when triaging a `bug-report` to check for V1/V2 parity (see "Bug-investigation workflow").
   - **Pick the right language tree.** Backend/agent/CLI questions live in `crates/` (Rust). TUI/rendering/input questions live in `packages/` (TypeScript). The skill's "Key files" table maps common topics to specific paths — start there if the user named a feature.
   - Stop reading once you have enough to answer. A focused 1-3 file dive is the goal; if you find yourself opening more than ~5 files, the question is too broad — ask a clarifying follow-up instead.
   - For `feature-request`: skip the source dive and rely on `search_github_issues` + `search_kiro_knowledge`.

4. **Use `introspect` only for canonical names/schemas.** If the user asks for the exact name of a setting, the schema of a tool, or the full list of slash commands — `introspect` is the fastest path. For *what a flag does, defaults to, or how it interacts with other flags*, do not trust introspect — read the source.

5. **Draft the answer** in this shape:
   - One-line direct answer (no preamble).
   - One ```fenced``` code block if the answer involves a command or a code excerpt.
   - Optional 1-sentence context.
   - Sources line: `Sources: \`crates/chat-cli/src/cli/chat/mod.rs:330-345\`, \`docs/foo.md\`, \`github_issue:#42\`` — list every file:line range you read and every chunk/issue you cited. Source paths come first.

6. **Self-check before sending.**
   - If the question was about behavior/flags/errors and your draft has no source citation (a `crates/...` or `packages/...` path), you skipped step 3 — go read the code.
   - If the question is about TUI behavior and your only source is in `crates/` (Rust), you read the wrong tree — TUI behavior lives in `packages/tui/` and `packages/twinki/`. Re-read.
   - If your draft cites only V1 (`crates/chat-cli/src/cli/chat/`) for current behavior AND the user didn't ask about classic AND this isn't a parity check, you answered from the wrong surface — re-read from V2.
   - If your draft cites a `legacy/` or `v1_export/` path, that's almost certainly wrong — those are migration shims, not current behavior.
   - If your draft has a citation but the source path didn't actually appear in retrieval or your `read` output this turn, you fabricated — fix it.

## Source-tree priorities

The bot's CWD is a fresh checkout of `kiro-team/kiro-cli` (cloned to `/var/lib/kiro-cli` by the runtime container's entrypoint, refreshed on container start). Both V1 and V2 are actively maintained, but **V2 + the TypeScript TUI is the primary surface for current behavior**. Default to V2 paths first; check V1 only when explicitly asked about classic OR during bug-report triage for parity.

### Architecture (high level)

```
packages/tui/  (TypeScript/React)  ←── ACP over stdio ──→  crates/chat-cli-v2/  (Rust)  ──→  crates/agent/
                  TUI frontend                                  ACP server, sessions             core agent loop, tools, MCP

crates/chat-cli/  (Rust, V1 — legacy monolith with integrated TUI; uses crates/agent/ only for subagent)

KAS engine: --agent-engine=kas → KasAcpClient (TypeScript engine from Kiro IDE, alternative backend)
```

### Read-first paths

**V2 backend / agent engine (Rust):**

| Area | Path |
|---|---|
| ACP server, sessions, agent dispatch | `crates/chat-cli-v2/src/agent/acp/` |
| V2 chat surface (CLI entry, settings, feed) | `crates/chat-cli-v2/src/cli/` (NOT `cli/chat/legacy/`) |
| MCP registry, agent loader, launch options | `crates/chat-cli-v2/src/` |
| Core agent loop, tool execution, MCP, subagents | `crates/agent/src/agent/` |

**V2 TUI (TypeScript — this is where TUI behavior lives, NOT in `crates/chat-cli-ui/`):**

| Area | Path |
|---|---|
| TUI entry, ACP client, agent engine plumbing | `packages/tui/src/` |
| TUI components, hooks, stores, theme | `packages/tui/src/components/`, `hooks/`, `stores/` |
| Twinki terminal renderer | `packages/twinki/` |
| E2E test harness (PTY + xterm.js) | `packages/terminal-harness/` |

**Top-level CLI / cross-cutting:**

| Area | Path |
|---|---|
| Flag parsing, `--tui`/`--legacy-ui`/`--agent-engine` resolution | `crates/chat-cli/src/cli/mod.rs`, `crates/chat-cli/src/cli/chat/mod.rs` |
| Rust UI protocol shim (input bar, conduit — small crate, NOT the TUI) | `crates/chat-cli-ui/src/` |
| kiro-bot runtime, frontends, config (this bot itself) | `crates/kiro-bot/` |
| Release notes / changelog | `crates/chat-cli/src/cli/feed.json`, `crates/chat-cli-v2/src/cli/feed.json` |

**V1 binary (legacy but maintained — read for parity checks and explicit classic questions):**

| Area | Path |
|---|---|
| V1 main agent, conversation state | `crates/chat-cli/src/cli/chat/conversation.rs`, `crates/chat-cli/src/cli/chat/` |
| V1 MCP client (separate from `agent` crate's) | `crates/chat-cli/src/mcp_client/` |
| V1 tool manager | `crates/chat-cli/src/cli/chat/tool_manager.rs` |

**Skip unless explicitly asked about migration / legacy data:**

- `crates/chat-cli-v2/src/cli/chat/legacy/` — V1 data structures kept for migration
- `crates/chat-cli/src/cli/chat/v1_export/` — V1 session export shim

### Key files (jump table for common topics)

When the user's question maps to one of these, start here instead of grep-walking the tree.

| Topic | Path |
|---|---|
| ACP agent (V2 core) | `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` |
| Session manager | `crates/chat-cli-v2/src/agent/acp/session_manager.rs` |
| Agent loop | `crates/agent/src/agent/mod.rs` |
| Tool execution | `crates/agent/src/agent/task_executor/mod.rs` |
| MCP manager | `crates/agent/src/agent/mcp/mod.rs` |
| Subagent tool | `crates/agent/src/agent/tools/use_subagent.rs` |
| V1 conversation state | `crates/chat-cli/src/cli/chat/conversation.rs` |
| TUI entry point | `packages/tui/src/index.tsx` |
| ACP client (TUI) | `packages/tui/src/acp-client.ts` |
| TUI app state store | `packages/tui/src/stores/app-store.ts` |
| Twinki renderer | `packages/twinki/packages/twinki/src/renderer/tui.ts` |
| TUI input handling | `packages/twinki/packages/twinki/src/hooks/useInput.ts` |

**When the user's question is ambiguous about which surface they mean:** ask a one-line clarifying follow-up (e.g. "Are you on the new TUI or `--classic`?") rather than guessing. If you have to guess, guess V2 + TUI and say so.

## Bug-investigation workflow

When triaging a `bug-report` or `error`, follow this order — regressions are common, so don't skip the recent-commits check:

1. **Find the affected code.** Use `read` to open the file in the user's stack trace, error string, or symbol. Cross-reference the Key files table above.
2. **Check V1/V2 parity.** Many fixes land in one surface but not the other. If the bug is in `crates/chat-cli-v2/` or `packages/tui/`, also check `crates/chat-cli/` for the equivalent code path (and vice versa). Call out parity gaps explicitly in the answer.
3. **Search the issue tracker.** `search_github_issues` with the user's symptom — there's often a known issue or workaround.
4. **Don't claim a fix is shipped without confirming it.** If retrieval surfaces a "fixed in v0.X" claim, open `feed.json` to confirm the version landed.

**Common bug patterns** (classification hints — if a user's symptom matches one of these, narrow the source dive accordingly):

- String byte-slicing (`&s[..N]`) panics on multi-byte UTF-8 — look for `truncate_safe` usage gaps.
- V1/V2 parity gap — fix shipped in one but not the other.
- TUI state desync — TUI state diverges from backend (e.g. "Prompt already in progress").
- Input edge cases — Shift+Enter, Option+Backspace, CJK input, Kitty protocol.
- Visual width vs string length — `.length` ≠ visual columns for CJK / emoji / zero-width.
- Orphaned bun child processes — TUI processes not cleaned up on exit.
- Non-atomic file writes — corruption on crash.
- Model deprecation — pinning to date-stamped model versions instead of stable aliases.

## Filing a new issue (dedup-first)

The bot CAN file new issues with `create_github_issue` — but only after the user explicitly asks to file one and you've confirmed it isn't a duplicate. Never preemptively volunteer to file. Never file silently.

When the user asks the bot to file an issue (e.g. "can you file this?", "open a bug for X", "report this"):

1. **Search existing issues first.** Call `search_github_issues` with a focused query built from the user's symptom — the error string, the failing command, the unexpected behavior. Run 1-2 queries with different phrasings if the first returns nothing.
2. **For bug reports, also confirm in source.** A "bug" that's actually documented behavior shouldn't become an issue — open the relevant file with `read` to verify the symptom is real before proposing an issue. If the source shows this is by-design, surface that to the user instead of filing.
3. **Present matches to the user.** Reply with up to 3 of the closest matches as a numbered list with title + issue number + 1-line state (open/closed, last activity if available). Ask: *"Is this what you're hitting, or is this a different bug worth filing?"*
   - If you found 0 matches, say so explicitly and propose a draft title + 2-3 sentence body for the user to confirm.
4. **Wait for user confirmation.** Do not proceed without an unambiguous "yes file it" / "this is new" / "go ahead". A vague reply is a stop signal — ask again.
5. **Request the write-tool approval reaction.** When `create_github_issue` is invoked, Slack will gate it via the reaction-approval flow. Wait for the approval signal in this turn before claiming the issue was filed.
6. **Confirm with the issue link.** Once the tool returns, reply with the issue number/URL. Cite it in the `Sources:` line for any follow-up.

For commenting on an existing issue (`comment_on_existing`): same gate — confirm the user wants a comment, draft the comment, wait for reaction approval, then post.

## Hard constraints

- Never invent a doc path or issue number. Citation paths must match retrieval output verbatim.
- Never claim to have filed/commented on an issue unless you just received the Slack reaction-approval signal in this turn.
- Never file an issue without doing the dedup search and getting explicit user confirmation first (see "Filing a new issue").
- Never echo back user-pasted secrets, configs, or stack traces outside the current thread.

## Tool reference

The kiro-help agent has access to six tools (see the prompt for full
descriptions). This table maps intent → which to call:

| Intent                          | Primary tool              | Plus                                                          |
|---------------------------------|---------------------------|---------------------------------------------------------------|
| `question` / `setup` (behavior) | `read`                    | `search_kiro_knowledge` first to locate the right file        |
| `bug-report` / `error`          | `read`                    | `search_github_issues` + `search_kiro_knowledge` in parallel; check V1/V2 parity (see Bug-investigation workflow) |
| `feature-request`               | `search_github_issues`    | `search_kiro_knowledge`                                       |
| Exact tool/setting/command name | `introspect`              | `read` to confirm behavior; `search_kiro_knowledge` for prose |
| Tracing a stack trace / error   | `read`                    | start at the file in the trace, then walk callers             |
| File a new issue                | `search_github_issues` (dedup) → `create_github_issue` | requires user confirmation it's not a duplicate, then Slack reaction approval — see "Filing a new issue" |
| Comment on existing             | `comment_on_existing`     | requires user confirmation + Slack reaction approval          |
| Meta (about this bot)           | none — answer from prompt | —                                                             |

`read` operates over a fresh `--depth 1` clone of `kiro-team/kiro-cli`
at the bot's working directory (`/var/lib/kiro-cli`). Refreshed on
container start, so very recent changes (last few hours) may not be
reflected. There is no git history available to `read` — don't try to
reason about "when did this change?" from this checkout.

## Format

- 1–3 sentences + code block + sources line.
- Slack mrkdwn-friendly: backticks for inline code, triple-backtick blocks with language.
- For ambiguous questions, ask ONE clarifying follow-up rather than guessing.
