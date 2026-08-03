import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase.js';
import {
  CTRL_X,
  launchActivityTrayCase,
  updateTasks,
} from './helpers/activity-tray.js';

const META_UP = '\x1b[1;3A';
const SHIFT_ENTER = '\x1b[13;2u';

describe('activity tray Lite PTY stories', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    await testCase?.cleanup();
    testCase = null;
  });

  it('Given the passive Lite tray is expanded, when the prompt navigates a multiline draft, then input remains prompt-owned', async () => {
    testCase = await launchActivityTrayCase('tray-lite', { lite: true });
    expect((await testCase.getStore()).uiMode).toBe('lite');

    await testCase.typeAndSubmit('start task work');
    await testCase.waitForStore((state) => state.isProcessing);
    await updateTasks(testCase, 'lite-task-create', 'create', [
      {
        id: 'task-1',
        task_description: 'Lite task',
        completed: false,
      },
    ]);
    await testCase.waitForStore((state) => state.tasks.length === 1);
    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);

    await testCase.sendKeys('top');
    await testCase.sendKeys(SHIFT_ENTER);
    await testCase.sendKeys('bottom');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'top\nbottom'
    );

    await testCase.sendKeys(META_UP);
    await testCase.sendKeys('!');
    const state = await testCase.waitForStore(
      (value) => value.commandInputValue === 'top!\nbottom'
    );
    expect(state.activityTrayExpanded).toBe(true);
  }, 30_000);
});
