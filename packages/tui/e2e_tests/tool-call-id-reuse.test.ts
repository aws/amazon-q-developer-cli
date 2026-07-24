/**
 * E2E regression for tool-call id reuse across turns.
 *
 * Some serving paths (observed with GPT models) emit tool_use_ids that are
 * only unique within a single request — call_0, call_1, … reset every turn —
 * so a multi-turn session reuses ids. The TUI keys tool rows by id, so before
 * the conversion-layer remap a reused id silently rewrote the previous turn's
 * finished row (already flushed to static scrollback): the new tool never
 * rendered and its approval prompt showed a stale tool name with no command.
 *
 * This drives the REAL Rust agent with two turns whose shell tool calls share
 * the wire id `call_0` and asserts the TUI creates two distinct rows and the
 * second approval targets the new row.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/** Push one turn: a shell ToolUseEvent + end, then the post-tool response. */
async function pushShellTurn(
  tc: E2ETestCase,
  toolUseId: string,
  command: string,
  followUp: string
) {
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'ToolUseEvent',
        data: {
          tool_use_id: toolUseId,
          name: 'shell',
          input: JSON.stringify({ command }),
          stop: true,
        },
      },
    },
  ]);
  await tc.pushSendMessageResponse(null);
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: { kind: 'AssistantResponseEvent', data: { content: followUp } },
    },
  ]);
  await tc.pushSendMessageResponse(null);
}

describe('tool-call id reuse across turns', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('a reused wire id renders a second tool row and the approval shows the new command', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('tool-call-id-reuse')
      .withTerminal({ width: 120, height: 40 })
      .launch();
    const tc = testCase;

    await tc.waitForText('ask a question', 10000);
    await tc.getSessionId();

    // Turn 1: shell tool with wire id call_0.
    await pushShellTurn(tc, 'call_0', 'echo hello', 'First done.');
    await tc.sendKeys('run echo hello');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForText('requires approval', 15000);

    const store1 = await tc.getStore();
    expect(store1.pendingApproval?.toolCall?.toolCallId).toBe('call_0');
    await tc.pressEnter(); // approve
    await tc.waitForText('First done.', 15000);

    // Turn 2: the serving path reuses wire id call_0 for a NEW tool call.
    await pushShellTurn(tc, 'call_0', 'rm /tmp/foo.txt', 'Second done.');
    await tc.sendKeys('now delete the file');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForText('requires approval', 15000);

    // The approval must target the NEW row, not the finished turn-1 row.
    const store2 = await tc.getStore();
    expect(store2.pendingApproval?.toolCall?.toolCallId).toBe('call_0#1');

    // Two distinct tool rows exist — the reused id did not overwrite turn 1.
    const toolRows = store2.messages.filter(
      (m: { role: string }) => m.role === 'tool_use'
    );
    expect(toolRows.map((m: { id: string }) => m.id)).toEqual([
      'call_0',
      'call_0#1',
    ]);
    const secondRow = toolRows[1] as unknown as {
      name: string;
      content: string;
    };
    expect(secondRow.content).toContain('rm /tmp/foo.txt');

    // The new command is visible on screen while the approval is pending.
    await tc.waitForText('rm /tmp/foo.txt', 5000);

    await tc.pressEnter(); // approve
    await tc.waitForText('Second done.', 15000);
  }, 60000);
});
