This is a one-shot planner loop for bun/TUI code-safety reviews.

The loop turns a directive into a set of self-contained work items that
downstream agents can execute one at a time. The planner does **not** perform
any review itself. It reads the playbook and the relevant slice of the repo
only far enough to decide how to partition work.

## Global rules

- You emit work items to the autoloop per-run scratchpad at
  `{{STATE_DIR}}/work/`. **Work items are per-run ephemeral artifacts** —
  they are not promoted, copied, or synced into the repo tree. They live
  in the scratchpad for the life of the run; downstream `review-code` and
  `review-blackbox` invocations receive their work item as the objective
  prompt, or a driver reads them directly from the scratchpad.
- One file per work item, plus `_manifest.md` and `_done.md`.
- Do **not** produce findings. Do not read source for the purpose of detecting
  issues — only to decide how to partition.
- Use relative paths in work-item scope. The worker will CWD to the repo root.
- Always split `[code]` and `[blackbox]` techniques into separate work items.
- Target per-item input budget: ~60 000 tokens. Roughly: sum of LOC across a
  work item's scope should stay under ~15 000.
- Partition by **feature** when possible, not by directory. Directory is a
  fallback when feature clustering is unclear.
- For `[blackbox]` work items where no probe script exists yet, mark
  `defer: runbook` and emit a runbook-style item. Do not invent probe scripts.
- Every path in a work item's scope must exist. Verify with `ls` or
  equivalent before including.
- Emit `task.complete` via the event tool once all work-item files,
  `_manifest.md`, and `_done.md` are present under `{{STATE_DIR}}/work/`.

## State files you own

- `{{STATE_DIR}}/work/<id>.md` — one per work item.
- `{{STATE_DIR}}/work/_manifest.md` — the index with rationale column.
- `{{STATE_DIR}}/work/_done.md` — summary of what was produced and what
  was skipped.

These files persist in the autoloop per-run scratchpad
(`{{STATE_DIR}}/work/`, which resolves under `autoloop`'s run directory)
after the run completes, available for post-mortem inspection via
`autoloop loops show <run-id>`. They are **not** copied into the
repository tree.

Ralph, manual, and custom-CI runners that want durable work-item files may
write directly to `docs/review-playbook/runner/work/` as an archive
convention, but the autoloop pipeline does not require or produce that.

## Templates and references

- Playbook index: `docs/review-playbook/README.md`.
- Individual reviews: `docs/review-playbook/<review-id>.md` (for example
  `04-dead-fds.md`).
- Work-item schema: `docs/review-playbook/runner/work-item.template.md`.
- Finding-file schema (only relevant context for the worker — you do not
  emit findings): same template file.

## Parallel conflict handling

Multiple loops may run in parallel in the same repository. If you hit
unexpected file changes, do not panic or revert. Re-read the file and
continue.

## Durable learnings

If you identify a recurring partitioning pattern worth remembering, use
`{{TOOL_PATH}} memory add learning ...` to record it. Keep entries short.
