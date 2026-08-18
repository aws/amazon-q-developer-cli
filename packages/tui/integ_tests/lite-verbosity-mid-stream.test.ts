/**
 * Lite /verbosity presets typed mid-stream execute immediately (they are
 * classified as turn-inert) and leave <Static>-committed rows byte-identical.
 * Anchor: PR #2643; src/stores/app-store.ts (lite slash queue);
 * Updated for PR #4020: /verbosity is now TURN_INERT, dispatched immediately.
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

  it('dispatches /verbosity immediately mid-stream (inert); static rows survive', async () => {
    testCase = await launchLiteInteg('lite-verbosity-mid-stream-cycle', {
      timeout: 20000,
    });

    // Inject a streaming agent message. The mocked prompt() doesn't auto-
    // complete (we drive completeTurn() ourselves below), so isProcessing
    // stays true while we type commands.
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

    // /verbosity is turn-inert (PR #4020 command registry), so these dispatch
    // immediately rather than queuing. The turn stays active throughout.
    await testCase.typeAndSubmit('/verbosity default');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity full');
    await testCase.sleepMs(150);
    await testCase.typeAndSubmit('/verbosity lean');
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);
    // Inert commands are NOT queued — they fire immediately. The queue should
    // be empty (or contain only the initial 'start streaming' if it hasn't
    // been consumed yet).
    const queuedSlashCount = store.queuedMessages.filter((m: string) =>
      m.startsWith('/verbosity')
    ).length;
    expect(queuedSlashCount).toBe(0);

    // End the streaming turn.
    await testCase.waitForStore((s) => !s.isProcessing, 5000);

    store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);

    // Streamed content must survive verbatim — re-rendering under a new preset
    // must not mutate <Static>-committed scrollback (append-only contract).
    const allMsgText = store.messages.map((m) => JSON.stringify(m)).join(' ');
    expect(allMsgText).toContain(streamedContent);

    await exitLiteInteg(testCase);
  }, 30000);
});
