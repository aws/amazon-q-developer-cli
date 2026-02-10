/**
 * E2E tests for tool message rendering (Shell, Read, Write, Generic).
 *
 * These tests verify that tool messages are rendered correctly in the TUI
 * when receiving ToolUseEvent from the backend. The tests inject mock
 * responses and verify the tool UI elements appear on screen.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Tool Messages', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('renders shell tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push shell tool call and response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-1',
            name: 'execute_bash',
            input: JSON.stringify({ command: 'ls -la' }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Command executed.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('run ls');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('Command executed', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify shell tool indicator is displayed (shows "Ran" for execute_bash)
    expect(snapshot.some((line) => line.includes('Ran'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Command executed'))).toBe(true);
  }, 30000);

  it('renders read tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('read-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push read tool call and response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-2',
            name: 'fs_read',
            input: JSON.stringify({ ops: [{ path: 'src/index.ts' }] }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'File contents here.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('read file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('File contents', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify read tool indicator is displayed
    expect(snapshot.some((line) => line.includes('Read'))).toBe(true);
    expect(snapshot.some((line) => line.includes('File contents'))).toBe(true);
  }, 30000);

  it('renders write tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('write-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push write tool call and response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-3',
            name: 'fs_write',
            input: JSON.stringify({
              command: 'create',
              path: 'src/new-file.ts',
              content: 'export const hello = "world";',
            }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'File created successfully.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('create file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('File created', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify write tool response is displayed (shows "Created" for fs_write create)
    expect(snapshot.some((line) => line.includes('Created'))).toBe(true);
    expect(snapshot.some((line) => line.includes('File created'))).toBe(true);
  }, 30000);

  it('renders multiple tool calls in sequence', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('multiple-tool-calls')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push multiple tool calls followed by response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-read',
            name: 'fs_read',
            input: JSON.stringify({ ops: [{ path: 'package.json' }] }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-write',
            name: 'fs_write',
            input: JSON.stringify({
              command: 'strReplace',
              path: 'package.json',
              oldStr: '"version": "1.0.0"',
              newStr: '"version": "1.1.0"',
            }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Version bumped to 1.1.0' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('bump version');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('Version bumped', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify both tool indicators and response are displayed
    expect(snapshot.some((line) => line.includes('Read'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Version bumped'))).toBe(true);
  }, 30000);

  it('renders grep tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('grep-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push grep tool call and response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-grep',
            name: 'grep',
            input: JSON.stringify({ pattern: 'useState' }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Found useState in 3 files.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('search for useState');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('Found useState', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify grep tool indicator is displayed (shows "Grepped" for grep)
    expect(snapshot.some((line) => line.includes('Grepped'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Found useState'))).toBe(true);
  }, 30000);

  it('renders glob tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('glob-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push glob tool call and response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-glob',
            name: 'glob',
            input: JSON.stringify({ pattern: '**/*.tsx' }),
            stop: true,
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Found 15 TypeScript files.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('find tsx files');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for response to render
    await testCase.waitForText('Found 15', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify glob tool indicator is displayed (shows "Globbed" for glob)
    expect(snapshot.some((line) => line.includes('Globbed'))).toBe(true);
    expect(snapshot.some((line) => line.includes('TypeScript files'))).toBe(true);
  }, 30000);
});
