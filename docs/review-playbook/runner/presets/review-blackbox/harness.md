This is a one-shot blackbox-review loop for a single `[blackbox]` work item.

The loop executes one work item from the bun/TUI code-safety review playbook.
The objective prompt is the work-item contents; the reviewer runs probes or
existing tests named by the work item's techniques and emits per-finding files
in the playbook's finding-file schema.

## Global rules

- Your output is a set of per-finding Markdown files under
  `{{STATE_DIR}}/findings/` (the autoloop per-run scratchpad). The
  `review-dedup` hat promotes surviving findings from all worker
  scratchpads into `docs/review-playbook/runner/findings/` at the end of
  the run; you do **not** write to the committed path directly.
- Ralph, manual, and custom-CI runners may write straight to
  `docs/review-playbook/runner/findings/` if they are not running dedup.
  When in doubt, follow the scratchpad convention — it is always safe.
- Execute **only** the `[blackbox]` techniques listed in the work item's
  `techniques:` frontmatter. Do not run `[code]` techniques.
- Respect the work item's `harness` field. Possible values include:
  - `unit` — run `bun test` in the relevant package directory.
  - `integration` — run `bun run test:integ`.
  - `e2e` — run `bun run test:e2e` (requires build).
  - `ad-hoc` — run `bun run packages/tui/scripts/probes/<name>.ts`.
  - `ci-matrix` — the work item will be dispatched to CI via
    `.github/workflows/review-playbook.yml`; locally, emit a runbook.
  - `null` — you must identify the harness from the technique text.
- Respect the work item's `platform` field. If it says `linux` and you
  are on a different platform, emit a regression finding noting the
  mismatch, then the done marker.
- If `defer: runbook`, emit a runbook file (human-readable steps to
  run the probe manually) instead of executing anything.
- **One file per finding.** Write to disk immediately when an invariant
  fails. Do not batch.
- Use the schema in `docs/review-playbook/runner/work-item.template.md`.
- File naming:
  `{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-<kebab-slug>.md`
  (UTC time).
- Emit a done marker named
  `{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-done.md`
  when complete.
- Capture command output and exit codes in each finding's `Evidence`
  section. Include the exact command line you ran.
- Emit `task.complete` via the event tool once the done marker is
  written.
- Use `status: open` for every finding you emit. The dedup and validate
  hats transition findings to `resolved`, `duplicate`, or
  `needs-human-check` later in the pipeline.

## Severity rubric

- `crash` — test crashed, probe exited non-zero unexpectedly.
- `spiral` — runtime resource growth that did not self-limit (e.g. RSS
  still climbing after the test ended).
- `slowdown` — measurement exceeded expected wall-clock but did not crash.
- `regression` — platform-specific behaviour that differs from the
  reference platform, or a missing probe / harness mismatch.
- `smell` — a probe that passes but reveals a fragile assumption.

## Runbook mode

When `defer: runbook`, do not execute anything. Emit a single file at
`{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-runbook.md`
containing:

1. YAML frontmatter with `kind: runbook`, `platform`, `harness`, and
   the work-item id.
2. A heading with the work-item title.
3. Prerequisites (build steps, environment variables).
4. Steps (numbered, specific, each with a command or user action).
5. What to measure and how.
6. Pass / fail budgets.
7. Where to record results.

Then emit the done marker with `findings-emitted: 0` and a note that
the run was deferred to a runbook.

## Evidence requirements

Every finding body includes:

- One-line title.
- Short paragraph: what failed and why it matters.
- `Evidence` section: the exact command, exit code, and key output
  lines. Use code fences for multi-line output.
- `Proposed fix` section: short snippet or one paragraph.

## Platform and environment

- The workflow sets `KIRO_PROBE_COMMIT`, `KIRO_PROBE_REF`,
  `KIRO_PROBE_BUILD`, `KIRO_PROBE_PLATFORM`, `KIRO_PROBE_BUN_VERSION`
  in the environment when running in CI. Include these in the done
  marker's frontmatter so every finding is traceable.
- Locally, read from `git` and `bun --version` if the environment
  variables are unset.

## Knight Rider sidecar (interactive TUI testing)

When running in CI via the review-playbook workflow, a Knight Rider
instance is available at `http://localhost:3001`. It wraps a live
`kiro-cli chat --tui` process in a real PTY and exposes an HTTP API for
driving the TUI interactively.

**When to use it:** Work items with `knight-rider: true` in their
frontmatter benefit from interactive testing. Use Knight Rider when the
technique requires observing UI behaviour, testing slash commands,
verifying visual output, or exercising flows that probes cannot script
headlessly.

**Availability check:** Before using the API, verify readiness:

```bash
curl -sf http://localhost:3001/api/status | grep -q '"ready"'
```

If Knight Rider is not ready (build was skipped, boot failed), fall back
to the technique's non-interactive path or emit a regression finding.

**Key endpoints:**

| Method | Endpoint | Body | Purpose |
|--------|----------|------|---------|
| GET | `/api/status` | — | `{ ready, frameCount, outputDir }` |
| GET | `/api/screen` | — | `{ lines: string[] }` — current terminal |
| POST | `/api/keys` | `{ "keys": "x" }` | Send keystrokes (char by char, 40ms delay recommended) |
| POST | `/api/enter` | — | Press Enter |
| POST | `/api/escape` | — | Press Escape |
| POST | `/api/up` | — | Arrow Up |
| POST | `/api/down` | — | Arrow Down |
| POST | `/api/ctrlc` | — | Ctrl+C |
| POST | `/api/wait-for-text` | `{ "text": "...", "timeout": 10000 }` | Block until text visible |
| POST | `/api/sleep` | `{ "ms": 1000 }` | Sleep N ms |
| POST | `/api/frame` | `{ "label": "name" }` | Capture named screenshot |
| POST | `/api/resize` | `{ "cols": 120, "rows": 40 }` | Resize terminal |
| GET | `/api/memory` | — | `{ rss, heapUsed, rssMB, heapUsedMB }` |

**Interaction pattern:**

```bash
KR="http://localhost:3001/api"

# Type text char by char
for c in $(echo "hello" | fold -w1); do
  curl -s -X POST $KR/keys -d "{\"keys\":\"$c\"}" > /dev/null
  sleep 0.04
done

# Send prompt and wait for response
curl -s -X POST $KR/enter
curl -s -X POST $KR/wait-for-text -d '{"text":"ask a question","timeout":60000}'

# Capture evidence frame
curl -s -X POST $KR/frame -d '{"label":"after-prompt"}'
```

**Evidence:** Frames are saved to `.knight-rider-frames/` and uploaded
as a separate artifact (`knight-rider-<review-id>`). Reference frame
labels in your finding's Evidence section. The output directory also
contains `index.html` — a self-contained video replay of all frames.

**Rules when using Knight Rider:**

- Always capture a frame before and after significant actions.
- Use `/api/wait-for-text` for synchronisation — do not rely on fixed sleeps.
- If the TUI becomes unresponsive, capture a frame labelled `stuck-*`,
  then emit a `crash` finding.
- Do not kill or restart the Knight Rider process — it is shared across
  all work items in the review job.

## Durable learnings

Record recurring invariant shapes via `{{TOOL_PATH}} memory add learning ...`.
Keep entries short.

## State directory

`{{STATE_DIR}}/` is the per-run scratchpad. Findings go under
`{{STATE_DIR}}/findings/` — the `review-dedup` hat promotes surviving
findings into `docs/review-playbook/runner/findings/` at the end of the
pipeline. You may use `{{STATE_DIR}}/` for any intermediate notes you
want.
