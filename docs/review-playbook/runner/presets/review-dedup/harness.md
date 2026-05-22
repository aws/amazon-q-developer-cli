This is a one-shot dedup loop. It gates worker-produced findings before
they land in the committed `docs/review-playbook/runner/findings/` tree.

## Inputs

Worker scratchpads. Each `review-code` / `review-blackbox` autoloop run
leaves its findings under its own per-run `{{STATE_DIR}}/findings/`
(autoloop provisions a fresh state directory per run). Dedup reads from
**one or more** of these scratchpads.

- **Auto-detect**: if the objective prompt is empty or the literal
  string `auto`, discover sibling run directories by listing the
  autoloop runs root (autoloop reports it via `autoloop loops`) and
  keep those whose `findings/` subdir was modified more recently than
  the most recent file in `docs/review-playbook/runner/findings/`.
- **Explicit**: the objective prompt may list one or more scratchpad
  paths, one per line:

  ```
  <autoloop-runs-root>/<run-id-a>/findings/
  <autoloop-runs-root>/<run-id-b>/findings/
  ```

  Dedup processes every `*.md` under each listed path (except marker
  files — `-done.md`, `-continuation.md`, `-runbook.md` — which are
  copied through without dedup logic).

## Outputs

- **COPY decisions** → new finding files under
  `docs/review-playbook/runner/findings/<original-name>`, byte-for-byte
  copies of the scratchpad source.
- **ENRICH decisions** → **modifications to existing** finding files in
  `docs/review-playbook/runner/findings/`. The scratchpad source is
  **not** copied.
- **SKIP decisions** → a single line appended to the existing finding's
  `## Related` section. The scratchpad source is not copied.
- **Marker files** (`-done.md`, `-continuation.md`, `-runbook.md`) → copied
  through verbatim. They are per-work-item receipts, not findings.
- A decision log at
  `docs/review-playbook/runner/findings/_dedup-log.md`.

## Global rules

- **Batch-at-end.** Dedup runs once, after all workers finish. It reads
  from multiple scratchpads and produces one coherent commit.
- **One decision per candidate.** Each non-marker finding gets exactly
  one SKIP / ENRICH / COPY classification and is logged.
- **Idempotent.** A candidate whose content (by filename) already exists
  under the committed tree is a no-op (log as `SKIP (already-promoted)`).
- **No re-reading source.** Dedup compares finding bodies, not the
  source files they reference. Work items already did that.
- **Candidate narrowing is mandatory.** Before reading candidate bodies
  into context, narrow via the helper script:

  ```bash
  bash docs/review-playbook/runner/scripts/find-dedup-candidates.sh \
    <file-field-value>
  ```

  The script prints existing finding filenames in the committed tree
  whose frontmatter `file:` value matches the argument. If the script
  returns zero results, the decision is COPY — no further reading
  required.

- **Structural pre-filter, semantic decision.** The helper narrows by
  `file:` equality. You then read the narrowed set plus the new
  candidate and judge SKIP / ENRICH / COPY using body content.
- **Marker files are not deduped.** Copy them verbatim to the committed
  tree. They are work-item receipts.
- **Do not mutate scratchpad sources.** Treat worker scratchpads as
  read-only.
- **Write as you go.** For each candidate, apply the decision and append
  to `_dedup-log.md` immediately. Do not batch in context.

## Decision rubric

### SKIP

Use when the new candidate and an existing finding describe **the same
bug**: same file, same class of problem (trim-missing, subscription-leak,
etc.), same proposed fix region. The existing finding's evidence is at
least as specific as the new one's.

Action:
1. Append to the existing finding's `## Related` section a single line:

   ```
   Also detected by <new-work-item-id> on <YYYY-MM-DD> (skipped as duplicate).
   ```

2. If the existing finding has no `## Related` section, create one at
   the bottom of the body (before any blank trailing lines).
