# Rollup instructions

You are the **rollup hat**. Your job is to read every finding file under `docs/review-playbook/runner/findings/` and produce a single ranked, reviewable report at `docs/review-playbook/runner/findings/_rollup.md`.

You do not read source code. You only read finding files.

Rollup is the **last** stage of the review pipeline. By the time you run,
the `review-dedup` hat has already promoted findings into
`docs/review-playbook/runner/findings/` and handled SKIP/ENRICH/COPY
decisions (tracked in `_dedup-log.md`). The `review-validate` hat may have
transitioned some findings to `status: resolved` or
`status: needs-human-check` (tracked in `_validation.md`). Rollup
aggregates what remains into a single human-readable report.

## Inputs

- Every `.md` file under `docs/review-playbook/runner/findings/` that **is not** a marker file (`*-done.md`, `*-continuation.md`, `*-runbook.md`) and **is not** an underscore-prefixed sidecar (`_rollup.md`, `_dedup-log.md`, `_validation.md`).
- Marker files, used separately to detect coverage gaps.
- The dedup log at `_dedup-log.md` if present, for the "Dedup summary" section.
- The validation report at `_validation.md` if present, for the "Validation summary" section.
- The manifest at `{{STATE_DIR}}/work/_manifest.md` if the planner's
  scratchpad is still available — it lists which work items should have
  run. Rollup tolerates its absence.
- Severity rubric from `docs/review-playbook/README.md`.

## Output

A single `_rollup.md` under `docs/review-playbook/runner/findings/` with the sections below. The underscore prefix keeps it first in an alphabetical listing.

## Process

### Step 1 — Load all findings

For each `.md` file in `findings/` other than markers and underscore sidecars:

- Parse the YAML frontmatter.
- Record `{id, work-item, review, technique, class, severity, file, line, platforms-affected, discovered-by, discovered-at, status, duplicate-of, corroborated-by, resolved-at, resolved-at-commit}`.
- Read the body as the human description (kept verbatim; do not re-word).

### Step 2 — Load markers

For each `-done.md`, `-continuation.md`, `-runbook.md` marker:

- Record the work item id it belongs to (derived from the filename prefix).
- Record whether it indicates completion, partial completion, or a deferred runbook.

### Step 3 — Detect coverage gaps

If the manifest is available, cross-reference:

- Every non-deferred work item should have at least one marker file (at minimum a `-done.md`).
- Work items with no marker: add to "Not run".
- Work items with only a `-continuation.md` (no `-done.md`): add to "Incomplete".
- Work items that are `defer: runbook` in the manifest should have a `-runbook.md` marker; list them in "Runbooks emitted".

If the manifest is absent, skip this step and note it in the rollup header.

### Step 4 — Filter by status

Dedup and validate have already normalised statuses. For the main ranked
tables, include findings where `status == open`. Report other statuses
in their own sections:

- `resolved` findings in a separate "Resolved since last validate" section,
  with their `resolved-at-commit` so readers can audit the fix.
- `duplicate` findings: skip from the main tables; summarise counts only
  (they are already collapsed upstream by dedup).
- `wontfix` findings: separate section so humans remember what was
  explicitly accepted.
- `needs-human-check` findings: pulled into a top-of-report section to
  prompt triage.

### Step 5 — Cross-link near-duplicates

Two `open` findings are **near-duplicates** if they share `file` (and
`line` if both have one) but were not merged by dedup (different
`class`, different `discovered-by`, or cross-review signal). Keep both
but add a cross-link in each finding's `## Related` section during the
rollup read pass — this is a report-only operation, do **not** rewrite
finding files.

Explicit duplicates (`status: duplicate` with `duplicate-of` set) are
not re-listed here; they were handled by the dedup hat.

### Step 6 — Rank

Sort surviving `open` findings by severity, highest first: `crash > spiral > regression > slowdown > smell`.

Within a severity, sort by `review` id (Review 4 before Review 12 within the same severity).

### Step 7 — Group for browsability

Produce three views of the same data:

- **By severity** — one section per severity level.
- **By review** — one section per review.
- **By file** — one section per file, listing every finding against it. Useful for "how bad is this file?"

A finding appears in all three views.

### Step 8 — Flag conflicts

If two findings against the same `file:line` propose contradictory fixes, flag them in a "Conflicts" section at the top. The planner or a human decides which applies.

### Step 9 — Write the rollup

Use the template below. Link to each finding by its filename so a reader can click through.

## Rollup template

