/**
 * Queued-message editing via ↑/↓ + Esc (lite). ↑ from an empty prompt walks
 * back through queued messages (Claude Code parity) and shows the
 * "▸ editing queued #<n>" header; re-submit writes back to the SAME slot via
 * replaceQueuedMessage (FIFO preserved, no append); Esc abandons the edit and
 * leaves the queue byte-for-byte intact.
 *
 * Anchor: PR #2643 ("Editing-queue header") + PromptInput.tsx ↑/↓ queue-restore.
 */

import { describe, expect, it } from 'bun:test';
import {
  launchLiteInteg,
  trackCleanup,
} from './helpers/integ-lifecycle';
import { TestCase } from '../src/test-utils/TestCase';

// TEMPORARILY SKIPPED: mid-turn message queueing in lite is known to be
// not-ideal UX on this branch — these tests assert the final queueing behavior
// that lands with the core logic branch later. Re-enable (drop `.skip`) in the
// PR that ports the finished queueing logic into lite.
describe.skip('lite queued message editing', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  /**
   * KIRO_TEST_MOCK_TURN_TIMEOUT_MS=20000 keeps the turn alive past the 2s mock
   * auto-resolve so queued messages don't drain mid-test. These tests rely on
   * afterEach's force-kill for cleanup: the Ctrl+C ladder can't cleanly exit
   * while multiple queued messages are in flight (each Ctrl+C interrupts a turn
   * rather than closing).
   */
  function launchQueueEditCase(name: string): Promise<TestCase> {
    return launchLiteInteg(name, {
      env: { KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' },
      timeout: 20000,
    });
  }

  /**
   * Boots lite, types a real message to start a turn (so isProcessing=true),
   * then queues two further chat messages. Returns the queued texts.
   */
  async function setupQueueWithTwoEntries(
    tc: TestCase
  ): Promise<{ first: string; second: string }> {
    // Trigger an in-flight turn so subsequent submits queue.
    await tc.typeAndSubmit('start the turn');
    await tc.sleepMs(200);

    const first = 'queue_first_message';
    const second = 'queue_second_message';
    await tc.typeAndSubmit(first);
    await tc.sleepMs(150);
    await tc.typeAndSubmit(second);
    await tc.sleepMs(150);

    const store = await tc.getStore();
    expect(store.isProcessing).toBe(true);
    expect(store.queuedMessages).toEqual([first, second]);
    return { first, second };
  }

  // All three share: launch, queue two entries, press ↑ from the empty prompt
  // (pulls the LAST queued slot back, editingQueueIndex===1). They differ only
  // in what happens after the pull. Each `after` callback receives the queued
  // texts captured at setup.
  it.each([
    {
      label: 'pulling via ↑ shows the editing header and loads the text',
      testName: 'lite-queue-edit-pull-shows-header',
      after: async (
        tc: TestCase,
        _queued: { first: string; second: string }
      ) => {
        const store = await tc.getStore();
        expect(store.commandInputValue).toBe('queue_second_message');
        await tc.waitForVisibleText('editing queued #2', 3000);
      },
    },
    {
      label: 'edit + Enter writes back to the same slot (FIFO, no append)',
      testName: 'lite-queue-edit-resubmit',
      after: async (
        tc: TestCase,
        queued: { first: string; second: string }
      ) => {
        let store = await tc.getStore();
        expect(store.queuedMessages.length).toBe(2);

        await tc.sendKeys('\x15'); // Ctrl+U: kill-line back-to-start
        await tc.sleepMs(100);
        await tc.typeAndSubmit('queue_second_edited');
        await tc.sleepMs(250);

        store = await tc.getStore();
        // Replace, not append: length stays 2 and the edit stays at index 1.
        expect(store.queuedMessages.length).toBe(2);
        expect(store.queuedMessages[0]).toBe(queued.first);
        expect(store.queuedMessages[1]).toBe('queue_second_edited');
        expect(store.editingQueueIndex).toBeNull();
      },
    },
    {
      label: 'cancelling the edit (Esc) leaves the queue intact',
      testName: 'lite-queue-edit-cancel',
      after: async (
        tc: TestCase,
        queued: { first: string; second: string }
      ) => {
        const queueBefore = (await tc.getStore()).queuedMessages.slice();
        // Esc clears editingQueueIndex without writing back
        // (PromptInput.tsx:828 queueRestoreRef Esc handler).
        await tc.pressEscape();
        await tc.sleepMs(200);

        const store = await tc.getStore();
        expect(store.editingQueueIndex).toBeNull();
        // Queue must be byte-for-byte unchanged.
        expect(store.queuedMessages).toEqual(queueBefore);
        expect(store.queuedMessages).toEqual([queued.first, queued.second]);
      },
    },
  ])(
    '$label',
    async ({ testName, after }) => {
      testCase = await launchQueueEditCase(testName);

      const queued = await setupQueueWithTwoEntries(testCase);

      // Press ↑ from empty prompt — pulls the LAST queued ('second') back.
      await testCase.sendKeys('\x1b[A');
      await testCase.sleepMs(200);

      const store = await testCase.getStore();
      expect(store.editingQueueIndex).toBe(1);

      await after(testCase, queued);
    },
    30000
  );
});
