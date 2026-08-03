import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase.js';
import {
  CTRL_S,
  CTRL_X,
  LEGACY_SHIFT_DOWN,
  SHIFT_DOWN,
  SHIFT_LEFT,
  SHIFT_RIGHT,
  TAB,
  completeWorkflow,
  launchActivityTrayCase,
  startWorkflow,
} from './helpers/activity-tray.js';

describe('activity tray workflow PTY stories', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    await testCase?.cleanup();
    testCase = null;
  });

  it('Given one active workflow, when the tray opens and node selection changes, then ordinary editing remains prompt-owned', async () => {
    testCase = await launchActivityTrayCase('tray-wf1');
    await startWorkflow(testCase, 'workflow-1', 'Typing workflow', [
      'typing first node',
      'typing second node',
    ]);
    await testCase.typeAndSubmit('start workflow');
    await testCase.waitForVisibleText('Typing workflow running', 10_000);

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);

    await testCase.sendKeys('jk123');
    await testCase.waitForStore((state) => state.commandInputValue === 'jk123');
    await testCase.sendKeys('\x1b[D');
    await testCase.sleepMs(50);
    await testCase.sendKeys('X');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'jk12X3'
    );

    await testCase.sendKeys(LEGACY_SHIFT_DOWN);
    await testCase.waitForVisibleText('typing second node', 10_000);
    await testCase.sendKeys(' after node move');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'jk12X after node move3'
    );

    await testCase.pressEscape();
    const collapsed = await testCase.waitForStore(
      (state) => !state.activityTrayExpanded
    );
    expect(collapsed.commandInputValue).toBe('jk12X after node move3');

    await testCase.sendKeys(' after collapse');
    await testCase.waitForStore(
      (state) =>
        state.commandInputValue === 'jk12X after node move after collapse3'
    );
  }, 35_000);

  it('Given two workflows and one queued message, when the user types, switches workflows and tabs, and both workflows complete, then one draft survives', async () => {
    testCase = await launchActivityTrayCase('tray-wf2');
    await startWorkflow(testCase, 'workflow-1', 'Alpha workflow', [
      'alpha first node',
      'alpha second node',
    ]);
    await startWorkflow(testCase, 'workflow-2', 'Beta workflow', [
      'beta first node',
      'beta second node',
    ]);
    await testCase.typeAndSubmit('start workflows');
    await testCase.waitForVisibleText('Beta workflow running', 10_000);

    await testCase.sendKeys(CTRL_S);
    await testCase.waitForStore(
      (state) => state.activeInterruptMode === 'queue'
    );
    await testCase.typeAndSubmit('queued handoff');
    await testCase.waitForStore((state) => state.queuedMessages.length === 1);

    await testCase.sendKeys('draft before open');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'draft before open'
    );
    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('alpha first node', 10_000);

    await testCase.sendKeys(' after open');
    const draft = 'draft before open after open';
    await testCase.waitForStore((state) => state.commandInputValue === draft);

    await testCase.sendKeys(SHIFT_RIGHT);
    await testCase.waitForVisibleText('beta first node', 10_000);
    expect((await testCase.getStore()).commandInputValue).toBe(draft);

    await testCase.sendKeys(SHIFT_DOWN);
    await testCase.sendKeys(' after node move');
    const movedDraft = `${draft} after node move`;
    await testCase.waitForStore(
      (state) => state.commandInputValue === movedDraft
    );

    await testCase.sendKeys(SHIFT_LEFT);
    await testCase.waitForVisibleText('alpha first node', 10_000);
    await testCase.sendKeys(TAB);
    await testCase.waitForStore((state) => state.activityTrayTab === 'queue');
    expect((await testCase.getStore()).commandInputValue).toBe(movedDraft);

    await testCase.sendKeys(TAB);
    await testCase.waitForStore(
      (state) => state.activityTrayTab === 'workflow'
    );
    await completeWorkflow(
      testCase,
      'workflow-1',
      'Alpha workflow',
      'alpha first node'
    );
    await completeWorkflow(
      testCase,
      'workflow-2',
      'Beta workflow',
      'beta first node'
    );

    await testCase.sendKeys(' after completion');
    const completedDraft = `${movedDraft} after completion`;
    await testCase.waitForStore(
      (state) => state.commandInputValue === completedDraft
    );

    await testCase.pressEscape();
    const collapsed = await testCase.waitForStore(
      (state) => !state.activityTrayExpanded
    );
    expect(collapsed.commandInputValue).toBe(completedDraft);

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Messages (1)', 10_000);
    await testCase.sleepMs(200);
    expect(testCase.getSnapshot().join('\n')).not.toContain('Workflow (');

    await testCase.sendKeys(' after queue reopen');
    await testCase.waitForStore(
      (state) =>
        state.commandInputValue === `${completedDraft} after queue reopen`
    );
  }, 45_000);
});
