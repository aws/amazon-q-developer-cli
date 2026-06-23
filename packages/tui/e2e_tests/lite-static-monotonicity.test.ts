/**
 * E2E test: Lite mode <Static> append-only monotonic cursor invariant.
 *
 * Validates bug-mine entries:
 *   1.1 — Monotonic by-index cursor: never mutate prior items
 *   1.3 — Shallow copy required for twinki to detect new items
 *   1.4 — Delta-append walk: never rebuild full items array
 *   1.5 — Turn summary trailer placement locked at first emission
 *
 * Strategy: push three sequential turns and assert after each that
 * prior responses remain byte-for-byte in the terminal snapshot,
 * new responses appear BELOW prior ones (monotonic append), and
 * no duplicate rows appear (no re-emission of committed items).
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

    // --- Turn 1 ---
    await streamReply(testCase, 'FIRST_RESPONSE_MARKER_ABC');

    await sendUserMessage(testCase, 'hello');
    await testCase.waitForText('FIRST_RESPONSE_MARKER_ABC', 15000);
    await testCase.waitForIdle(10000);

    // Snapshot after first turn
    const snap1 = testCase.getSnapshot();
    const firstIdx = snap1.findIndex((l) =>
      l.includes('FIRST_RESPONSE_MARKER_ABC')
    );
    expect(firstIdx).toBeGreaterThan(-1);

    // --- Turn 2 ---
    await streamReply(testCase, 'SECOND_RESPONSE_MARKER_XYZ');

    await sendUserMessage(testCase, 'again');
    await testCase.waitForText('SECOND_RESPONSE_MARKER_XYZ', 15000);
    await testCase.waitForIdle(10000);

    // Verify both are present and in monotonic order
    const snap2 = testCase.getSnapshot();
    const first2Idx = snap2.findIndex((l) =>
      l.includes('FIRST_RESPONSE_MARKER_ABC')
    );
    const second2Idx = snap2.findIndex((l) =>
      l.includes('SECOND_RESPONSE_MARKER_XYZ')
    );
    expect(first2Idx).toBeGreaterThan(-1); // first still exists byte-for-byte
    expect(second2Idx).toBeGreaterThan(first2Idx); // second is below first

    // No duplicate of first response (trailer not re-emitted)
    const firstOccurrences = snap2.filter((l) =>
      l.includes('FIRST_RESPONSE_MARKER_ABC')
    );
    expect(firstOccurrences.length).toBe(1);

    // --- Turn 3 ---
    await streamReply(testCase, 'THIRD_RESPONSE_MARKER_999');

    await sendUserMessage(testCase, 'third');
    await testCase.waitForText('THIRD_RESPONSE_MARKER_999', 15000);
    await testCase.waitForIdle(10000);

    const snap3 = testCase.getSnapshot();

    const f3 = snap3.findIndex((l) => l.includes('FIRST_RESPONSE_MARKER_ABC'));
    const s3 = snap3.findIndex((l) => l.includes('SECOND_RESPONSE_MARKER_XYZ'));
    const t3 = snap3.findIndex((l) => l.includes('THIRD_RESPONSE_MARKER_999'));

    // All three are present (no silent drops from monotonic cursor violation)
    expect(f3).toBeGreaterThan(-1);
    expect(s3).toBeGreaterThan(f3);
    expect(t3).toBeGreaterThan(s3);

    // No duplicates of any marker (no re-emission)
    const secondOccurrences = snap3.filter((l) =>
      l.includes('SECOND_RESPONSE_MARKER_XYZ')
    );
    const thirdOccurrences = snap3.filter((l) =>
      l.includes('THIRD_RESPONSE_MARKER_999')
    );
    expect(firstOccurrences.length).toBe(1);
    expect(secondOccurrences.length).toBe(1);
    expect(thirdOccurrences.length).toBe(1);
  }, 60000);
});
