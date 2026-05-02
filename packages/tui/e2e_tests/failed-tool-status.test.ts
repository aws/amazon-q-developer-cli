/**
 * E2E test: failed tool calls must have result.status='error' in the store
 * and show error styling (not success green). Failed calls must also surface
 * the attempted arguments so the user can see what was tried.
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

  // Failed fs_read must render the attempted path and the error message.
  // Before the fix, Read didn't consume `result`, so failed reads showed
  // the Read title with no error or path indication.
  it('failed fs_read shows the attempted path and error message on screen', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-read-shows-params')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const missingPath = '/nonexistent/path/failed-read-test.txt';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-read-1',
            name: 'fs_read',
            input: JSON.stringify({ operations: [{ mode: 'Line', path: missingPath }] }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Read failed as expected.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read the missing file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Read failed as expected', 15000);

    const snapshot = testCase.getSnapshot();
    const screen = snapshot.join('\n');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // The tool name must be visible (some label for the failed tool)
    expect(
      snapshot.some(
        (line) => line.includes('Read') || line.includes('fs_read')
      )
    ).toBe(true);

    // The attempted path must be visible so the user can see what was tried.
    expect(screen).toContain(missingPath);

    // The real parse-error wording must reach the UI (not the generic
    // "Tool execution failed" fallback).
    expect(screen).toContain('does not exist');
    expect(screen).not.toContain('Tool execution failed');
  }, 30000);

  // Failed Shell calls must render the attempted command. Shell.tsx already
  // renders `target={displayCommand}`; this test guards against regressions.
  it('failed shell tool shows the attempted command on screen', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-shell-shows-command')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // A command guaranteed to fail without triggering approval. We use an
    // uncommon marker so we can assert it appears verbatim in the output.
    const command = 'false # failed-shell-test-marker';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-shell-1',
            name: 'execute_bash',
            input: JSON.stringify({ command }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Shell exited non-zero.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('run the failing command');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Shell exited non-zero', 15000);

    const snapshot = testCase.getSnapshot();
    const screen = snapshot.join('\n');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // The tool label and the attempted command must both be visible.
    expect(snapshot.some((line) => line.includes('Shell'))).toBe(true);
    expect(screen).toContain('failed-shell-test-marker');
    // Real backend error, not the generic fallback.
    expect(screen).not.toContain('Tool execution failed');
  }, 30000);

  // Failed fs_read in Image mode must surface image_paths[0] as the target
  // so the user sees which image the agent tried to read.
  it('failed fs_read Image mode shows image_paths[0] on screen', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-read-image-mode')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const missingImg = '/nonexistent/path/failed-image-test.png';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-read-img-1',
            name: 'fs_read',
            input: JSON.stringify({
              operations: [{ mode: 'Image', image_paths: [missingImg] }],
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Image read failed as expected.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read a missing image');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Image read failed', 15000);

    const screen = testCase.getSnapshot().join('\n');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());
    expect(screen).toContain(missingImg);
  }, 30000);

  // Failed fs_write must surface the target path. Write doesn't render its
  // own errors so this also exercises the FallbackError routing.
  it('failed fs_write shows the attempted path and error message on screen', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-write-shows-params')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const missingPath = '/nonexistent/path/failed-write-test.txt';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-write-1',
            name: 'fs_write',
            input: JSON.stringify({
              command: 'create',
              path: missingPath,
              content: 'hi',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Write failed as expected.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('create a file at an unwritable path');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Write failed', 15000);

    const screen = testCase.getSnapshot().join('\n');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());
    expect(screen).toContain(missingPath);
  }, 30000);

  // Unknown tool (ParseError path) must render tool name, target-like args,
  // and the real parse error — not the generic "Tool execution failed".
  it('failed unknown tool shows name, args and real error on screen', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('failed-unknown-tool')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const marker = 'failed-unknown-tool-marker';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'fail-unknown-1',
            name: 'some_mcp_tool_that_does_not_exist',
            input: JSON.stringify({ query: marker, count: 3 }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Unknown tool failed as expected.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('call the unknown tool');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Unknown tool failed', 15000);

    const screen = testCase.getSnapshot().join('\n');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Tool name must appear somewhere on screen.
    expect(screen).toContain('some_mcp_tool_that_does_not_exist');
    // The primary target extracted from args (query) appears on the title line.
    expect(screen).toContain(marker);
    // The real ParseError message replaces the generic fallback.
    expect(screen).toContain('does not exist');
    expect(screen).not.toContain('Tool execution failed');
    // Noisy protocol prefix should be stripped before displaying to the user.
    expect(screen).not.toContain('Failed to parse the tool use');
  }, 30000);
});
