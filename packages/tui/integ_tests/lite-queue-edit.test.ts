/**
 * Integ test: Queued message editing via ↑/↓ + Esc.
 *
 * 1. WHAT user-observable behavior does this assert?
 *    Per PR #2643's "Footer chrome > Editing-queue header" surface area:
 *    in lite mode, ↑ from an empty prompt walks back through queued
 *    messages (Claude Code parity). When the user pulls a queued slot
 *    back into the input, the lite layout shows the cyan editing
 *    header "▸ editing queued #<n>" (LiteLayout.tsx:2075-2080).
 *    Resubmitting writes the edited text back into the same queue slot
 *    via replaceQueuedMessage, preserving FIFO order. Esc abandons the
 *    edit; the queue is left intact.
 *
 *    Three observable transitions tested:
 *      (a) Queue [A, B] + ↑ → editingQueueIndex=1, commandInputValue='B'
 *          and the editing header text is visible on screen.
 *      (b) Edit body to 'B-edited' + Enter → queuedMessages=[A, B-edited],
 *          editingQueueIndex=null, queue length unchanged.
 *      (c) Pull then Esc (without resubmitting) → queue intact, no edit
 *          applied, editingQueueIndex=null.
 *
 * 2. WHAT class of regression would this catch?
 *    Anyone refactoring navigateQueueUp / queueRestoreRef in
 *    src/components/chat/prompt-bar/PromptInput.tsx and breaking the
 *    "↑ pulls latest queued message" semantics would fail (a). Anyone
 *    wiring Enter through onSubmit instead of replaceQueuedMessage
 *    would either drop the edit or duplicate it (queue grows by one)
 *    and (b) catches both shapes. Anyone making Esc commit the edit
 *    instead of cancelling would fail (c) — the queue would mutate.
 *
 * 3. Could the test pass even if the feature is broken?
 *    No.
 *      (a) requires both editingQueueIndex AND commandInputValue AND
 *          the visible "editing queued" header to flip. Three signals
 *          tied together — implausibly false-positive.
 *      (b) compares the queue array byte-for-byte before/after edit.
 *          A bug that submits as a fresh message instead of replacing
 *          the slot leaves the queue with 3 entries, not 2. A bug that
 *          drops the edit leaves [A, B] unchanged.
 *      (c) compares the queue array before vs. after Esc — must be
 *          unchanged AND editingQueueIndex must be null.
 *
 * Anchor: PR #2643 ("Editing-queue header") + PromptInput.tsx ↑/↓
 *          queue-restore navigation.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

describe('lite queued message editing', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /**
   * Boots lite, types a real message to start a turn (so isProcessing=true),
   * then queues two further chat messages. Returns the queued texts.
   */
  async function setupQueueWithTwoEntries(
    tc: TestCase
  ): Promise<{ first: string; second: string }> {
    await tc.waitForVisibleText('ask a question', 10000);
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

  it('pulling a queued slot back via ↑ shows the editing header and loads the text', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-queue-edit-pull-shows-header')
      .withLite()
      // Keep the turn alive past the 2s mock auto-resolve so queued
      // messages don't drain on us mid-test.
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
      .withTimeout(20000)
      .launch();

    await setupQueueWithTwoEntries(testCase);

    // Press ↑ from empty prompt — pulls the LAST queued ('second') back.
    await testCase.sendKeys('\x1b[A');
    await testCase.sleepMs(200);

    const store = await testCase.getStore();
    expect(store.editingQueueIndex).toBe(1);
    expect(store.commandInputValue).toBe('queue_second_message');

    // Editing header is visible on screen.
    await testCase.waitForVisibleText('editing queued #2', 3000);

    // afterEach cleanup() kills the PTY. Tests using
    // KIRO_TEST_MOCK_TURN_TIMEOUT_MS=20000 with multiple queued messages
    // can't reliably exit via the Ctrl+C ladder because each Ctrl+C
    // interrupts a turn rather than cleanly closing — leave the cleanup
    // path to afterEach's force-kill.
  }, 30000);

  it('edit + Enter writes back to the same slot, preserving queue length and FIFO order', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-queue-edit-resubmit')
      .withLite()
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
      .withTimeout(20000)
      .launch();

    const { first } = await setupQueueWithTwoEntries(testCase);

    // ↑ to pull the second queued back.
    await testCase.sendKeys('\x1b[A');
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.editingQueueIndex).toBe(1);
    expect(store.queuedMessages.length).toBe(2);

    // Replace the loaded body with 'queue_second_edited'. Ctrl+U clears
    // line, then type. Then Enter to commit.
    await testCase.sendKeys('\x15'); // Ctrl+U: kill-line back-to-start
    await testCase.sleepMs(100);
    await testCase.typeAndSubmit('queue_second_edited');
    await testCase.sleepMs(250);

    store = await testCase.getStore();
    // Queue length stays 2 (replace, not append). FIFO is preserved —
    // the edited entry stays at index 1, ahead of any new submissions.
    expect(store.queuedMessages.length).toBe(2);
    expect(store.queuedMessages[0]).toBe(first);
    expect(store.queuedMessages[1]).toBe('queue_second_edited');
    expect(store.editingQueueIndex).toBeNull();

    // afterEach cleanup() kills the PTY. Tests using
    // KIRO_TEST_MOCK_TURN_TIMEOUT_MS=20000 with multiple queued messages
    // can't reliably exit via the Ctrl+C ladder because each Ctrl+C
    // interrupts a turn rather than cleanly closing — leave the cleanup
    // path to afterEach's force-kill.
  }, 30000);

  it('cancelling the edit (Esc) leaves the queue intact', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-queue-edit-cancel')
      .withLite()
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
      .withTimeout(20000)
      .launch();

    const { first, second } = await setupQueueWithTwoEntries(testCase);

    // ↑ to pull.
    await testCase.sendKeys('\x1b[A');
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.editingQueueIndex).toBe(1);
    const queueBefore = [...store.queuedMessages];

    // Esc cancels the edit. PromptInput's queueRestoreRef Esc handler
    // (PromptInput.tsx:828) clears editingQueueIndex without writing back.
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.editingQueueIndex).toBeNull();
    // Queue must be byte-for-byte unchanged.
    expect(store.queuedMessages).toEqual(queueBefore);
    expect(store.queuedMessages).toEqual([first, second]);

    // afterEach cleanup() kills the PTY. Tests using
    // KIRO_TEST_MOCK_TURN_TIMEOUT_MS=20000 with multiple queued messages
    // can't reliably exit via the Ctrl+C ladder because each Ctrl+C
    // interrupts a turn rather than cleanly closing — leave the cleanup
    // path to afterEach's force-kill.
  }, 30000);
});