```markdown
# Review rollup

**Generated**: <ISO timestamp>
**Directive**: <copied from the manifest, or "unknown" if absent>
**Work items in manifest**: <count or "manifest unavailable">
**Work items with `done` marker**: <count>
**Work items with `continuation` marker**: <count>
**Work items missing all markers**: <count>
**Deferred runbooks**: <count>

## TL;DR

- <count> open findings (after dedup + validate).
- Severity: <crash> crash, <spiral> spiral, <regression> regression, <slowdown> slowdown, <smell> smell.
- <count> findings need human check.
- <count> findings resolved since last validate.
- <count> findings wontfix (accepted).
- <count> findings collapsed as duplicates by dedup.
- <count> conflicts flagged for human review.

## Needs human check

| severity | file:line | review | class | finding | file |
|----------|-----------|--------|-------|---------|------|

## Conflicts

| severity | file:line | finding A | finding B | notes |
|----------|-----------|-----------|-----------|-------|

## Not run

| work-item id | kind | partition | platform | reason |
|--------------|------|-----------|----------|--------|

## Incomplete

| work-item id | budget hit | continuation file |
|--------------|-----------|-------------------|

## Runbooks emitted

| work-item id | runbook file |
|--------------|--------------|

### How to implement a deferred runbook

Each runbook describes a blackbox probe that needs to be implemented as a
script. To convert a runbook into an executable probe:

1. **Create the probe script** at `packages/tui/scripts/probes/<name>.ts`
   where `<name>` matches the work-item id (e.g. `01-resize-storm-blackbox`
   → `resize-storm.ts`).

2. **Follow the probe contract**:
   - Launch `chat_cli` in a PTY (use `CHAT_CLI_BIN` env var or default to
     `target/debug/chat_cli`)
   - Apply the stress condition described in the runbook
   - Measure the metrics specified (RSS, FD count, render count, responsiveness)
   - Exit `0` on pass, `1` on fail (threshold breached), `2` on probe crash
   - Write findings to `$PROBE_OUTPUT_DIR` (or `docs/review-playbook/runner/findings/`)
   - Write `<finding-id>-metrics.json` with structured measurements
   - Write `<finding-id>-stdout.log` and `<finding-id>-stderr.log` (full capture, up to 10MB)

3. **Use existing probes as reference**: See `packages/tui/scripts/probes/resize-storm.ts`
   and other probes in that directory for the implementation pattern.

4. **Test locally**: `bun run packages/tui/scripts/probes/<name>.ts`

5. **The next playbook run will automatically execute it** — the un-defer
   step detects new probe scripts and patches cached work items to run them.

## Dedup summary

Copied verbatim from `_dedup-log.md` (counts and one-line rationale per decision), or "no dedup log present" if dedup did not run.

## Validation summary

Copied verbatim from `_validation.md` (counts: N validated, M resolved, K still open, J needs-human-check), or "no validation report present" if validate did not run.

## Resolved since last validate

| severity | file:line | review | class | finding | resolved-at-commit | file |
|----------|-----------|--------|-------|---------|-------------------|------|

## Wontfix

| severity | file:line | review | class | finding | reason | file |
|----------|-----------|--------|-------|---------|--------|------|

## Findings by severity

### Crash

| file:line | review | class | finding | file |
|-----------|--------|-------|---------|------|

### Spiral
...

### Regression
...

### Slowdown
...

### Smell
...

## Findings by review

### Review 01 — async render path

| severity | file:line | class | finding | file |
|----------|-----------|-------|---------|------|

### Review 02 — yoga layout
...

## Findings by file

### packages/tui/src/hooks/useTerminalSize.ts

| line | severity | review | class | finding | file |
|------|----------|--------|-------|---------|------|

### packages/twinki/packages/twinki/src/renderer/tui.ts
...
```

## Column conventions

- `file` in the "by severity" and "by review" tables is the source code location (from the finding's `file:` field plus `:line`).
- `file` in the last column of each table is the **finding file** — the `.md` file under `findings/`. Link it so the reader can jump through.
- `finding` is the title line of the finding (the `# ...` at the top of its body).

## Guardrails

- Do not invent findings.
- Do not re-classify severity. Take what the finding file says.
- Do not re-word finding descriptions. Quote titles verbatim.
- Do not read source files. If a finding is unclear, link to it and flag it.
- Do not dedupe or validate from within rollup — those are separate
  hats with their own presets. Rollup is read-only aggregation.
- Do not merge findings across severities unless they are clearly the same issue with different classifications — in that case, log a conflict, not a merge.
- The rollup is regenerated each time; never hand-edit it. Edits belong in finding files or the manifest.
