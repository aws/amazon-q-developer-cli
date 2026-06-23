import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Bug-mine 1.2: Tool batch held until contiguous done-prefix settles.
 *
 * Validates that when two parallel tools are created (A first, B second) and
 * tool B finishes before tool A, the final scrollback still shows them in
 * creation order (A above B). Without the contiguous-prefix hold logic in
 * static-flush.ts:64-78, B would flush to <Static> first at an index where A
 * would later need to live, scrambling the display order.
 */
describe('lite tool batch order [bug-mine 1.2]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('tools appear in creation order regardless of completion order', async () => {
    testCase = await launchLiteInteg('lite-tool-batch-order');

    // Two tools created A-then-B; B completes before A (out-of-order). The
    // contiguous done-prefix hold (static-flush.ts:64-78) must still flush
    // them in creation order. The synchronous drain in MockSessionClient
    // delivers these to the per-prompt handler once typeAndSubmit runs.
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-alpha',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/alpha.txt' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-beta',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'echo beta' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-beta',
      result: { status: 'success', output: 'beta output' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-alpha',
      result: { status: 'success', output: 'alpha output' },
    });

    await testCase.typeAndSubmit('t0');
    // End the turn so tools are rendered in scrollback
    await testCase.completeTurn();
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    const toolMessages = store.messages.filter((m) => m.role === 'tool_use');

    // === Core assertions: creation order preserved ===

    // Store order: tool-alpha must appear before tool-beta in messages array
    const alphaIdx = toolMessages.findIndex((m) => m.id === 'tool-alpha');
    const betaIdx = toolMessages.findIndex((m) => m.id === 'tool-beta');
    expect(alphaIdx).toBe(0);
    expect(betaIdx).toBe(1);

    // Terminal order: "Read" must appear above "Shell" in scrollback
    const snapshot = testCase.getSnapshot();
    const alphaLine = snapshot.findIndex((line) => line.includes('Read'));
    const betaLine = snapshot.findIndex((line) => line.includes('Shell'));
    expect(alphaLine).not.toBe(-1);
    expect(betaLine).not.toBe(-1);
    expect(alphaLine).toBeLessThan(betaLine);

    await exitLiteInteg(testCase);
  }, 30000);
});
