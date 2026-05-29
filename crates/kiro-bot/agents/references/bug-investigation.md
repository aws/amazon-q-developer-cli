# Bug-investigation workflow

Reference for the kiro-help bot. Load this when classify=`bug-report` or `error` in the main workflow.

## Steps

When triaging a `bug-report` or `error`, follow this order — regressions are common, so don't skip the recent-commits / parity check:

1. **Find the affected code.** Use `read` to open the file in the user's stack trace, error string, or symbol. Cross-reference the "Key files" table in `references/source-tree.md`.
2. **Check V1/V2 parity.** Many fixes land in one surface but not the other. If the bug is in `crates/chat-cli-v2/` or `packages/tui/`, also check `crates/chat-cli/` for the equivalent code path (and vice versa). Call out parity gaps explicitly in the answer.
3. **Search the issue tracker.** `search_github_issues` with the user's symptom — there's often a known issue or workaround.
4. **Check git history (when execute_bash is available).** Regressions often map to a recent commit. See `references/shell-commands.md` for the safe `git log` / `git blame` commands you can run, and the path-scoping discipline.
5. **Don't claim a fix is shipped without confirming it.** If retrieval surfaces a "fixed in v0.X" claim, open `feed.json` to confirm the version landed.

## Common bug patterns

Classification hints — if a user's symptom matches one of these, narrow the source dive accordingly. These patterns have produced repeated production bugs.

- **String byte-slicing.** `&s[..N]` panics on multi-byte UTF-8 (CJK, emoji). Look for missing `truncate_safe` calls. Workspace-level `clippy::string_slice = "deny"` is supposed to catch these.
- **V1/V2 parity gap.** Fix shipped in one surface but not the other. Always check both when triaging.
- **TUI state desync.** TUI state diverges from backend (e.g. "Prompt already in progress", buttons stuck in disabled state). Look at `packages/tui/src/stores/`.
- **Input edge cases.** Shift+Enter, Option+Backspace, CJK input, Kitty protocol. `packages/twinki/packages/twinki/src/hooks/useInput.ts` is the usual home.
- **Visual width vs string length.** `.length` ≠ visual columns for CJK / emoji / zero-width. Use `visibleWidth()` from `text-width.ts`.
- **Orphaned bun child processes.** TUI processes not cleaned up on exit.
- **Non-atomic file writes.** `writeFileSync` without temp+rename can corrupt on crash.
- **Model deprecation.** Pinning to date-stamped model versions instead of stable aliases.
- **Broadcast channel back-pressure.** Shell output floods channel; needs batching (32ms interval).
- **MCP governance bypass.** Enterprise/API-key users must have tool governance enforced.

## Don't-do shortcuts

- **Don't guess at "when did X change?" without git access.** The clone is `--depth 1`, so without `execute_bash` enabled, the answer is "I can't see history — recommend the user check `git log` on their checkout."
- **Don't assume the user's report describes the V2 surface.** The bug might only repro under `--classic` / `--legacy-ui`. If you can't tell, ask.
- **Don't file an issue automatically just because a bug is confirmed.** Filing requires explicit user opt-in plus dedup — see `references/issue-filing.md`.
