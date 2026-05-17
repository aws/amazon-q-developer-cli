# Review 20 — Static item integrity and message ordering

**Why this class matters.** The TUI's static item pipeline — how completed conversation turns move from dynamic React rendering to terminal scrollback — has a fragile cursor/ID tracking system that has broken 4+ times in a single month (Apr–May 2026). Symptoms: new messages silently disappear, conversation turns render out of order, or content vanishes after terminal resize. These are silent data-loss bugs — the user gets no error, their conversation just becomes garbled or incomplete.

**Scope.** The static item lifecycle: `ConversationView.tsx` (emits completed turns to static), `trim-static-items.ts` (caps array length), twinki's `static-output.ts` (manages the monotonic cursor and `replaceStaticOutput`), and the `emittedIds` set that prevents re-emission. Also covers the incremental flush path where long turns are split into tail messages.

For concrete starting points: `trimStaticItems`, `adjustStaticCursor`, `emittedIds`, `appendStatic`, `resetStatic`, `replaceStaticOutput`, and the `totalStaticWritten` cursor in twinki.

## Techniques

1. **[code] Cursor invariant audit.** After every call to `trimStaticItems` (which splices from the front of the array), verify that `adjustStaticCursor(n)` is called to keep twinki's monotonic `totalStaticWritten` in sync. If the cursor exceeds the array length, `slice(cursor)` returns empty and new items are silently skipped.

2. **[code] Tombstone integrity.** Verify that `emittedIds` retains IDs of trimmed items. If trimmed IDs are deleted from `emittedIds`, the `completedTurns.forEach` loop can re-append them at the END of the static array, shuffling order.

3. **[code] Incremental flush completeness.** When a long turn completes and is flushed incrementally (tail messages appended as individual static items), verify the turn summary is also appended. Prior bug: summary disappeared because only tail messages were emitted.

4. **[blackbox] Multi-turn ordering probe.** Via Knight Rider, drive 12+ conversation turns with unique markers. Scroll through all scrollback and verify every marker is present and in correct sequence. This catches both swallowing (missing markers) and garbling (out-of-order markers).

5. **[blackbox] Resize survival test.** After driving multiple turns, resize the terminal and verify no turns disappear or reorder. This catches the `resetStatic` / `replaceStaticOutput` reflow path where items can be lost if the cursor is desynced.

6. **[blackbox] High-turn-count stress.** Drive 50+ turns (exceeding the 200 static item cap at ~4 items/turn) and verify the most recent turns remain visible while old turns are correctly trimmed without affecting new ones.

## What to record

For each finding: which invariant broke (cursor desync, tombstone deletion, missing flush), the file and line, the reproduction scenario (number of turns, resize timing), and Knight Rider frame evidence showing the visual symptom.

## Done criteria

The cursor invariant holds after every trim operation. `emittedIds` never deletes trimmed IDs. The incremental flush path emits summaries. The Knight Rider probe passes with all markers present and in order, including after resize.
