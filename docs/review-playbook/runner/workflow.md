# Blackbox probe workflow

Supporting notes for [`.github/workflows/review-playbook.yml`](../../../.github/workflows/review-playbook.yml).

## Why manual trigger only

- Blackbox probes can be expensive (per-platform matrix job × several minutes).
- They are exploratory by design; running them on every push creates noise.
- A manual trigger makes it obvious who asked for the probe, with which inputs, against which commit.

If we ever want scheduled runs later, add a `schedule:` block — but start with `workflow_dispatch` only.

## Inputs

| input | required | default | notes |
|-------|----------|---------|-------|
| `probe` | yes | `example-smoke` | Filename of the probe script under `packages/tui/scripts/probes/`, without `.ts`. |
| `platforms` | no | `linux,macos,windows` | Comma-separated. Allowed values: `linux`, `macos`, `windows`. |
| `build` | no | `tui+rust` | What to build before running the probe. See below. |
| `extra-args` | no | `` | Passed verbatim to `bun run <probe>.ts <args>`. |
| `timeout-minutes` | no | `45` | Per-OS timeout. |

## Build modes

The `build` input controls how much of the stack gets compiled before the probe runs. Mirrors the `tui-e2e` job in `tui.yml` where relevant.

| mode | installs | builds | typical probe | approx cold-build time |
|------|----------|--------|---------------|------------------------|
| `none` | bun + dependencies | nothing | plumbing smoke tests, pure-function probes, ad-hoc scripts that do not spawn the TUI | ~30s |
| `tui` | bun + dependencies + protobuf | twinki + TUI bundle (`packages/tui/dist/tui.js`) | probes that run the TUI binary but do not talk to the Rust backend (markdown fuzz, yoga edge-case fuzz, layout tests) | 1–3 min |
| `tui+rust` | everything for `tui` + Rust toolchain + cargo cache + `chat_cli` | everything for `tui` plus `chat_cli` | any probe that uses `E2ETestCase` or otherwise spawns the full `kiro-cli chat` binary | 5–15 min cold; 2–4 min warm (cargo cache hits) |

Default is `tui+rust` so probes see the same binaries the E2E suite uses. Set explicitly to `none` for cheap probes where the build would just be wasted time.

## What the probe actually runs against

The probe runs under:

- **Source commit**: whatever the workflow was dispatched against (`${{ github.sha }}`). Recorded as `KIRO_PROBE_COMMIT`.
- **Ref**: the branch / tag the workflow ran against. Recorded as `KIRO_PROBE_REF`.
- **Bun version**: whatever `oven-sh/setup-bun@v2` installs by default (currently the latest stable). Recorded as `KIRO_PROBE_BUN_VERSION`. This is **not** the embedded version pinned in `scripts/const.py` — the probe sees the runner's bun, not the user's. Keep that in mind when comparing probe findings against user reports; some bugs only reproduce under specific bun versions (see `docs/bun-performance-analysis.md`).
- **chat_cli binary**: only built when `build=tui+rust`. Built in debug mode (`cargo build -p chat_cli`) from the checked-out source.
- **TUI bundle**: only built when `build` is `tui` or `tui+rust`. Built with `NODE_ENV=production` via `packages/tui/package.json`'s `build` script.

Every finding includes these values in its frontmatter and in the `.meta` artifact so traceability is automatic.

## Outputs

For each selected platform, an artifact named `probe-<probe>-<platform>` containing the contents of `$PROBE_OUTPUT_DIR`. Artifacts are retained for 14 days.

Contents:

- Zero or more finding files: `<probe>-<platform>-<YYYYMMDD>-<HHMM>-<slug>.md`.
- A completion marker: `<probe>-<platform>-<YYYYMMDD>-<HHMM>-done.md` (or `-continuation.md` if budget was exhausted, or `-runbook.md` if deferred).
- Machine metrics: `<probe>-<platform>-<YYYYMMDD>-<HHMM>-metrics.json`.
- Raw stdout/stderr: `<platform>.log`.
- A meta record: `<platform>.meta` with `status`, `runner`, `platform`, `probe`, `build`, `ref`, `sha`, `bun`.

The workflow summary page lists each platform in its own section, shows finding counts and titles, and embeds the `sync-findings.ts` command with the run id pre-filled.

