# Probe scripts

Bun-executable scripts that run a single blackbox review technique and emit findings. Dispatched manually or by the `.github/workflows/blackbox-probe.yml` workflow, and driven locally by the review playbook runner.

## Shape

Every probe:

1. **Runs for a bounded wall-clock time.** Prefer < 5 min for `duration: fast`, < 30 min for `duration: medium`. Anything longer belongs in a runbook, not a probe.
2. **Writes one file per finding** to `$PROBE_OUTPUT_DIR` (the workflow sets this; default when run locally is `./probe-output/`). File naming:

   ```
   <probe>-<platform>-<YYYYMMDD>-<HHMM>-<slug>.md
   ```

   Each finding file has YAML frontmatter (`id`, `severity`, `file`, `line`, `class`, `platforms-affected`, `discovered-by`, `discovered-at`, `status`) plus a short body with description, evidence, and proposed fix.

3. **Writes a done marker** at `<probe>-<platform>-<YYYYMMDD>-<HHMM>-done.md` when the probe completes, containing a summary and the full metrics. The rollup stage uses this to detect probes that did not finish.

4. **Writes metrics** as `<probe>-<platform>-<YYYYMMDD>-<HHMM>-metrics.json` for machine parsing.

5. **Exits** `0` on no-finding, `1` on finding-detected, `2` on probe crash. Any non-zero exit causes the CI job to fail loudly.

6. **Prints a short human-readable summary** to stdout. Full structured output goes to files.

## Conventions

- Filename: `<topic>-<what-it-probes>.ts`, kebab-case. Examples: `resize-storm.ts`, `ssh-disconnect.ts`, `yoga-zero-width.ts`, `clipboard-roundtrip.ts`.
- Script reads `KIRO_PROBE_PLATFORM` (set by the workflow) or detects with `process.platform`.
- Respect `process.env.CI === '1'` and avoid anything that needs a real TTY if `CI` is set (use the `headless-ci` path instead).
- Write each finding as its own file using the schema in `docs/review-playbook/runner/work-item.template.md`.
- Copy any evidence (profiles, snapshots, screenshots) into `$PROBE_OUTPUT_DIR` so the workflow uploads them as artifacts.
- Reference evidence files by relative path in the finding's `Evidence` section.

## Running locally

```
bun run packages/tui/scripts/probes/example-smoke.ts
```

Outputs to `./probe-output/` by default. Override with `PROBE_OUTPUT_DIR=/tmp/...`.

## Running via CI

Trigger the workflow manually:

```
gh workflow run blackbox-probe.yml \
  -f probe=example-smoke \
  -f platforms=linux,macos,windows \
  -f build=tui+rust \
  -f timeout-minutes=45
```

The `build` input selects what the runner compiles before launching the probe:

- `none` — no build; plumbing smoke tests or pure-TS probes.
- `tui` — builds twinki + the TUI bundle; use for probes that run `bun run dist/tui.js`.
- `tui+rust` — builds everything above plus `chat_cli` via `cargo build`. Matches what `tui-e2e` in `tui.yml` does. Use for probes that spawn the full `kiro-cli chat` binary or use `E2ETestCase`.

The workflow uploads per-platform artifacts and shows a per-platform findings summary directly in the run's Actions page. The summary includes the commit SHA and bun version the probe ran under.

## Pulling findings back into the repo

After a workflow run, use the sync helper to download artifacts and copy finding files into `docs/review-playbook/runner/findings/`:

```
# Most recent run
bun run packages/tui/scripts/probes/sync-findings.ts --latest

# A specific run id
bun run packages/tui/scripts/probes/sync-findings.ts --run-id <id>

# Dry run (no writes)
bun run packages/tui/scripts/probes/sync-findings.ts --latest --dry-run

# Test against a local artifact dir (no gh CLI needed)
bun run packages/tui/scripts/probes/sync-findings.ts --from-dir /path/to/artifacts
```

The helper is idempotent (already-present files are skipped) and does not commit. Review the new files, then commit the ones worth keeping.

## Template

See `example-smoke.ts` in this directory for the minimum viable probe. Copy it when starting a new probe.
