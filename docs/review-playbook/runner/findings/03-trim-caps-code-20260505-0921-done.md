---
id: 03-trim-caps-code-20260505-0921-done
work-item: 03-trim-caps-code
review: 03-unbounded-growth
findings-emitted: 1
status: done
completed-at: 2026-05-05T09:21:00Z
---

# Done: 03-trim-caps-code

## Techniques applied

- **Technique 1 (Long-lived container census):** All containers in scope are bounded. `command-history` uses `slice(-1000)`, `inputMetrics` uses `slice(-1000)`, `feed-state` is disk-persisted and bounded by announcement count. No unbounded containers found.
- **Technique 4 (Trim-cursor invariant audit):** `trim-static-items.ts` documents cursor invariant correctly. `command-history` and `inputMetrics` use `slice` with no cursor dependency. No invariant violations.
- **Technique 6 (Config-driven caps):** Three caps lack env-var overrides (`MAX_HISTORY_SIZE`, `maxSamples`, `MAX_TAIL_SIZE`). One finding emitted.

## Files reviewed

- `packages/tui/src/utils/trim-static-items.ts` — bounded, env-var override present
- `packages/tui/src/utils/input-editing.ts` — pure functions, no long-lived state
- `packages/tui/src/utils/command-history.ts` — bounded, no env-var override
- `packages/tui/src/utils/reverse-search.ts` — per-session state machine, no growth
- `packages/tui/src/utils/feed-state.ts` — disk-persisted, bounded by feed content
- `packages/tui/src/utils/turn-flush-machine.ts` — pure function, no env-var override
- `packages/tui/src/utils/logger.ts` — direct file write, no buffer
- `packages/tui/src/utils/inputMetrics.ts` — bounded, no env-var override
- `packages/tui/src/utils/trust-gate-state.ts` — simple boolean, no containers
- `packages/tui/src/constants/settings.ts` — setting key declarations only

## Notes

- The work item scope mentions "kill ring" and "undo stack" but these do not exist in `input-editing.ts` (which contains only pure editing functions). They may live elsewhere outside this work item's scope.
