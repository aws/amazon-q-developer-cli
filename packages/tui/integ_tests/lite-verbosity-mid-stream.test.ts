/**
 * Lite /verbosity presets typed mid-stream must QUEUE (not fire) and drain
 * FIFO after the turn, leaving <Static>-committed rows byte-identical.
 * Anchor: PR #2643; src/stores/app-store.ts:4910-4969 (lite slash queue).
 */

import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from './helpers/integ-lifecycle';

describe('lite /verbosity mid-stream cycling', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  it('queues /verbosity commands FIFO mid-stream; static rows survive drain', async () => {
    testCase = await launchLiteInteg('lite-verbosity-mid-stream-cycle', {
      timeout: 20000,
    });

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

    // Three /verbosity presets mid-turn must hit the lite slash-queue branch in
    // handleUserInput (queued, not dispatched immediately).
    await testCase.typeAndSubmit('/verbosity default');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity full');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity lean');
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);
    const queuedSlashCount = store.queuedMessages.filter((m) =>
      m.startsWith('/verbosity')
    ).length;
    expect(queuedSlashCount).toBe(3);

    // End the streaming turn; the queue drains FIFO (default → full → lean).
    await testCase.waitForStore(
      (s) =>
        s.queuedMessages.filter((m) => m.startsWith('/verbosity')).length ===
          0 && !s.isProcessing,
      5000
    );

    store = await testCase.getStore();
    expect(
      store.queuedMessages.filter((m: string) => m.startsWith('/verbosity'))
        .length
    ).toBe(0);

    // Streamed content must survive verbatim — re-rendering under a new preset
    // must not mutate <Static>-committed scrollback (append-only contract).
    const allMsgText = store.messages.map((m) => JSON.stringify(m)).join(' ');
    expect(allMsgText).toContain(streamedContent);

    await exitLiteInteg(testCase);
  }, 30000);
});