## End-to-end flow (Option A: download artifacts, sync, review, commit)

The workflow has `permissions: contents: read` only. It does not push to the repo, does not open pull requests, does not modify anything. Findings are uploaded as workflow artifacts and pulled into the repo by a local sync helper when you want them.

### Step 1 — Trigger

```bash
gh workflow run review-playbook.yml \
  --repo <owner>/<repo> \
  --ref <branch> \
  -f probe=example-smoke \
  -f platforms=linux,macos,windows
```

Or click "Run workflow" in the Actions UI.

### Step 2 — Watch

```bash
gh run watch --repo <owner>/<repo>
```

Each matrix leg writes a `$GITHUB_STEP_SUMMARY` block showing:

- Findings count per platform.
- Titles of any findings emitted.
- Link to the per-platform artifact name.

Visible on the run's Summary page in the Actions UI — you can tell at a glance whether the run turned anything up without downloading.

### Step 3 — Sync findings into the repo

The sync helper at `packages/tui/scripts/probes/sync-findings.ts` downloads the run's artifacts and copies finding files, done markers, metrics, and error logs into `docs/review-playbook/runner/findings/`.

```bash
# Most recent run
bun run packages/tui/scripts/probes/sync-findings.ts --latest

# A specific run id
bun run packages/tui/scripts/probes/sync-findings.ts --run-id 1234567890

# Inspect without touching anything
bun run packages/tui/scripts/probes/sync-findings.ts --latest --dry-run
```

The helper is idempotent — files that already exist in `findings/` are skipped. It does not commit anything.

### Step 4 — Review and commit

Look at the new files, decide which to keep, then commit:

```bash
git status docs/review-playbook/runner/findings/
git add docs/review-playbook/runner/findings/
git commit -m "findings: probe <name> on <platforms> — <short description>"
```

If a finding is a duplicate of one already in `findings/`, either delete the new file or set `status: duplicate` with `duplicate-of: <existing-id>` in its frontmatter.

### Testing the plumbing

Before writing a real probe, verify the workflow works end-to-end:

```bash
gh workflow run review-playbook.yml \
  -f probe=example-smoke \
  -f platforms=linux,macos,windows
gh run watch
bun run packages/tui/scripts/probes/sync-findings.ts --latest --dry-run
```

Each platform should produce (exact filenames include a UTC timestamp):

- `probe-example-smoke-<platform>/example-smoke-<platform>-<YYYYMMDD>-<HHMM>-done.md`
- `probe-example-smoke-<platform>/example-smoke-<platform>-<YYYYMMDD>-<HHMM>-metrics.json`
- Zero or more finding files named `example-smoke-<platform>-<YYYYMMDD>-<HHMM>-<slug>.md` (the example probe emits none).
- `probe-example-smoke-<platform>.log`
- `probe-example-smoke-<platform>.meta`

## Authoring a new probe

1. Copy `packages/tui/scripts/probes/example-smoke.ts` to a new name.
2. Update the top-doc and the `PROBE_NAME` constant.
3. Implement the check. Exit `0` on invariant-held, `1` on finding, `2` on probe crash.
4. Write to `$PROBE_OUTPUT_DIR` using `<PROBE_NAME>-<PLATFORM>.*` filenames.
5. Test locally: `bun run packages/tui/scripts/probes/<name>.ts`.
6. Dispatch via the workflow on your fork to verify cross-OS behaviour.

## Failure semantics

- **Exit 0** — probe ran, invariant held. Workflow job is green.
- **Exit 1** — probe detected a finding. Workflow job is red. Artifacts still upload (the `if: always()` on the upload step).
- **Exit 2** (or any other non-zero) — probe itself crashed. Workflow job is red with the crash logged to `<PROBE_NAME>-<PLATFORM>.error.log`.

`fail-fast: false` is set on the matrix so a failure on one OS does not cancel the others.

## Security notes

- The workflow has `permissions: contents: read` only.
- It runs arbitrary code from `packages/tui/scripts/probes/`, so a malicious PR adding a probe and triggering the workflow could exfiltrate secrets — but the workflow accepts no secrets, has no write permissions, and is `workflow_dispatch` only. On forks it runs under the fork's quota.
- Do not add secrets to this workflow without reviewing the threat model.
