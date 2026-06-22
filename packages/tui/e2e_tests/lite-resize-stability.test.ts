/**
 * E2E test: Lite mode resize does NOT trigger scrollback redraw.
 *
 * Validates bug-mine entry 1.7:
 *   "Text is baked at flush-time width. DO NOT add a resize-driven redraw —
 *    it would either be silently dropped or scramble historical rows."
 *
 * Key invariant:
 *   - Old rows: unchanged byte-for-byte in terminal buffer after resize
 *   - New rows: rendered at new width
 *   - No re-emission of old rows (twinki would silently drop them)
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import type { PtyManager } from '../src/test-utils/shared/pty-manager';
import { streamReply } from './lite/helpers/responses';

/**
 * Access the private ptyManager to call resize().
 * E2ETestCase doesn't expose resize() publicly, but we need it
 * to actually change PTY dimensions (not just send SIGWINCH).
 */
function resizePty(testCase: E2ETestCase, cols: number, rows: number): void {
  const mgr = (testCase as unknown as { ptyManager: PtyManager }).ptyManager;
  mgr.resize(cols, rows);
}

/**
 * Extracts lines containing a marker from the snapshot.
 * Returns the right-trimmed text content of those lines.
 * (xterm pads lines to terminal width with spaces, so we rtrim for comparison.)
 */
function extractMarkerLines(snapshot: string[], marker: string): string[] {
  return snapshot
    .filter((line) => line.includes(marker))
    .map((l) => l.trimEnd());
}

