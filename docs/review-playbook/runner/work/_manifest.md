# Work-item manifest — review:03 (unbounded growth)

Generated: 2026-05-05T00:51Z
Updated: 2026-05-07 (blackbox un-defer: probe scripts now exist for techniques 7–10)

## Prior completions (not re-emitted)

| ID | Techniques | Status |
|----|-----------|--------|
| 03-trim-caps-code | 4, 6 | done (3 findings) |
| 03-session-heap-blackbox | 7, 8 | superseded — split into `03-session-lifetime-blackbox` (t7) and `03-heap-snapshot-blackbox` (t8) now that probe scripts exist |

## Work items this run

| ID | Kind | Techniques | Scope summary | LOC | Rationale |
|----|------|-----------|---------------|-----|-----------|
| 03-store-growth-code | code | 1, 3, 5 | Zustand store, selectors, session-conversations, message-stream-handler, feed-state, trim helpers | ~3970 | Largest state surface; messages/toolCalls/sessions grow per-turn |
| 03-acp-lifecycle-code | code | 1, 2, 5 | ACP client, kiro.ts, graceful-exit, turn-flush-machine, notification | ~1950 | EventEmitter-heavy; Maps keyed by session/request; subscription lifecycle |
| 03-input-utils-code | code | 1, 2, 3 | input-editing, command-history, reverse-search, path-completion, file-search, shell-escape, sessions | ~2400 | History buffers, completion caches, per-keystroke accumulators |
| 03-hooks-render-code | code | 1, 2, 5 | All hooks + logger, terminal-capabilities, serialize-conversation | ~900 | Subscription cleanup on unmount; potential WeakMap for refs |
| 03-session-lifetime-blackbox | blackbox | 7 | `packages/tui/scripts/probes/session-lifetime.ts` | — | 60-min RSS soak with linear-regression trend (≥1 MB/min = finding) |
| 03-heap-snapshot-blackbox | blackbox | 8 | `packages/tui/scripts/probes/heap-snapshot.ts` | — | Pre/post heap diff by constructor via test-mode IPC |
| 03-subscription-stress-blackbox | blackbox | 9 | `packages/tui/scripts/probes/subscription-stress.ts` | — | 100-cycle spawn/kill stress; probe-process RSS/FD drift |
| 03-forced-trim-blackbox | blackbox | 10 | `packages/tui/scripts/probes/forced-trim.ts` | — | 250 turns past `MAX_SESSION_MESSAGES=50`; RSS-slope ratio |

## Partitioning rationale

- **Feature-based split**: Store state (messages, sessions, tool calls) vs ACP lifecycle (event emitters, request maps) vs input utilities (undo, history, completion) vs hooks (subscription cleanup).
- **Budget compliance**: Largest item is ~3970 LOC, well under the 15k LOC / 60k token cap.
- **Technique separation**: Code techniques 1,2,3,5 distributed by relevance. Technique 2 (subscriptions) goes where EventEmitters live (ACP, hooks). Technique 3 (stream buffers) goes where streaming data accumulates (store, input).
- **Blackbox items un-deferred**: Probe scripts now exist for techniques 7, 8, 9, 10 under `packages/tui/scripts/probes/`. Each blackbox item is now `harness: ad-hoc` with `probe: <path>` set and `defer: null`. The un-defer rule from `rollup.md` §"How to implement a deferred runbook" was applied: probe filenames match partition labels (`session-lifetime.ts`, `heap-snapshot.ts`, `subscription-stress.ts`, `forced-trim.ts`).
- **Technique 8 work-item split**: The prior `03-session-heap-blackbox` bundled techniques 7 and 8 together because both were deferred. With probe scripts for each, they are now separate work items so they can run independently and emit findings against different techniques.
- **Prior work excluded**: Techniques 4, 6 already completed in earlier run `03-trim-caps-code`.
