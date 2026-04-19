/**
 * E2E test for shell output streaming (live output).
 *
 * Feature: shell-output-streaming
 * Property 4: Live and finished output use the same expand/collapse behavior
 *
 * Verifies the full pipeline: Rust ExecuteCmd streaming → ACP bridge →
 * TUI store (liveOutput) → rendered output → final result replaces liveOutput.
 *
 * Validates: Requirements 6.1, 6.2, 6.3
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Shell live output streaming', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('liveOutput is populated during execution and cleared on completion', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-live-output')
      .withTerminal({ width: 120, height: 40 })
      .withCliArgs('--trust-tools=shell')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Slow command: 7 lines with 1s delays gives ~7s to poll the store.
    const command =
      'for i in 1 2 3 4 5 6 7; do echo "stream-line-$i"; sleep 1; done';

    // Stream 1: Tool use — Rust backend executes for real, emitting
    // ToolCallUpdate events via event_tx as each line is read.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-live-1',
            name: 'shell',
            input: JSON.stringify({ command }),
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
          data: { content: 'Streaming complete.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('run stream');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Shell', 15000);

    // Poll the store while the command is still running to catch liveOutput.
    let sawLiveOutput = false;
    let capturedLiveOutput: string[] = [];
    const pollStart = Date.now();
    while (Date.now() - pollStart < 30000) {
      const store = await testCase.getStore();
      const toolMsg = store.messages.find(
        (m) => m.role === 'tool_use' && m.id === 'tool-live-1'
      );
      if (toolMsg && 'liveOutput' in toolMsg && toolMsg.liveOutput && toolMsg.liveOutput.length > 0) {
        sawLiveOutput = true;
        capturedLiveOutput = toolMsg.liveOutput;
        break;
      }
      if (toolMsg && 'result' in toolMsg && toolMsg.result) {
        break;
      }
      await testCase.sleepMs(100);
    }

    // Core assertion: liveOutput was actually populated during execution
    expect(sawLiveOutput).toBe(true);
    expect(capturedLiveOutput.join('\n')).toContain('stream-line-');

    // Wait for completion
    await testCase.waitForText('Streaming complete', 30000);

    // Verify final state: liveOutput cleared, result set
    const finalStore = await testCase.getStore();
    const finalToolMsg = finalStore.messages.find(
      (m) => m.role === 'tool_use' && m.id === 'tool-live-1'
    );
    expect(finalToolMsg).toBeDefined();

    if (finalToolMsg && 'liveOutput' in finalToolMsg) {
      expect(finalToolMsg.liveOutput).toBeUndefined();
    }

    if (finalToolMsg && 'result' in finalToolMsg) {
      expect(finalToolMsg.result).toBeDefined();
      expect(finalToolMsg.result?.status).toBe('success');
    }

    // 7 lines > PREVIEW_LINES(5), so expand hint should be visible
    const snapshot = testCase.getSnapshot();
    expect(snapshot.join('\n')).toContain('stream-line-1');
    expect(snapshot.some((line) => line.includes('+') && line.includes('lines'))).toBe(true);
  }, 60000);

  it('short output (≤ PREVIEW_LINES) shows all lines without expand hint', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-live-short')
      .withTerminal({ width: 120, height: 40 })
      .withCliArgs('--trust-tools=shell')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const command = 'for w in alpha beta gamma; do echo "$w"; sleep 1; done';

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-live-2',
            name: 'shell',
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
          data: { content: 'Short output done.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('run short');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Shell', 15000);

    // Poll for liveOutput during execution
    let sawLiveOutput = false;
    const pollStart = Date.now();
    while (Date.now() - pollStart < 30000) {
      const store = await testCase.getStore();
      const toolMsg = store.messages.find(
        (m) => m.role === 'tool_use' && m.id === 'tool-live-2'
      );
      if (toolMsg && 'liveOutput' in toolMsg && toolMsg.liveOutput) {
        sawLiveOutput = true;
        break;
      }
      if (toolMsg && 'result' in toolMsg && toolMsg.result) {
        break;
      }
      await testCase.sleepMs(100);
    }

    expect(sawLiveOutput).toBe(true);

    await testCase.waitForText('Short output done', 30000);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('alpha');
    expect(screenText).toContain('beta');
    expect(screenText).toContain('gamma');

    // No expand hint since 3 lines ≤ PREVIEW_LINES(5)
    expect(
      snapshot.some((line) => line.includes('+') && line.includes('lines') && line.includes('ctrl+o'))
    ).toBe(false);
  }, 60000);
});
