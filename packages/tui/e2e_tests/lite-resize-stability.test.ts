/**
 * Lite resize must NOT trigger a scrollback redraw [bug-mine 1.7]: text is
 * baked at flush-time width, so old rows must stay byte-for-byte intact after a
 * resize (a resize-driven redraw would be dropped or scramble historical rows).
 */

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import type { PtyManager } from '../src/test-utils/shared/pty-manager';
import { launchLiteE2E } from './lite/helpers/commands';
import { driveTurn } from './lite/helpers/responses';

/** Resize via the private ptyManager — E2ETestCase doesn't expose resize() (and we need a real dimension change, not just SIGWINCH). */
function resizePty(testCase: E2ETestCase, cols: number, rows: number): void {
  const mgr = (testCase as unknown as { ptyManager: PtyManager }).ptyManager;
  mgr.resize(cols, rows);
}

/** Marker lines from the snapshot, rtrimmed (xterm pads to terminal width). */
function extractMarkerLines(snapshot: string[], marker: string): string[] {
  return snapshot
    .filter((line) => line.includes(marker))
    .map((l) => l.trimEnd());
}

describe('lite resize stability [bug-mine 1.7]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it('old static rows stay byte-for-byte intact after terminal resize', async () => {
    testCase = await launchLiteE2E('lite-resize-stability', {
      terminal: { width: 80, height: 40 },
    });

    const markers = [
      'TURN_1_MARKER_ALPHA',
      'TURN_2_MARKER_BRAVO',
      'TURN_3_MARKER_CHARLIE',
    ];
    await driveTurn(testCase, markers[0]!, 'first');
    await driveTurn(testCase, markers[1]!, 'second');
    await driveTurn(testCase, markers[2]!, 'third');

    // Right-trimmed line content for each marker in a snapshot.
    const markerLine = (snap: string[], m: string) =>
      extractMarkerLines(snap, m)[0];

    // --- Capture pre-resize snapshot ---
    const snapBefore = testCase.getSnapshot();
    const before: Record<string, string | undefined> = {};
    for (const m of markers) {
      expect(extractMarkerLines(snapBefore, m).length).toBe(1);
      before[m] = markerLine(snapBefore, m);
    }

    // --- Resize wider: 80 -> 120 columns ---
    resizePty(testCase, 120, 40);
    await testCase.sleepMs(200);

    // No re-emission that scrambles old rows; line content byte-for-byte.
    const snapAfterWide = testCase.getSnapshot();
    for (const m of markers) {
      expect(extractMarkerLines(snapAfterWide, m).length).toBe(1);
      expect(markerLine(snapAfterWide, m)).toBe(before[m]);
    }

    // --- Send a new message at 120 cols — must render correctly ---
    await driveTurn(testCase, 'POST_RESIZE_WIDE_DELTA', 'wide');

    const snapAfterNewMsg = testCase.getSnapshot();
    expect(
      snapAfterNewMsg.some((l) => l.includes('POST_RESIZE_WIDE_DELTA'))
    ).toBe(true);
    for (const m of markers) {
      expect(markerLine(snapAfterNewMsg, m)).toBe(before[m]);
    }

    // --- Resize narrower: 120 -> 60 columns ---
    resizePty(testCase, 60, 40);
    await testCase.sleepMs(200);

    // All markers present, exactly once (TUI did not re-emit).
    const snapAfterNarrow = testCase.getSnapshot();
    const allTextNarrow = snapAfterNarrow.join('\n');
    for (const m of [...markers, 'POST_RESIZE_WIDE_DELTA']) {
      expect(allTextNarrow).toContain(m);
      expect(extractMarkerLines(snapAfterNarrow, m).length).toBe(1);
    }

    // --- Send another message at 60 cols — must render ---
    await driveTurn(testCase, 'POST_RESIZE_NARROW_ECHO', 'narrow');

    const snapFinal = testCase.getSnapshot();
    expect(snapFinal.some((l) => l.includes('POST_RESIZE_NARROW_ECHO'))).toBe(
      true
    );

    // All markers intact, exactly once — no re-emission, no scrambling.
    const finalAllText = snapFinal.join('\n');
    for (const m of [
      ...markers,
      'POST_RESIZE_WIDE_DELTA',
      'POST_RESIZE_NARROW_ECHO',
    ]) {
      expect(finalAllText).toContain(m);
      expect(extractMarkerLines(snapFinal, m).length).toBe(1);
    }
  }, 90000);
});
