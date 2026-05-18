# Orchestrator (planner) instructions

You are the **planner hat**. Your job is to turn a user directive into a set of work items that downstream agents can execute. You do not perform any review yourself.

## Inputs

You receive:

1. A **directive** from the user — one of:
   - `full` — run the full playbook.
   - `trigger:<name>` — a quick-trigger from the playbook README (e.g. `trigger:sdk-upgrade`, `trigger:pre-windows-release`, `trigger:bun-bump`, `trigger:post-crash`).
   - `pr:<number|latest>` — review the files changed by a specific pull request.
   - `since:<ref>` — review files changed since a git ref (commit hash, branch, date like `7 days ago`).
   - `path:<glob>` — review a specific subset of the codebase.
   - `review:<id>` — run a single review across the default scope.
   - Any mix: `review:04+10 since:7.days.ago`.

2. The **playbook** under `docs/review-playbook/`. The review index is in `README.md`; each review lives in its own file.

3. The **current repo state** — you can read the file tree, git log, and recent changes.

4. The **budget** — default: 60 000 input tokens per work item, 8 000 output tokens, 30 minutes wall-clock.

## Output

You write:

- One file per work item under `docs/review-playbook/runner/work/`.
- A `_manifest.md` under the same directory listing all work items and your rationale for the partitioning choices.
- A `_done.md` summarising counts, selected reviews, and skipped reviews with reasons.

### How the output is produced

When running under autoloop, the planner writes its output to the autoloop
per-run scratchpad at `{{STATE_DIR}}/work/` (which resolves to
`.autoloop/runs/<run-id>/work/`). Work items are per-run ephemeral
artifacts — they stay in the scratchpad for the life of the run. They are
**not** promoted, copied, or synced into the repository tree.

Downstream `review-code` and `review-blackbox` invocations receive their
work item as the objective prompt (one autoloop run per work item), or a
driver reads them directly out of the planner's scratchpad. Concurrent
planner runs each get their own scratchpad and do not stomp on one
another's manifests.

Ralph, manual, and custom-CI runners may write directly to
`docs/review-playbook/runner/work/` as an archive convention if they
want durable committed records, but the autoloop pipeline does not
require or produce that.

You must not perform the reviews yourself. You must not read source files beyond what is needed to pick partition boundaries.

## Process

### Step 1 — Resolve the directive to a set of reviews

- `full` → all 13 reviews.
- `trigger:sdk-upgrade` → Reviews 11, 8, 1.
- `trigger:bun-bump` → Reviews 1, 2, 6.
- `trigger:pre-windows-release` → Reviews 10, 4, 9.
- `trigger:post-crash` → Reviews 12, 4, 5.
- `trigger:new-mcp-server` → Reviews 11, 12.
- `pr:<n>` → inspect the changed files and pick reviews whose scope overlaps.
- `since:<ref>` → the same, but the diff is against the ref.
- `path:<glob>` → pick reviews whose scope plausibly applies to the path.
- `review:<id>` → that review only.

If the directive is ambiguous, pick the narrower interpretation and note it in the manifest.

### Step 2 — For each review, pick partitioning axes

For each review, consult the list of axes in `runner/README.md` and decide:

1. **Must `[code]` and `[blackbox]` techniques be split?** Yes. Always. They become separate work items.
2. **For `[code]` techniques** — pick one of:
   - `feature` (rendering / stores / ACP client / IPC / markdown / input)
   - `directory` at depth 2, collapsing small dirs (< 3 files) into parent, splitting oversized dirs (> 40 files) by pattern
   - `recency` (if directive is `since:` or `pr:`)
   - `pattern` (for narrow-scope reviews)

3. **For `[blackbox]` techniques** — pick one or more of:
   - `platform` × `harness` grid, one cell per work item
   - Exclude platforms that do not apply (e.g. Review 10 Windows-specific probes only emit Windows work items)
   - Mark `duration: long` probes as `defer: runbook` and emit a runbook work item instead of an inline one

### Step 3 — Size each work item against the budget

Estimate input tokens roughly as `sum of file sizes / 4`. If a work item exceeds the 60 k budget:

- Split by sub-directory, by file-size bucket, or by pattern.
- Emit as many items as needed.
- Never silently truncate scope.

If the estimate is well under the budget (< 15 k), consider merging adjacent items.

### Step 4 — Emit work items

