# Done — review:03 planner run (native-light)

Generated: 2026-05-05T00:51Z
Updated: 2026-05-07 (un-defer: techniques 7–10 promoted from runbook to executable probes)

## Summary

Emitted **8 work items** for review 03 (unbounded growth in long-running TUI state):
- 4 code work items covering techniques 1, 2, 3, 5
- 4 blackbox work items (all executable, none `defer: runbook`) covering techniques 7, 8, 9, 10

## What was skipped

- **Techniques 4, 6** — already completed in prior run `03-trim-caps-code` (3 findings emitted).

## What changed in this update

Prior runs emitted `03-session-heap-blackbox` with `defer: runbook` (techniques 7 + 8 bundled) and `03-subscription-stress-blackbox` + `03-forced-trim-blackbox` also deferred. Probe scripts now exist:

| Work item | Probe |
|-----------|-------|
| `03-session-lifetime-blackbox` (t7) | `packages/tui/scripts/probes/session-lifetime.ts` |
| `03-heap-snapshot-blackbox` (t8) | `packages/tui/scripts/probes/heap-snapshot.ts` |
| `03-subscription-stress-blackbox` (t9) | `packages/tui/scripts/probes/subscription-stress.ts` |
| `03-forced-trim-blackbox` (t10) | `packages/tui/scripts/probes/forced-trim.ts` |

All four are now `harness: ad-hoc`, `defer: null`. The old combined `03-session-heap-blackbox` item is superseded and not re-emitted.

## Execution order (suggested)

1. `03-store-growth-code` — no dependencies, largest scope
2. `03-acp-lifecycle-code` — no dependencies, parallel with #1
3. `03-input-utils-code` — no dependencies, parallel with #1
4. `03-hooks-render-code` — no dependencies, parallel with #1
5. `03-heap-snapshot-blackbox` — depends on #1; fastest blackbox (~10 min)
6. `03-forced-trim-blackbox` — depends on #1; ~2–3 min
7. `03-subscription-stress-blackbox` — depends on #2; ~2–3 min
8. `03-session-lifetime-blackbox` — depends on #1; longest (~60 min)

All code items are independent and can run in parallel. Blackbox items
can run in parallel with each other (they each launch their own PTY)
but contend for CPU and RSS — on a laptop, serialize the long-running
`03-session-lifetime-blackbox` after the others.
