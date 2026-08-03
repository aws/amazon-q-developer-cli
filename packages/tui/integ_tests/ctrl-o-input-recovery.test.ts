import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase.js';
import { AgentEventType } from '../src/types/agent-events.js';

const CTRL_O = '\x0f';

describe('Ctrl+O prompt recovery PTY stories', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    await testCase?.cleanup();
    testCase = null;
  });

  it('Given expanded file-write output loses its registration, when Ctrl+O or Escape collapses it, then prompt input recovers', async () => {
    testCase = await TestCase.builder()
      .withTestName('ctrl-o-input-recovery')
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15_000);

    await testCase.typeAndSubmit('create a multiline file');
    await testCase.waitForStore((state) => state.isProcessing);
    const fileContent = Array.from(
      { length: 24 },
      (_, index) => `export const value${index + 1} = ${index + 1};`
    ).join('\n');
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'write-input-recovery',
      name: 'fs_write',
      kind: 'edit',
      args: {
        command: 'create',
        path: '/tmp/input-recovery.ts',
        content: fileContent,
      },
    });
    await testCase.waitForVisibleText('input-recovery.ts', 10_000);
    await testCase.waitForStore((state) => state.hasExpandableToolOutputs);

    await testCase.sendKeys(CTRL_O);
    await testCase.waitForStore((state) => state.toolOutputsExpanded);

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'write-input-recovery',
      result: { status: 'success', output: { text: 'File created' } },
    });
    await testCase.completeTurn();
    await testCase.waitForStore((state) => !state.isProcessing, 5_000);
    await testCase.mockSetExpandableToolOutputs(false);
    const completedWrite = await testCase.waitForStore(
      (state) => !state.hasExpandableToolOutputs
    );
    expect(completedWrite.toolOutputsExpanded).toBe(true);

    await testCase.sendKeys(CTRL_O);
    await testCase.waitForStore((state) => !state.toolOutputsExpanded);
    await testCase.sendKeys('after file write');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'after file write'
    );

    await testCase.mockSetExpandableToolOutputs(true);
    await testCase.sendKeys(CTRL_O);
    await testCase.waitForStore((state) => state.toolOutputsExpanded);
    await testCase.pressEscape();
    await testCase.waitForStore((state) => !state.toolOutputsExpanded);
    await testCase.sendKeys(' after escape');

    const state = await testCase.waitForStore(
      (value) => value.commandInputValue === 'after file write after escape'
    );
    expect(state.hasExpandableToolOutputs).toBe(true);
  }, 30_000);
});
