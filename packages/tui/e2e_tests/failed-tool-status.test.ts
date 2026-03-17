/**
 * E2E test: failed tool calls must have result.status='error' in the store
 * and show error styling (not success green).
 *
 * Uses a tool with an invalid path that will fail during execution,
 * producing a ToolCallFinished with Error result from the Rust backend.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Failed Tool Status', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('tool that fails execution has result.status=error in store', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-tool-error-result')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Tool use with a path that doesn't exist → will fail
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-1',
            name: 'fs_read',
            input: JSON.stringify({
              ops: [{ path: '/nonexistent/path/that/does/not/exist.txt' }],
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool failure
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'The file could not be read.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read missing file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('could not be read', 15000);

    // Inspect the store to verify the tool message has error result
    const store = await testCase.getStore();
    const toolMsgs = store.messages.filter((m: any) => m.role === 'tool_use');

    console.log(
      'Tool messages:',
      JSON.stringify(
        toolMsgs.map((m: any) => ({
          id: m.id,
          name: m.name,
          isFinished: m.isFinished,
          result: m.result,
          status: m.status,
        })),
        null,
        2
      )
    );

    expect(toolMsgs.length).toBeGreaterThanOrEqual(1);
    const failedTool = toolMsgs[0] as any;
    expect(failedTool.isFinished).toBe(true);
    // This is the key assertion: failed tools must have error result
    expect(failedTool.result).toBeDefined();
    expect(failedTool.result.status).toBe('error');
  }, 30000);
});
