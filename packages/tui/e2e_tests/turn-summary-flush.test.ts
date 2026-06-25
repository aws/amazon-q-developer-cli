/**
 * Regression test: turn summary (credits + time) disappears when a turn was
 * incrementally flushed and a new prompt is entered.
 *
 * Root cause: when a completed turn had incremental flushing, its remaining
 * tail messages are appended to <Static> as individual `msg` items — but no
 * summary item is appended. The summary was only visible in the dynamic
 * ActiveTurnTail area, which gets replaced by the new active turn.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Turn summary survives incremental flush', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('turn summary remains visible after starting a second turn', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('turn-summary-flush')
      .withTerminal({ width: 120, height: 50 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Turn 1: 4 tool calls (exceeds TAIL_SIZE=2, forces incremental flushing)
    // + MeteringEvent to produce a turn summary with credits
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'package.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'tsconfig.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't3', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'bunfig.toml' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't4', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'AGENTS.md' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'MeteringEvent', data: { usage: 0.47, unit: 'credit', unit_plural: 'credits' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Done reading files.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read files');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Done reading files', 15000);
    await testCase.waitForIdle(10000);

    // Wait for the turn summary to render on screen
    await testCase.waitForText('Credits', 10000);

    console.log('Snapshot before turn 2:\n' + testCase.getSnapshotFormatted());

    // Verify summary is visible before second turn
    const snapshotBeforeTurn2 = testCase.getSnapshot();
    expect(snapshotBeforeTurn2.some(line => line.includes('Credits'))).toBe(true);

    // Turn 2: simple response (forces turn 1 to complete and move to static)
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Second turn response.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('ok');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Second turn response', 15000);
    await testCase.waitForIdle(10000);

    console.log('Snapshot after turn 2:\n' + testCase.getSnapshotFormatted());

    // THE BUG: the turn 1 summary should still be visible after turn 2 starts,
    // but it disappears because the incrementally-flushed turn completion path
    // doesn't append a summary item to <Static>.
    const snapshotAfterTurn2 = testCase.getSnapshot();
    expect(
      snapshotAfterTurn2.some(line => line.includes('Credits')),
    ).toBe(true);
  }, 60000);
});