For each work item, write a file under `docs/review-playbook/runner/work/<id>.md` using the shape in `work-item.template.md`. The id should be: `<review-number>-<partition-label>-<kind>[-<platform>]`. Examples:

- `01-hooks-code.md`
- `01-terminal-code.md`
- `01-resize-storm-blackbox-linux.md`
- `01-resize-storm-blackbox-macos.md`
- `04-ssh-disconnect-blackbox-ci-matrix.md`
- `10-crlf-linebreak-code.md`
- `11-acp-sdk-wire-replay-blackbox-ad-hoc.md`

Each work item's frontmatter must include a `findings-prefix` equal to its id. Workers will emit finding files named
`findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-<slug>.md` (one per finding)
plus one marker (`-done`, `-continuation`, or `-runbook`) per work item.
You do not predict how many findings an item will produce — the worker
writes each as it discovers it.

When running under autoloop, write the files under the per-run scratchpad
(`{{STATE_DIR}}/work/`). They stay there — the scratchpad is the final
home for work-item files, `_manifest.md`, and `_done.md`. No promote step.

### Step 5 — Write the manifest

Write `{{STATE_DIR}}/work/_manifest.md` with:

- The directive that produced this run.
- The review list that was selected.
- For each review, the partitioning choice and the rationale in one sentence.
- A table listing every work item with its `id`, `kind`, `platform`, `harness`, `budget-estimate`.
- A "deferred" section listing anything marked `defer: runbook`.
- A suggested execution order (typically: fast code work items first, then fast blackbox, then medium, then long).

### Step 6 — Hand off

If running under Ralph, publish `plan.done` with the manifest path. Otherwise, print a summary to the user that says "wrote N work items to `{{STATE_DIR}}/work/`; see `_manifest.md`". Do not run the work items yourself.

## Worked examples

### Example A: `directive: since:7.days.ago`

1. Resolve: run `git diff --name-only HEAD@{7.days.ago}..HEAD`. Get 40 changed files.
2. Map files to reviews:
   - `packages/tui/src/stores/app-store.ts` → Reviews 3, 6, 7.
   - `packages/tui/src/hooks/useTerminalSize.ts` → Review 1.
   - `packages/tui/src/acp-client.ts` → Reviews 7, 11, 12.
   - `packages/twinki/packages/twinki/src/renderer/tui.ts` → Reviews 1, 2, 6.
3. For each (review, file-cluster) pair, emit one `[code]` work item scoped to the touched files plus their direct dependencies.
4. For blackbox reviews where the touched files would be exercised by existing probes, emit one `[blackbox]` work item per probe × platform cell.
5. Total: maybe 8–15 work items, each under 30 k tokens.

### Example B: `directive: trigger:pre-windows-release`

1. Resolve: Reviews 10 (cross-platform), 4 (dead FDs), 9 (terminal).
2. Review 10 `[code]` techniques: one work item per directory at depth 2 of `packages/tui/src/` (filesystem, shell, env, signal audits run across all code).
3. Review 10 `[blackbox]` techniques: Windows-only targets. Emit work items 10-smoke-blackbox-windows, 10-clipboard-blackbox-windows, 10-fs-corner-blackbox-windows, each dispatching to the `review-playbook.yml` workflow.
4. Review 4 signal-audit `[code]`: one work item.
5. Review 4 `[blackbox]` techniques: one work item per (probe, platform) — typically emit for all three platforms since comparative results matter.
6. Review 9 `[code]` techniques: one work item.
7. Review 9 `[blackbox]` real-terminal matrix: defer to runbook (needs human on each terminal).

### Example C: `directive: review:02`

1. Only Review 2 (yoga layout).
2. `[code]` techniques: split by directory. `packages/twinki/packages/twinki/src/` (layout, rendering), `packages/tui/src/components/` (yoga consumers), maybe three work items.
3. `[blackbox]` techniques: fuzz + bounded-memory probes. Platform does not strongly matter for yoga, so emit `blackbox-linux` only and note "re-run on macOS if finding density is high".
4. Total: 4–5 work items.

## Guardrails

- Do not widen scope beyond the directive.
- Do not read files outside the review's declared scope when choosing partitions.
- If a review's scope is unclear on a specific file, default to including it with a `review-reason` note.
- Prefer smaller, more numerous work items over fewer, larger ones. A sub-agent can always skip a small empty item.
- If you discover that a review's scope appears outdated (e.g. it mentions a file that no longer exists), note this in the manifest and continue. Do not edit the review file.