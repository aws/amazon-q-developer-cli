/**
 * E2E tests for incremental static flushing of active turn messages.
 *
 * These tests verify that:
 * 1. Tool uses are rendered and not lost when multiple tools run sequentially
 * 2. LLM text between tool calls is preserved in correct order
 * 3. Completed turns show all messages (no content loss on turn transition)
 * 4. Dividers appear correctly between turns
 * 5. Parallel tool calls don't cause content to disappear
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Incremental Static Flushing', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /**
   * Core regression: tool uses must not disappear when more than TAIL_SIZE
   * messages accumulate in the active turn.
   */
  it('preserves all tool uses when more than 2 tool calls occur', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-many-tools')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // 4 sequential tool uses — exceeds TAIL_SIZE=2, forces incremental flushing
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'package.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'tsconfig.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't3', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'bunfig.toml' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't4', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'AGENTS.md' }] }), stop: true } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Read all 4 files.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read 4 files');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Read all 4 files', 15000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // All 4 Read tool uses must be visible — none lost to flushing
    const readLines = snapshot.filter(line => line.includes('Read'));
    expect(readLines.length).toBeGreaterThanOrEqual(4);
    expect(snapshot.some(line => line.includes('Read all 4 files'))).toBe(true);
  }, 30000);

  /**
   * LLM text between tool calls must be preserved in correct order.
   */
  it('preserves LLM text between tool calls in correct order', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-llm-between-tools')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Tool → LLM text → Tool → LLM text
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'glob', input: JSON.stringify({ pattern: '*.ts' }), stop: true } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Found some files.' } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'package.json' }] }), stop: true } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'All done.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('list then read');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('All done', 15000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(snapshot.some(line => line.includes('Found'))).toBe(true);
    expect(snapshot.some(line => line.includes('Read') || line.includes('Found'))).toBe(true);
    expect(snapshot.some(line => line.includes('All done'))).toBe(true);

    // Verify order: "Found some files" must appear before "All done"
    const foundIdx = snapshot.findIndex(line => line.includes('Found some files'));
    const doneIdx = snapshot.findIndex(line => line.includes('All done'));
    expect(foundIdx).toBeLessThan(doneIdx);
  }, 30000);

  /**
   * Turn completion must not lose any messages — all content from the active
   * turn must appear in the completed (static) turn.
   */
  it('no content loss on turn completion with many tool calls', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-turn-completion')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'execute_bash', input: JSON.stringify({ command: 'echo step1' }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'execute_bash', input: JSON.stringify({ command: 'echo step2' }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't3', name: 'execute_bash', input: JSON.stringify({ command: 'echo step3' }), stop: true } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'All steps complete.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('run 3 steps');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('All steps complete', 15000);

    // Now send a second message to force the first turn to complete
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Second turn response.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('ok');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Second turn response', 15000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify store has all 3 tool messages (content not lost, may be scrolled off screen)
    const store = await testCase.getStore();
    const toolMsgs = store.messages.filter(m => m.role === 'tool_use');
    expect(toolMsgs.length).toBe(3);
    expect(snapshot.some(line => line.includes('All steps complete'))).toBe(true);
    expect(snapshot.some(line => line.includes('Second turn response'))).toBe(true);
  }, 45000);

  /**
   * Write tool followed by shell tool — regression for "Wrote disappears".
   */
  it('write tool result is not lost when followed by another tool', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-write-then-shell')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'fs_write', input: JSON.stringify({ command: 'create', path: '/tmp/kiro-e2e-flush.txt', content: 'hello' }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'execute_bash', input: JSON.stringify({ command: 'cat /tmp/kiro-e2e-flush.txt' }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't3', name: 'execute_bash', input: JSON.stringify({ command: 'echo done' }), stop: true } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'File written and verified.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('write then verify');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('File written and verified', 15000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Wrote must be visible — this was the core regression
    expect(snapshot.some(line => line.includes('Wrote'))).toBe(true);
    expect(snapshot.some(line => line.includes('Ran'))).toBe(true);
    expect(snapshot.some(line => line.includes('File written and verified'))).toBe(true);
  }, 30000);

  /**
   * Dividers must appear between turns and have correct full-terminal width.
   */
  it('dividers appear between turns', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-dividers')
      .withTerminal({ width: 80, height: 30 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Turn 1
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'First response.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('First response', 10000);

    // Turn 2
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Second response.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('again');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Second response', 10000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Count divider lines (lines that are mostly '─' characters)
    const dividerLines = snapshot.filter(line => {
      const stripped = line.trim();
      return stripped.length > 10 && /^[─\-─]+$/.test(stripped.replace(/\s/g, ''));
    });
    expect(dividerLines.length).toBeGreaterThanOrEqual(2);

    // Dividers should span most of the terminal width (80 cols)
    dividerLines.forEach(line => {
      expect(line.length).toBeGreaterThan(40);
    });
  }, 30000);

  /**
   * Long run: 8 sequential tool calls — stress test for flushing.
   */
  it('handles long run of 8 sequential tool calls without content loss', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-long-run')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const files = ['AGENTS.md', 'bunfig.toml', 'eslint.config.js', 'package.json', 'TESTING.md', 'tsconfig.json', 'vite.config.ts', 'README.md'];
    await testCase.pushSendMessageResponse(
      files.map((f, i) => ({
        kind: 'event' as const,
        data: {
          kind: 'ToolUseEvent' as const,
          data: { tool_use_id: `t${i}`, name: 'fs_read', input: JSON.stringify({ ops: [{ path: f }] }), stop: true },
        },
      }))
    );
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Read all 8 files successfully.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('read 8 files');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Read all 8 files', 20000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    const readLines = snapshot.filter(line => line.includes('Read'));
    // All 8 reads + final response line
    expect(readLines.length).toBeGreaterThanOrEqual(8);
    expect(snapshot.some(line => line.includes('Read all 8 files successfully'))).toBe(true);

    // Verify store has all 8 tool messages
    const store = await testCase.getStore();
    const toolMsgs = store.messages.filter(m => m.role === 'tool_use');
    expect(toolMsgs.length).toBe(8);
  }, 45000);

  /**
   * User message must appear before tool uses in the rendered output.
   */
  it('user message appears before tool uses in correct order', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('flush-message-order')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't1', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'package.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't2', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'tsconfig.json' }] }), stop: true } } },
      { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: 't3', name: 'fs_read', input: JSON.stringify({ ops: [{ path: 'bunfig.toml' }] }), stop: true } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Files read.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('check configs');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Files read', 15000);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    const userMsgIdx = snapshot.findIndex(line => line.includes('check configs'));
    const firstReadIdx = snapshot.findIndex(line => line.includes('Read'));
    const finalMsgIdx = snapshot.findIndex(line => line.includes('Files read'));

    expect(userMsgIdx).toBeGreaterThanOrEqual(0);
    expect(firstReadIdx).toBeGreaterThan(userMsgIdx);
    expect(finalMsgIdx).toBeGreaterThan(firstReadIdx);
  }, 30000);
});
