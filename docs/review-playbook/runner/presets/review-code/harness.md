This is a one-shot code-review loop for a single `[code]` work item.

The loop executes one work item from the bun/TUI code-safety review playbook.
The objective prompt is the work-item contents; the reviewer applies the
techniques listed in the work item's frontmatter to the files in scope and
emits per-finding files in the playbook's finding-file schema.

## Global rules

- Your output is a set of per-finding Markdown files under
  `{{STATE_DIR}}/findings/` (the autoloop per-run scratchpad). The
  `review-dedup` hat promotes surviving findings from all worker
  scratchpads into `docs/review-playbook/runner/findings/` at the end of
  the run; you do **not** write to the committed path directly.
- Ralph, manual, and custom-CI runners may write straight to
  `docs/review-playbook/runner/findings/` if they are not running dedup.
  When in doubt, follow the scratchpad convention — it is always safe.
- Execute **only** the `[code]` techniques listed in the work item's
  `techniques:` frontmatter. Do not run `[blackbox]` techniques.
- Apply techniques **only** to files matching `scope.include` minus
  `scope.exclude`. Do not expand scope.
- Read each file in scope **at most once**. If a finding needs context
  from another file, note it but do not re-read.
- **One file per finding.** Write to disk immediately when a finding is
  identified. Do not batch.
- Use the schema in `docs/review-playbook/runner/work-item.template.md`.
  Every finding file has frontmatter: id, work-item, review, technique,
  class, severity, file, line, platforms-affected, discovered-by,
  discovered-at, status.
- File naming:
  `{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-<kebab-slug>.md`
  where slug is a short (~50 char) lowercase kebab summary of the
  finding title. Date and time are UTC.
- Emit a done marker named
  `{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-done.md`
  with `findings-emitted: N` in frontmatter when complete.
- If you exhaust your iteration budget, write a continuation marker
  `{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-continuation.md`
  describing what remains and emit `task.complete` anyway. The next run
  will pick it up.
- Emit `task.complete` via the event tool once all findings and the
  marker file are written.
- Use `status: open` for every finding you emit. The dedup and validate
  hats transition findings to `resolved`, `duplicate`, or
  `needs-human-check` later in the pipeline.

## Severity rubric

- `crash` — process exits unexpectedly, throws to top level, corrupts state.
- `spiral` — unbounded resource growth the user cannot recover from.
- `slowdown` — bounded but noticeable degradation.
- `regression` — works on one platform / terminal / locale, fails on another.
- `smell` — suspicious pattern that works today but could regress.

## Platforms

Windows has no `SIGPIPE` / `SIGHUP`. macOS is a special POSIX flavour. Linux is
the reference POSIX. Call out platform-specific behaviour in
`platforms-affected`.

## Evidence

Every finding body includes:

- A one-line title.
- A short paragraph explaining what and why.
- An `Evidence` section with a code snippet or grep output, including
  file:line references.
- A `Proposed fix` section with a concrete snippet or one-paragraph
  description.

## Durable learnings

If you notice a recurring anti-pattern worth remembering across review runs,
use `{{TOOL_PATH}} memory add learning ...`. Keep it short.

## State directory

`{{STATE_DIR}}/` is the per-run scratchpad. Findings go under
`{{STATE_DIR}}/findings/` — the `review-dedup` hat promotes surviving
findings into `docs/review-playbook/runner/findings/` at the end of the
pipeline. You may use `{{STATE_DIR}}/` for any intermediate notes you
want.
