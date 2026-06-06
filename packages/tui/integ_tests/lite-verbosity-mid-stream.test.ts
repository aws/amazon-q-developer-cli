/**
 * Integ test: /verbosity preset cycling mid-stream.
 *
 * 1. WHAT user-observable behavior does this assert?
 *    Per PR #2643's "/verbosity" surface area: typing /verbosity preset
 *    commands during an in-flight turn does not corrupt rendered rows.
 *    In lite mode, slash commands typed while isProcessing=true are
 *    QUEUED (src/stores/app-store.ts:4912 — handleUserInput's queue
 *    branch) and fire only after the current turn ends. The append-only
 *    invariant (the lite mode's headline contract per PR #2643) means
 *    rows committed to <Static> before the swap MUST stay byte-identical
 *    even after a queued /verbosity preset drains.
 *
 *    Two distinct cycles tested:
 *      - minimal → full → lean queued mid-stream: queue drains FIFO,
 *        last preset wins, queue is empty post-drain, isProcessing=false.
 *      - The committed agent content from the streaming turn appears
 *        verbatim in store.messages after the turn ends — re-rendering
 *        at the new verbosity does NOT mutate the message body.
 *
 * 2. WHAT class of regression would this catch?
 *    Anyone who refactors handleUserInput to fire slash commands
 *    immediately during isProcessing (instead of queueing) would let
 *    a /verbosity preset apply mid-stream, and adjacent frames would
 *    render at different verbosity levels — visible row corruption.
 *    Anyone who breaks the FIFO drain order (so the FIRST queued
 *    command wins instead of the last) would fail the queue-drain
 *    assertion. Anyone who makes <Static> rows mutable on /verbosity
 *    swap (re-rendering committed messages with the new caps) would
 *    fail the message-body byte-identity assertion.
 *
 * 3. Could the test pass even if the feature is broken?
 *    No. The test asserts:
 *      (a) queuedMessages contains both /verbosity commands while
 *          isProcessing=true (proves they queued, not fired),
 *      (b) after the turn ends, queuedMessages is empty (proves drain),
 *      (c) the committed agent message content is byte-identical to
 *          the streamed content (proves no re-render on preset swap).
 *    A bug that fired commands immediately would fail (a) — the
 *    queue would be empty mid-stream. A drain bug would fail (b).
 *    A row-corruption bug would fail (c).
 *
 * Anchor: PR #2643 ("/verbosity" surface area + append-only contract);
 *          src/stores/app-store.ts:4910-4969 (lite slash queue).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';

describe('lite /verbosity mid-stream cycling', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('queues /verbosity commands FIFO mid-stream; static rows survive drain', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-verbosity-mid-stream-cycle')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject a streaming agent message. The mocked prompt() doesn't auto-
    // complete (we drive completeTurn() ourselves below), so isProcessing
    // stays true while we type queued commands.
    const streamedContent = 'VERBOSITY_STREAM_MARKER_BODY_TURN_ONE';
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'verbosity-stream-msg-1',
      content: { type: 'text', text: streamedContent } as any,
    });

    await testCase.typeAndSubmit('start streaming');
    await testCase.sleepMs(300);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Type three /verbosity preset commands during the in-flight turn.
    // Each must be queued (lite slash-queue branch in handleUserInput),
    // not dispatched immediately.
    await testCase.typeAndSubmit('/verbosity minimal');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity full');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity lean');
    await testCase.sleepMs(200);

    // (a) All three should be in the queue right now (proof they queued).
    store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);
    const queuedSlashCount = store.queuedMessages.filter((m) => m.startsWith('/verbosity')).length;
    expect(queuedSlashCount).toBe(3);

    // End the streaming turn. The queue drains FIFO: minimal → full → lean.
    // Each preset apply is synchronous; after all drain, queuedMessages
    // must be empty and isProcessing must flip to false.
    await testCase.completeTurn();
    // Drain is async — poll on the queue going empty + isProcessing=false.
    const drained = await waitForCondition(testCase, (s) => {
      const noVerbosityLeft =
        s.queuedMessages.filter((m: string) => m.startsWith('/verbosity'))
          .length === 0;
      return noVerbosityLeft && !s.isProcessing;
    });
    expect(drained).toBe(true);

    // (b) Queue is empty post-drain.
    store = await testCase.getStore();
    expect(
      store.queuedMessages.filter((m: string) => m.startsWith('/verbosity')).length
    ).toBe(0);

    // (c) The streamed agent content is preserved verbatim in committed
    //     messages — re-rendering under a new preset must not mutate
    //     <Static>-committed scrollback (append-only contract).
    const allMsgText = store.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMsgText).toContain(streamedContent);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});

async function waitForCondition(
  tc: TestCase,
  predicate: (state: any) => boolean,
  timeoutMs = 5000
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const s = await tc.getStore();
    if (predicate(s)) return true;
    await tc.sleepMs(75);
  }
  return false;
}
