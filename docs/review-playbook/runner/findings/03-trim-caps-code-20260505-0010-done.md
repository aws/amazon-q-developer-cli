---
findings-emitted: 3
work-item: 03-trim-caps-code
review: 03-unbounded-growth
completed-at: 2026-05-05T00:10:00Z
---

# Done — 03-trim-caps-code

## Summary

Applied techniques 4 and 6 across `packages/tui/src/**` and `packages/twinki/packages/twinki/src/**`.

### Technique 4 (Trim-cursor invariant audit)

Grepped for `splice`, `slice(`, `shift(`, `pop(` across all stores and utilities.
Identified 10 trim sites. Two findings:

1. **KillRing** (`kill-ring.ts`) — no size cap, grows unbounded.
2. **UndoStack** (`undo-stack.ts`) — no size cap, grows unbounded with structuredClone snapshots.

All other trim sites have correct invariants:
- `trim-static-items.ts` — reference pattern, adjustStaticCursor called correctly.
- `PromptInput.tsx` undoStack — capped at 100, shift from front, no cursor index.
- `session-conversations.ts` — capped at 50, no downstream cursor.
- `command-history.ts` — capped at 1000, currentIndex reset on add.
- `Editor.ts` history — capped at 100, pop from end, historyIndex reset on add.
- `inputMetrics.ts` — capped at maxSamples=1000, no cursor.
- `commands/effects.ts` — pure function returning new slice, no cursor.

### Technique 6 (Config-driven caps)

Grepped for `MAX_`, `LIMIT_`, `_CAP`, `_SIZE` constants and `process.env.KIRO_` reads.
One finding covering four caps:

- `MAX_SESSION_MESSAGES=50` — no env-var override.
- `MAX_HISTORY_SIZE=1000` — no env-var override.
- `MAX_EXPANDED_LINES=1000` — no env-var override.
- `MAX_TAIL_SIZE=6` — no env-var override.

Only `MAX_STATIC_ITEMS` follows the env-var override pattern (`KIRO_MAX_STATIC_ITEMS`).

UI-only display caps (`MAX_VISIBLE_LINES`, `MAX_TOOL_COL`, `MAX_PARAMS`, `MAX_DISPLAY_TURNS`)
were excluded as they don't affect memory growth.
