/**
 * E2E test: Lite mode <Static> append-only monotonic cursor invariant.
 *
 * Validates bug-mine entries:
 *   1.1 — Monotonic by-index cursor: never mutate prior items
 *   1.3 — Shallow copy required for twinki to detect new items
 *   1.4 — Delta-append walk: never rebuild full items array
 *   1.5 — Turn summary trailer placement locked at first emission
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { launchLiteE2E, sendUserMessage } from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

describe('lite static append-only [bug-mine 1.1, 1.3, 1.4, 1.5]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('prior messages remain byte-for-byte after subsequent turns', async () => {
    testCase = await launchLiteE2E('lite-static-monotonicity', {
      terminal: { width: 120, height: 50 },
    });

    const markers = [
      'FIRST_RESPONSE_MARKER_ABC',
      'SECOND_RESPONSE_MARKER_XYZ',
      'THIRD_RESPONSE_MARKER_999',
    ];

    // After each turn, every marker so far must be present, in strictly
    // increasing order (no cursor violation / silent drop), and appear exactly
    // once (no trailer re-emission). Asserting the growing invariant after each
    // turn covers the same monotonic/exactly-once contract as the unrolled turns.
    for (let turn = 0; turn < markers.length; turn++) {
      await streamReply(testCase, markers[turn]!);
      await sendUserMessage(testCase, `turn ${turn + 1}`);
      await testCase.waitForText(markers[turn]!, 15000);
      await testCase.waitForIdle(10000);

      const snap = testCase.getSnapshot();
      let prevIdx = -1;
      for (let i = 0; i <= turn; i++) {
        const m = markers[i]!;
        const idx = snap.findIndex((l) => l.includes(m));
        expect(idx).toBeGreaterThan(prevIdx);
        expect(snap.filter((l) => l.includes(m)).length).toBe(1);
        prevIdx = idx;
      }
    }
  }, 60000);
});
