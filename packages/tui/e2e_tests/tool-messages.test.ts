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
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

describe('Tool Messages', () => {
  let testCase: E2ETestCase | null = null;
  let tempDir: string = '';

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    if (tempDir) {
      try { fs.rmSync(tempDir, { recursive: true }); } catch { /* ignore */ }
      tempDir = '';
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
    await testCase.waitForText('Ran', 10000);
    await testCase.waitForText('Command executed', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Ran'))).toBe(true);
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
    await testCase.waitForText('Wrote', 10000);
    await testCase.waitForText('File created', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Wrote'))).toBe(true);
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
    await testCase.waitForText('Searched', 10000);
    await testCase.waitForText('Found useState', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Searched'))).toBe(true);
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
    await testCase.waitForText('Found', 10000);
    await testCase.waitForText('Found 15', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some((line) => line.includes('Found'))).toBe(true);
    expect(snapshot.some((line) => line.includes('TypeScript files'))).toBe(
      true
    );
  }, 30000);

  it('write strReplace sends correct start line via ACP', async () => {
    // Create a temp file where the replacement target is on line 5 (1-indexed).
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-write-'));
    const filePath = path.join(tempDir, 'test-file.py');
    fs.writeFileSync(filePath, [
      'import os',
      'import sys',
      '',
      'def main():',
      '    print("hello world")',
      '    return 0',
      '',
      'if __name__ == "__main__":',
      '    main()',
    ].join('\n'));

    testCase = await E2ETestCase.builder()
      .withTestName('write-str-replace')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-str-replace-1',
            name: 'write',
            input: JSON.stringify({
              command: 'strReplace',
              path: filePath,
              oldStr: '    print("hello world")',
              newStr: '    print("hello, world!")\n    print("goodbye, world!")',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Done.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('update print');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the approval dialog — ToolCall event has been fully processed by then
    await testCase.waitForText('requires approval', 15000);

    // Verify the snapshot shows the correct line number in the diff summary
    const snapshot = testCase.getSnapshot();
    expect(snapshot.some((line) => line.includes('at L5'))).toBe(true);

    // Verify the store has correct locations and diff content
    const store = await testCase.getStore();
    const toolMsg = store.messages.find((m) => m.role === 'tool_use');
    expect(toolMsg).toBeDefined();

    if (!('locations' in toolMsg!)) throw new Error('no locations on tool msg');
    expect(toolMsg!.locations![0]!.line).toBe(5);

    const parsed = JSON.parse((toolMsg as any).content);
    expect(parsed.command).toBe('strReplace');
    expect(parsed.oldStr).toContain('print("hello world")');
    expect(parsed.newStr).toContain('print("hello, world!")');
  }, 30000);

  it('write create overwrite preserves new content in store', async () => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-create-'));
    const filePath = path.join(tempDir, 'game.py');
    fs.writeFileSync(filePath, 'def play():\n    print("playing")\n    return True\n');

    testCase = await E2ETestCase.builder()
      .withTestName('write-create-overwrite')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const newContent = 'def play():\n    print("playing game")\n    score = 0\n    return score';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-create-1',
            name: 'write',
            input: JSON.stringify({ command: 'create', path: filePath, content: newContent }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Done.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('update game');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('requires approval', 15000);

    // Verify the store has the tool message with correct content
    const store = await testCase.getStore();
    const toolMsg = store.messages.find((m) => m.role === 'tool_use');
    expect(toolMsg).toBeDefined();

    const parsed = JSON.parse((toolMsg as any).content);
    expect(parsed.command).toBe('create');
    expect(parsed.newStr).toContain('print("playing game")');
    expect(parsed.newStr).toContain('return score');

    // Verify locations exist (start_line = 1 for create)
    if ('locations' in toolMsg!) {
      expect(toolMsg!.locations![0]!.line).toBe(1);
    }
  }, 30000);
});