describe('lite resize stability [bug-mine 1.7]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('old static rows stay byte-for-byte intact after terminal resize', async () => {
    // Start at 80 columns
    testCase = await E2ETestCase.builder()
      .withTestName('lite-resize-stability')
      .withTerminal({ width: 80, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    const sendTurn = async (prompt: string, marker: string) => {
      await streamReply(testCase!, marker);
      await testCase!.sendKeys(prompt);
      await testCase!.sleepMs(100);
      await testCase!.pressEnter();
      await testCase!.waitForText(marker, 15000);
      await testCase!.waitForIdle(10000);
    };

    await sendTurn('first', 'TURN_1_MARKER_ALPHA');
    await sendTurn('second', 'TURN_2_MARKER_BRAVO');
    await sendTurn('third', 'TURN_3_MARKER_CHARLIE');

    // --- Capture pre-resize snapshot ---
    const snapBefore = testCase.getSnapshot();
    const marker1Before = extractMarkerLines(snapBefore, 'TURN_1_MARKER_ALPHA');
    const marker2Before = extractMarkerLines(snapBefore, 'TURN_2_MARKER_BRAVO');
    const marker3Before = extractMarkerLines(
      snapBefore,
      'TURN_3_MARKER_CHARLIE'
    );

    expect(marker1Before.length).toBe(1);
    expect(marker2Before.length).toBe(1);
    expect(marker3Before.length).toBe(1);

    // --- Resize wider: 80 -> 120 columns ---
    resizePty(testCase, 120, 40);
    await testCase.sleepMs(200);

    const snapAfterWide = testCase.getSnapshot();
    const marker1AfterWide = extractMarkerLines(
      snapAfterWide,
      'TURN_1_MARKER_ALPHA'
    );
    const marker2AfterWide = extractMarkerLines(
      snapAfterWide,
      'TURN_2_MARKER_BRAVO'
    );
    const marker3AfterWide = extractMarkerLines(
      snapAfterWide,
      'TURN_3_MARKER_CHARLIE'
    );

    // Markers must still be present (no re-emission that could scramble them)
    expect(marker1AfterWide.length).toBe(1);
    expect(marker2AfterWide.length).toBe(1);
    expect(marker3AfterWide.length).toBe(1);

    // Content of marker lines must be identical (byte-for-byte text preserved)
    expect(marker1AfterWide[0]).toBe(marker1Before[0]);
    expect(marker2AfterWide[0]).toBe(marker2Before[0]);
    expect(marker3AfterWide[0]).toBe(marker3Before[0]);

    // --- Send a new message at 120 cols — must render correctly ---
    await sendTurn('wide', 'POST_RESIZE_WIDE_DELTA');

    const snapAfterNewMsg = testCase.getSnapshot();
    expect(
      snapAfterNewMsg.some((l) => l.includes('POST_RESIZE_WIDE_DELTA'))
    ).toBe(true);

    // Old markers still intact after new message at new width
    expect(extractMarkerLines(snapAfterNewMsg, 'TURN_1_MARKER_ALPHA')[0]).toBe(
      marker1Before[0]
    );
    expect(extractMarkerLines(snapAfterNewMsg, 'TURN_2_MARKER_BRAVO')[0]).toBe(
      marker2Before[0]
    );
    expect(
      extractMarkerLines(snapAfterNewMsg, 'TURN_3_MARKER_CHARLIE')[0]
    ).toBe(marker3Before[0]);

    // --- Resize narrower: 120 -> 60 columns ---
    resizePty(testCase, 60, 40);
    await testCase.sleepMs(200);

    const snapAfterNarrow = testCase.getSnapshot();
    const allTextNarrow = snapAfterNarrow.join('\n');

    // All markers still present in the buffer
    expect(allTextNarrow).toContain('TURN_1_MARKER_ALPHA');
    expect(allTextNarrow).toContain('TURN_2_MARKER_BRAVO');
    expect(allTextNarrow).toContain('TURN_3_MARKER_CHARLIE');
    expect(allTextNarrow).toContain('POST_RESIZE_WIDE_DELTA');

    // No duplicates after narrow resize (TUI did not re-emit)
    const narrowMarker1 = extractMarkerLines(
      snapAfterNarrow,
      'TURN_1_MARKER_ALPHA'
    );
    const narrowMarker2 = extractMarkerLines(
      snapAfterNarrow,
      'TURN_2_MARKER_BRAVO'
    );
    const narrowMarker3 = extractMarkerLines(
      snapAfterNarrow,
      'TURN_3_MARKER_CHARLIE'
    );
    const narrowMarkerWide = extractMarkerLines(
      snapAfterNarrow,
      'POST_RESIZE_WIDE_DELTA'
    );
    expect(narrowMarker1.length).toBe(1);
    expect(narrowMarker2.length).toBe(1);
    expect(narrowMarker3.length).toBe(1);
    expect(narrowMarkerWide.length).toBe(1);

    // --- Send another message at 60 cols — must render ---
    await sendTurn('narrow', 'POST_RESIZE_NARROW_ECHO');

    const snapFinal = testCase.getSnapshot();
    console.log('Final snapshot:\n' + testCase.getSnapshotFormatted());

    // Final message rendered
    expect(snapFinal.some((l) => l.includes('POST_RESIZE_NARROW_ECHO'))).toBe(
      true
    );

    // All original markers intact — no re-emission, no scrambling
    const finalAllText = snapFinal.join('\n');
    expect(finalAllText).toContain('TURN_1_MARKER_ALPHA');
    expect(finalAllText).toContain('TURN_2_MARKER_BRAVO');
    expect(finalAllText).toContain('TURN_3_MARKER_CHARLIE');
    expect(finalAllText).toContain('POST_RESIZE_WIDE_DELTA');
    expect(finalAllText).toContain('POST_RESIZE_NARROW_ECHO');

    // Still exactly one occurrence of each marker (no duplicates from re-emission)
    expect(extractMarkerLines(snapFinal, 'TURN_1_MARKER_ALPHA').length).toBe(1);
    expect(extractMarkerLines(snapFinal, 'TURN_2_MARKER_BRAVO').length).toBe(1);
    expect(extractMarkerLines(snapFinal, 'TURN_3_MARKER_CHARLIE').length).toBe(
      1
    );
    expect(extractMarkerLines(snapFinal, 'POST_RESIZE_WIDE_DELTA').length).toBe(
      1
    );
    expect(
      extractMarkerLines(snapFinal, 'POST_RESIZE_NARROW_ECHO').length
    ).toBe(1);
  }, 90000);
});
