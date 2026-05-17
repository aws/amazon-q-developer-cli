# Review runner

This directory turns the review playbook (../*.md) into **work items** that sub-agents can execute one at a time, then runs each finding through a dedup + validate gate before rolling up a human-readable report. The goal is context-budget discipline: instead of one agent trying to hold the whole codebase plus the whole playbook in its head, a **planner** breaks the work up into right-sized pieces, workers execute them independently, and later hats consolidate the output.

## How it works

Five instruction files describe the five stages of the pipeline:

1. [`orchestrator.md`](orchestrator.md) — instructions for the **planner**. Reads a directive ("review the last 7 days", "pre-Windows-release check"), inspects the repo, and emits one file per work item under `{{STATE_DIR}}/work/` (the autoloop per-run scratchpad).
2. [`work-item.template.md`](work-item.template.md) — the shape every work item and every finding must conform to. Also documents the finding lifecycle: `open → resolved | duplicate | wontfix | needs-human-check`.
3. `presets/review-code/harness.md` + `presets/review-blackbox/harness.md` — instructions for the **worker** hats. Each consumes one work item and emits per-finding files to `{{STATE_DIR}}/findings/`.
4. `presets/review-dedup/harness.md` — instructions for the **dedup hat**. Batches worker-produced findings across scratchpads, compares them against the committed tree, and applies SKIP / ENRICH / COPY per candidate. This is the only stage that writes to committed `findings/`.
5. `presets/review-validate/harness.md` — instructions for the **validate hat**. Re-checks `status: open` findings against current source and transitions them to `resolved` or `needs-human-check` as appropriate.
6. [`rollup.md`](rollup.md) — instructions for the **rollup hat**. Reads finding files from committed `findings/` (never source), groups by severity / review / file, and writes `_rollup.md`.

Every hat is LLM instructions — markdown, not code — so the runner is tool-agnostic. You can drive it with [autoloop](https://mikeyobrien.github.io/autoloop/) using the bundled presets, with Ralph using the same instruction files as hats, with a bash `for` loop that pastes each work item into a fresh chat, or by hand.

## Presets

Five ready-to-run autoloop presets live under `presets/`, each a self-contained directory with its own `autoloops.toml`, `topology.toml`, `harness.md`, and `roles/` folder:

| Preset | Directory | Purpose |
|--------|-----------|---------|
| `review-planner` | [presets/review-planner/](presets/review-planner/) | Turns a directive into work-item files under `{{STATE_DIR}}/work/`. Single role, one-shot. |
| `review-code` | [presets/review-code/](presets/review-code/) | Executes one `[code]` work item, emits findings to `{{STATE_DIR}}/findings/`. Single role, one-shot. |
| `review-blackbox` | [presets/review-blackbox/](presets/review-blackbox/) | Executes one `[blackbox]` work item (runs probes or tests), emits findings to `{{STATE_DIR}}/findings/`. Single role, one-shot. |
| `review-dedup` | [presets/review-dedup/](presets/review-dedup/) | Reads worker scratchpads, promotes surviving findings to committed `findings/`. SKIP / ENRICH / COPY decisions logged to `_dedup-log.md`. Single role, one-shot. |
| `review-validate` | [presets/review-validate/](presets/review-validate/) | Re-checks `status: open` findings against current source. Transitions to `resolved` or `needs-human-check`. Summary at `_validation.md`. Single role, one-shot. |

All five use the **kiro** ACP backend (`-b kiro` is the default). They set `review.enabled = false` and `event_loop.required_events = []` because the one-shot scan pattern does not need the metareview gate or quality-gate events — `task.complete` alone signals the end.

Run a preset directly with:

```bash
# 1. Plan. Work items land in the autoloop scratchpad — not the repo tree.
autoloop run docs/review-playbook/runner/presets/review-planner \
  -b kiro "$(cat directive.md)"

# 2. Execute each work item. Findings land in each run's own scratchpad.
autoloop run docs/review-playbook/runner/presets/review-code \
  -b kiro "$(cat .autoloop/runs/<planner-run-id>/work/04-entry-lifecycle-code.md)"

autoloop run docs/review-playbook/runner/presets/review-blackbox \
  -b kiro "$(cat .autoloop/runs/<planner-run-id>/work/04-ssh-disconnect-blackbox.md)"

# 3. Dedup. Pulls findings from worker scratchpads into committed findings/.
autoloop run docs/review-playbook/runner/presets/review-dedup -b kiro "auto"

# 4. (Optional) Validate. Marks resolved/needs-human-check in place.
autoloop run docs/review-playbook/runner/presets/review-validate -b kiro "all"

# 5. Rollup. Read-only aggregation — no dedicated preset; invoke the
#    rollup.md instructions from whichever driver you prefer (Ralph hat,
#    kiro-cli subagent, or `kiro-cli chat` with rollup.md pasted in).
```

Each preset is ~150 lines total across four files. Extend them by authoring new roles/prompts; do not fork the bundled autoloop presets. If you need to override behaviour at the workflow level, use autoloop's [profile feature](https://mikeyobrien.github.io/autoloop/features/profiles.html) rather than editing the preset files.

## Directories

```
runner/
  README.md                   you are here
  orchestrator.md             planner instructions (stage 1)
  work-item.template.md       work-item + finding shape; finding lifecycle
  rollup.md                   rollup instructions (stage 5)
  scripts/
    find-dedup-candidates.sh  structural pre-filter used by review-dedup
  work/                       archive / non-autoloop runners write here;
                              autoloop pipeline uses {{STATE_DIR}}/work/
  findings/                   committed findings — only the review-dedup
                              hat writes here (workers write to
                              {{STATE_DIR}}/findings/ and dedup promotes)
    01-hooks-code-20260503-0935-settimeout-in-resize-handler.md
    01-hooks-code-20260503-0942-done.md
    ...
    _dedup-log.md             dedup hat output (regenerated per run)
    _validation.md            validate hat output (regenerated per run)
    _rollup.md                rollup hat output (regenerated per run)
  presets/
    review-planner/
    review-code/
    review-blackbox/
    review-dedup/
    review-validate/
```

### Work items are per-run, findings are durable

**Work items** are ephemeral per-run artifacts. They live in the autoloop per-run scratchpad at `{{STATE_DIR}}/work/` (which resolves to `.autoloop/runs/<run-id>/work/`) and stay there. They are not promoted, copied, or committed by the pipeline. Concurrent planner runs each get their own scratchpad and do not collide.

Downstream `review-code` and `review-blackbox` workers receive their work item as the objective prompt; a driver reads it out of the planner's scratchpad. If you want durable work-item archives, write directly to `docs/review-playbook/runner/work/` from a Ralph or manual workflow — but the autoloop pipeline neither requires nor produces that.

**Findings** are the durable output. Workers write them to `{{STATE_DIR}}/findings/` during execution. The `review-dedup` hat then reads from worker scratchpads and promotes surviving findings to committed `docs/review-playbook/runner/findings/`, applying SKIP / ENRICH / COPY per candidate. Dedup is the only stage that writes to the committed findings tree.

### Finding lifecycle

Every finding carries a `status` field:

- `open` — active finding. Workers always emit `open`.
- `resolved` — validated as fixed. Set by `review-validate` when evidence no longer holds in the current source. Includes `resolved-at` (ISO-8601 UTC) and `resolved-at-commit` (git HEAD).
- `duplicate` — merged into another finding by `review-dedup`. The duplicate is either dropped entirely (default) or tombstoned with `duplicate-of: <original-id>` for audit. The original stays `open` and gets a back-reference in its `## Related` section.
- `wontfix` — manually marked as intentional or accepted. Human-set only; no automated hat writes `wontfix`.
- `needs-human-check` — validate could not determine autonomously whether the finding is resolved (file moved, evidence is descriptive prose, refactor beyond recognition). Surfaces in the rollup for triage.

See `work-item.template.md` for the full schema including companion fields (`duplicate-of`, `corroborated-by`, `resolved-at`, `resolved-at-commit`).

### Why one file per finding?

- Each finding has its own frontmatter, status, and life cycle. A finding can be opened, resolved, marked duplicate, wontfix, or flagged needs-human-check without churning a big shared file.
- `ls findings/01-*` immediately lists everything from review 1.
- `grep -l "severity: crash" findings/*.md` enumerates all crash-class findings.
- Every finding is a reviewable unit — easy to link from a PR, easy to cross-reference.
- The rollup (`_rollup.md`) becomes a pure read-only aggregation of the finding files.

## Triggers

The runner supports any of:

- **By hand** — "run the planner with directive X", paste the resulting work items into kiro-cli one at a time.
- **Ralph** — the three stages map onto three hats; see [the Ralph pattern below](#ralph-pattern).
- **CI hook** — a GitHub Action can run the planner on every PR and post the suggested scope as a comment. Execution stays opt-in.
- **Cron** — run the full playbook once a quarter by scheduling the planner with `directive: full`.
- **On user-reported crash** — run the planner with `directive: post-crash <stack-trace>`.

## Partitioning dimensions

The planner considers these axes when breaking work up. Which axes apply depends on the review and the directive.

### For code reviews

- **Feature / domain** — rendering, stores, ACP client, IPC, markdown, input handling.
- **Directory / package** — one work item per directory at depth 2, collapsing tiny dirs into the parent, splitting oversized ones.
- **Recency** — files changed since a cutoff (commit, date, PR).
- **Pattern** — all `use*.ts` hooks, all `*.tsx` components, all files matching a glob.
- **File size** — a single oversized file gets its own work item.

### For blackbox reviews

- **Platform** — `linux`, `macos`, `windows`, `headless-ci`.
- **Harness** — `unit`, `integration`, `e2e`, `knight-rider`, `cpu-prof`, `heap-snapshot`, `ad-hoc`, `ci-matrix`.
- **Duration** — `fast` (seconds), `medium` (minutes), `long` (hours — 4-hour soaks get runbooks, not inline tasks).
- **Prerequisites** — needs real PTY? needs network? needs a specific terminal emulator?

**Critical rule the planner enforces: never mix `[code]` and `[blackbox]` techniques into a single work item.** They need different budgets, different runtimes, different reviewers. Every work item is one or the other.

## Cross-platform blackbox

The playbook calls for running many blackbox probes on all three OSes. In practice:

- **Linux** — can be run locally on any host via Docker, or on CI directly.
- **macOS** — can be run locally on a macOS host, or on CI (`macos-latest` / `macos-latest-xlarge`).
- **Windows** — practically can only be run on a real Windows machine or CI. Wine is unreliable for a real TUI; nested virt is fragile.

To bridge the gap, this repo has a **manually-triggered GitHub Action** that runs any probe on any combination of OSes:

- File: [`.github/workflows/review-playbook.yml`](../../../.github/workflows/review-playbook.yml)
- Trigger: `workflow_dispatch` only (no scheduled or PR runs).
- Inputs: probe script name, platforms (comma-separated: `linux,macos,windows`), optional extra args.
- Execution: runs `packages/tui/scripts/probes/<name>.ts` on each selected OS.
- Output: uploads `/tmp/probe-output/` as artifacts per-OS and shows a findings summary in `$GITHUB_STEP_SUMMARY`.

The workflow is documented in [workflow.md](workflow.md).

**Pulling findings back into the repo.** The workflow has `contents: read` only — it uploads artifacts but does not push. A local helper at `packages/tui/scripts/probes/sync-findings.ts` downloads the artifacts and copies finding files into `findings/`:

```
bun run packages/tui/scripts/probes/sync-findings.ts --latest
```

The helper is idempotent and does not commit — the human (or orchestrator) reviews the new files and commits the ones worth keeping.

A planner-generated blackbox work item therefore has three execution targets:

- `harness: ad-hoc`, `platform: native` — run on the local machine directly.
- `harness: ad-hoc`, `platform: docker-linux` — run in a Docker container (for Linux from non-Linux).
- `harness: ci-matrix`, `platform: <os>` — dispatch to the `review-playbook.yml` workflow.

The planner picks the cheapest viable target given the probe's prerequisites.

## Probe scripts

Probes live under `packages/tui/scripts/probes/`. See the README there for the expected shape. A probe is a bun script that:

1. Runs for a bounded wall-clock time.
2. Emits structured output (JSON or CSV) under `/tmp/probe-output/`.
3. Exits 0 on "no finding", non-zero on "finding detected".

The workflow treats any non-zero exit as a failure worth attaching artifacts for.

## Ralph pattern

If you drive the runner with Ralph, a minimum viable `ralph.yml` looks like:

```yaml
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 200

hats:
  planner:
    name: Planner
    triggers: [work.start]
    default_publishes: plan.done
    instructions_file: docs/review-playbook/runner/orchestrator.md

  worker:
    name: Reviewer
    triggers: [plan.done, item.next]
    default_publishes: item.next       # keeps looping until no items left
    instructions_file: docs/review-playbook/runner/work-item-executor.md  # see note

  deduplicator:
    name: Deduplicator
    triggers: [items.all-done]
    default_publishes: dedup.done
    instructions_file: docs/review-playbook/runner/presets/review-dedup/harness.md

  validator:
    name: Validator
    triggers: [dedup.done]
    default_publishes: validate.done
    instructions_file: docs/review-playbook/runner/presets/review-validate/harness.md

  rollup:
    name: Rollup
    triggers: [validate.done]
    default_publishes: LOOP_COMPLETE
    instructions_file: docs/review-playbook/runner/rollup.md
```

The `worker` hat's instructions tell it to read the next unfinished item from `work/_manifest.md`, execute it, write findings to `findings/` (or `{{STATE_DIR}}/findings/` if the dedup hat follows in the loop), and publish `items.all-done` when the manifest is exhausted (otherwise `item.next`). The instruction file for that loop is small — a paragraph — and can be inlined into `orchestrator.md` as a sub-section rather than a separate file.

If you don't want the dedup or validate stages, wire `rollup` to trigger directly on `items.all-done` and skip the intermediate hats — the pipeline degrades gracefully.

## Budget discipline

- **Target: each work item fits in ~60k input tokens of context.** The planner is responsible for splitting items that would exceed this.
- **One file per finding, written immediately.** The worker writes a new finding file the moment it identifies a finding, not at the end of the work item. This avoids "keep all findings in context" failure.
- **Never re-read source across techniques.** A work item lists all files it needs in scope; the worker reads each file once and runs all applicable techniques against it.
- **Technique-level early exit.** If `[code]` technique 1 finds zero matches in scope, the worker can skip subsequent techniques that share the same source-level anchor.
- **No source in rollup.** The rollup stage reads only `findings/*.md`. If it needs code context, it links to the original finding, which links to the source.