/**
 * E2E tests for tool message rendering (Shell, Read, Write, Generic).
 *
 * These tests verify that tool messages are rendered correctly in the TUI
 * when receiving ToolUseEvent from the backend. The tests inject mock
 * responses and verify the tool UI elements appear on screen.
 *
 * The mock infrastructure replaces the LLM API stream but the Rust backend
 * still executes tools for real. After tool execution the agent calls
 * send_message again with tool results, so each test must push TWO mock
 * response streams:
 *   1. Tool use events (+ optional assistant text)
 *   2. Final assistant response after tool execution
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

    // Stream 1: Tool use
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-1',
            name: 'execute_bash',
            input: JSON.stringify({ command: 'echo hello' }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Command executed.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('run ls');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tool to finish and assistant response to render
    await testCase.waitForText('Bashed', 10000);
    await testCase.waitForText('Command executed', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Bashed'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Command executed'))).toBe(
      true
    );
  }, 30000);

  it('renders read tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('read-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Tool use
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-2',
            name: 'fs_read',
            input: JSON.stringify({ ops: [{ path: 'package.json' }] }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'File contents here.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('read file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tool to finish and assistant response to render
    await testCase.waitForText('Read', 10000);
    await testCase.waitForText('File contents', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Read'))).toBe(true);
    expect(snapshot.some((line) => line.includes('File contents'))).toBe(true);
  }, 30000);

  it('renders write tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('write-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Tool use
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
              path: '/tmp/kiro-e2e-test-file.txt',
              content: 'export const hello = "world";',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'File created successfully.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('create file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tool to finish and assistant response to render
    await testCase.waitForText('Created', 10000);
    await testCase.waitForText('File created', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Created'))).toBe(true);
    expect(snapshot.some((line) => line.includes('File created'))).toBe(true);
  }, 30000);

  it('renders multiple tool calls in sequence', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('multiple-tool-calls')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Multiple tool uses
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
            tool_use_id: 'tool-grep',
            name: 'grep',
            input: JSON.stringify({ pattern: 'version', path: '.' }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Version bumped to 1.1.0' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('bump version');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tools to finish and assistant response to render
    await testCase.waitForText('Read', 10000);
    await testCase.waitForText('Version bumped', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Read'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Version bumped'))).toBe(true);
  }, 30000);

  it('renders grep tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('grep-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Tool use
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-grep',
            name: 'grep',
            input: JSON.stringify({ pattern: 'useState', path: '.' }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Found useState in 3 files.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('search for useState');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tool to finish and assistant response to render
    await testCase.waitForText('Grepped', 10000);
    await testCase.waitForText('Found useState', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Grepped'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Found useState'))).toBe(true);
  }, 30000);

  it('renders glob tool message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('glob-tool-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream 1: Tool use
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
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: Assistant response after tool execution
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Found 15 TypeScript files.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send prompt
    await testCase.sendKeys('find tsx files');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for tool to finish and assistant response to render
    await testCase.waitForText('Globbed', 10000);
    await testCase.waitForText('Found 15', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Globbed'))).toBe(true);
    expect(snapshot.some((line) => line.includes('TypeScript files'))).toBe(
      true
    );
  }, 30000);
});
