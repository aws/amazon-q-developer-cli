# Cross-cutting techniques and run order

## Techniques that apply to every review

### Profiling tools

- `bun --cpu-prof` — CPU flame graphs. See `docs/bun-performance-analysis.md` for the workflow.
- `bun --heap-snapshot` — heap snapshots before/after an interaction.
- `bun test --coverage` — ensures the review actually exercises the code.
- `process.memoryUsage()` inside test loops — RSS / heapUsed / external.
- `performance.now()` inside hot-path probes — per-call cost.

### Static analysis

- `tsc --noEmit` — must pass; strict null checks catch a surprising number of edge-case bugs.
- ESLint with `no-floating-promises`, `no-misused-promises`, `require-await` — required.
- `bunx knip` (if configured) — dead code and unused dependencies.
- `rg` is the default search tool; prefer it to `grep -r`.

### Runtime probes

- Always run probes with `NODE_ENV=production` bundling, because dev bundles have extra assertions that mask issues.
- Run probes against the embedded bun, not the dev bun. Embedded bun lags; bugs surface there first.
- Always run with `TWINKI_DEBUG_REDRAW=1` during manual exploration to surface render counts.

### Scope hygiene

- Skip generated files (`packages/tui/src/types/generated/`).
- Skip `node_modules/` unless specifically auditing a new dependency (Review 8).
- Skip anything under `docs/`, `autodocs/`, `scripts/` unless the script runs at user runtime.

### Recording findings

A CSV or Markdown table works. Columns:

| review | class | file:line | severity | description | technique | proposed fix | follow-up | test added? |
|--------|-------|-----------|----------|-------------|-----------|--------------|-----------|-------------|

Severity uses the rubric in the main README. "Follow-up" is a commit hash or PR number where the fix or test landed.

## Suggested run order

A reviewer or subagent running this playbook for the first time should start with Reviews 4, 5, and 12, because their findings are usually cheap to fix and high-severity (crashes, spirals from dead TTYs, stack overflows on adversarial input, silent exception swallows). Reviews 2 and 11 are the richest but need the most expertise. Reviews 6, 10, and 13 are the broadest and benefit from being run after the others have reduced the noise floor. Reviews 1, 3, 7, 8, 9 can be run in any order. Review 10 (cross-platform) should be run before any release that targets Windows.

| Order | Review | Typical effort | Typical findings |
|-------|--------|----------------|------------------|
| 1     | 4. Dead FDs / broken streams | Half a day | 1–3 |
| 2     | 5. Recursion and stack | Half a day | 2–5 |
| 3     | 12. Exception handling | 1 day | 5–15 |
| 4     | 1. Async in resize/render | Half a day | 1–3 |
| 5     | 2. Yoga edge cases | 1–2 days | 3–10 |
| 6     | 3. Unbounded growth | 1–2 days | 2–5 |
| 7     | 11. Type / schema drift | 1–2 days | 5–15 |
| 8     | 7. Concurrency | 1 day | 2–5 |
| 9     | 10. Cross-platform | 1–2 days | 5–10 |
| 10    | 9. Terminal edge cases | 1 day | 3–8 |
| 11    | 8. Dependency footprint | Per-upgrade | Varies |
| 12    | 6. Hot-path allocation | 2–3 days | 5–15 |
| 13    | 13. Test coverage | 1–2 days | Many |

Run the playbook in full at least once per quarter. See the main [README](README.md) for common triggers that call for running a subset.