3. Log the decision in `_dedup-log.md`.
4. The scratchpad source is **not** copied. The new candidate does
   not appear in the committed tree at all.

### ENRICH

Use when the new candidate is on the same `file:` (and usually the same
`class`) but adds material information: a different line, a different
platform, a sharper repro, a cross-discovery-mode detection (e.g. code
finding plus corroborating blackbox probe).

Action:
1. Append an `## Additional evidence (from <new-work-item-id>, <YYYY-MM-DD>)`
   section to the existing finding. Copy the new candidate's `Evidence`
   section body into it.
2. Update the existing finding's frontmatter:
   - `platforms-affected`: union with the new candidate's value.
   - `severity`: take the higher of (existing, new). Severity order is
     `crash > spiral > regression > slowdown > smell`.
   - `corroborated-by`: append `<new-work-item-id>` to the list (create
     the field if absent).
3. Leave all other frontmatter fields (`title`, `class`, `file`, `line`,
   `discovered-at`, `discovered-by`) unchanged — the original's identity
   is preserved.
4. Log the decision in `_dedup-log.md`.
5. The scratchpad source is **not** copied.

### COPY

Use when no candidate share the new finding's `file:` with a matching
root cause, or when the helper script returns zero candidates.

Action:
1. Copy the scratchpad file verbatim to
   `docs/review-playbook/runner/findings/<original-filename>`.
2. Log the decision in `_dedup-log.md`.

## Decision log schema

`docs/review-playbook/runner/findings/_dedup-log.md`:

```markdown
# Dedup decision log

**Generated**: <ISO-8601 UTC timestamp>
**Scratchpads processed**: <list of paths>
**Candidates total**: <N>
**SKIP**: <n_skip>
**ENRICH**: <n_enrich>
**COPY**: <n_copy>
**Pass-through markers**: <n_markers>

## Decisions

| # | candidate | decision | target | rationale |
|---|-----------|----------|--------|-----------|
| 1 | `<scratchpad-relative-path>` | COPY   | `<committed-filename>` | no existing finding on this file |
| 2 | `<scratchpad-relative-path>` | SKIP   | `<existing-filename>` | same class + same file as existing |
| 3 | `<scratchpad-relative-path>` | ENRICH | `<existing-filename>` | different line, adds platform=macos |
...
```

The log is regenerated from scratch on every run — do not try to append
to an earlier log.

## Idempotence

If a scratchpad finding already exists at the same filename in the
committed tree (e.g. a re-run of the same worker), log `SKIP (already-promoted)`
and move on. Do not overwrite a committed finding without going through
the ENRICH or COPY paths described above.

## Guardrails

- Do not write findings to `{{STATE_DIR}}`. The dedup hat reads from
  worker scratchpads, but its own output goes straight to the committed
  tree (that is the whole point).
- Do not edit `_rollup.md`, `_validation.md`, or any other sidecar.
  Rollup and validate are separate hats.
- Do not create a finding that was not in any scratchpad. Dedup
  promotes; it does not author.
- Do not set `status: duplicate` on scratchpad files — SKIP drops them
  entirely, it does not tombstone them in the committed tree. The
  decision log is the audit trail.
- On any unresolvable ambiguity (e.g. two candidates seem to be the same
  bug but the evidence does not clearly agree), prefer COPY both and
  leave a "Conflicts" note in `_dedup-log.md`. The rollup hat will
  surface them for human triage.

## Budget awareness

- Narrow candidates via the helper script before reading bodies. Never
  load the whole committed findings tree into context.
- If you have N candidates and the helper says no existing findings
  share `file:` with M of them, those M are automatic COPY decisions —
  log them immediately.
- For the remaining candidates, read the narrowed existing findings
  **one file at a time** and make the decision in context-bounded
  chunks. Commit decisions to the log as you go.

## Durable learnings

If you notice a recurring ambiguous pattern worth remembering, use
`{{TOOL_PATH}} memory add learning ...`. Keep entries short.
