# Shell-command discipline (`execute_bash`)

Reference for the kiro-help bot. The agent has limited shell access via `execute_bash` for read-only investigation — primarily git history (`log`, `blame`, `show`) and search (`rg`, `find`, `wc`). All other commands are denied by the agent config's `toolsSettings.shell` allowlist; the bot's runtime cannot run write commands, network commands, or arbitrary scripts.

Load this reference when:
- The user asks "when did X change?" / "why was Y added?" / "who wrote this?" — git history questions.
- You need to find a symbol or string across the tree faster than reading individual files.
- Step 4 of the bug-investigation workflow (`references/bug-investigation.md`) — checking recent commits for regressions.

## What you CAN run

The agent config allows these patterns. They run from the bot's working directory (`/var/lib/kiro-cli`, the kiro-cli checkout).

**Git history (read-only):**

| Command | Purpose | Example |
|---|---|---|
| `git log` | Recent commits, optionally scoped to a path | `git log --oneline -20 -- crates/chat-cli-v2/src/cli/chat/mod.rs` |
| `git log --since=...` | Commits in a time window | `git log --since="2 weeks ago" --oneline crates/agent/` |
| `git blame` | Line-by-line authorship | `git blame -L 320,360 crates/chat-cli/src/cli/chat/mod.rs` |
| `git show` | Full diff of a specific commit | `git show <sha> -- <path>` |
| `git diff` | Compare commits or branches | `git diff HEAD~5 HEAD -- packages/tui/` |
| `git status` | Working tree state (mostly clean — read-only checkout) | `git status` |
| `git rev-parse HEAD` | Current commit SHA (useful for "as of which version") | `git rev-parse HEAD` |

**Note on `--depth 1`:** the runtime container's clone is shallow. `git log` returns only the most recent commit by default. The container entrypoint runs `git fetch --unshallow` after the initial clone to backfill history — that's a container-level operation the bot itself cannot replicate (`git fetch` is on the denylist). If `git log` returns one commit when you expect many, the unshallow fetch hasn't completed yet; tell the user the bot's clone may be still fetching history and to retry.

**Code search:**

| Command | Purpose | Example |
|---|---|---|
| `rg` (ripgrep) | Fast regex search | `rg -n 'AgentEngine::V1' crates/` |
| `rg --type rust` | Scope by language | `rg --type rust 'fn resolve_skill' crates/` |
| `find` | File-name search | `find crates/agent/src -name 'fs_*.rs'` |
| `wc -l` | Line count | `wc -l crates/chat-cli-v2/src/cli/chat/mod.rs` |

**Filesystem inspection (read-only):**

| Command | Purpose |
|---|---|
| `ls`, `stat`, `file` | Directory and file metadata |
| `head -N`, `tail -N` | Quick peeks (prefer `read` for actual content with line numbers) |

## What you CANNOT run (and shouldn't try)

The shell allowlist denies these by default. The agent will get a permission error if it tries — don't tell the user "I tried but it was denied"; just don't try.

- **Anything that writes:** `git commit`, `git push`, `git checkout`, `git reset`, `rm`, `mv`, `mkdir`, `>` redirection, `tee`.
- **Anything that fetches:** `curl`, `wget`, `git fetch` (entrypoint already did this), `git pull`, `npm install`, `cargo install`, `bun install`.
- **Process control:** `kill`, `killall`, `pkill`.
- **Privilege:** `sudo`, `su`, `chmod`, `chown`.
- **Eval-like:** `bash -c`, `sh -c`, `eval`, `source`, sourcing scripts.
- **Anything not on the allowlist.** When in doubt, don't try — fall back to `read`.

## Discipline

1. **Prefer `read` for file contents.** `read` gives you line numbers and integrates with the citation flow. Use `cat`/`head`/`tail` only as a quick sanity check, not to draft answers.
2. **Always scope `git log` and `git blame` to a path.** Unscoped `git log` on the kiro-cli repo dumps thousands of commits — wasteful. Either pass `-- <path>` or `-n <count>`.
3. **Cite shell output in `Sources:`.** Format as `git log crates/foo.rs (HEAD~10..HEAD)` or `git blame crates/foo.rs:120-145` — same idea as file path citations.
4. **Don't use shell to bypass dedup.** When the user asks to file an issue, you still go through `references/issue-filing.md` — not `gh issue create` (which isn't allowed anyway).
5. **Single command per call.** Don't chain with `&&` or `;` to slip a denied command past the allowlist parser. Run each step as its own tool call.
6. **Output budget.** Truncate aggressively when reporting back — `git log --oneline -20` is almost always enough; if the user wants more, ask.

## Examples — typical patterns

**"When did the `--tui` flag change to default?"**

```
git log --oneline -30 -- crates/chat-cli/src/cli/chat/mod.rs
```
If the result list is too long, refine with another single call (no pipes — see rule 5):
```
git log --oneline --grep='tui' -- crates/chat-cli/src/cli/chat/mod.rs
```
Then `read` the most relevant commit's file at the right revision.

**"Why was `truncate_safe` added?"**

```
git log --all --oneline -S 'truncate_safe' -- crates/agent/src/agent/util/
git show <sha-of-introducing-commit>
```

**"Did fix for [symptom] land in V1 too?"**

```
git log --oneline --since='3 months ago' --grep='<keyword>' -- crates/chat-cli/src/cli/chat/
```
If a parallel V1 commit isn't visible, that's a parity gap — surface it in the answer.

## When NOT to use shell

- For routine file reads — `read` is always preferable.
- For "what does X currently do?" — read the file at HEAD; history is usually irrelevant.
- For schema lookups — `introspect` is faster.
- For doc retrieval — `search_kiro_knowledge`.

The shell tool exists specifically for *temporal* and *cross-file search* questions that the other tools can't answer.
