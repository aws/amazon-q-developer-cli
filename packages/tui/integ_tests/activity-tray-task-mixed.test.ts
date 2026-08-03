import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase.js';
import { AgentEventType } from '../src/types/agent-events.js';
import {
  CTRL_S,
  CTRL_X,
  SHIFT_DOWN,
  TAB,
  completeWorkflow,
  launchActivityTrayCase,
  startWorkflow,
  updateTasks,
} from './helpers/activity-tray.js';

describe('activity tray task and mixed-tab PTY stories', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    await testCase?.cleanup();
    testCase = null;
  });

  it('Given an active task list, when the tray stays open through a file write and task update, then the user can keep editing one draft', async () => {
    testCase = await launchActivityTrayCase('tray-task');
    await testCase.typeAndSubmit('start task work');
    await testCase.waitForStore((state) => state.isProcessing);

    await updateTasks(testCase, 'task-create', 'create', [
      {
        id: 'task-1',
        task_description: 'Inspect the queue',
        completed: false,
      },
      {
        id: 'task-2',
        task_description: 'Verify prompt input',
        completed: false,
      },
    ]);
    await testCase.waitForStore((state) => state.tasks.length === 2);

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Tasks (2)', 10_000);

    await testCase.sendKeys('task draft');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'task draft'
    );

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'task-file-write',
      name: 'fs_write',
      kind: 'edit',
      args: {
        command: 'create',
        path: '/tmp/tray-input-recovery.ts',
        content: 'export const trayInput = true;\n',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'task-file-write',
      result: { status: 'success', output: { text: 'File created' } },
    });
    await testCase.waitForVisibleText('tray-input-recovery.ts', 10_000);
    await testCase.sendKeys(' after file write');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'task draft after file write'
    );

    await updateTasks(testCase, 'task-complete', 'complete', [
      {
        id: 'task-1',
        task_description: 'Inspect the queue',
        completed: true,
      },
      {
        id: 'task-2',
        task_description: 'Verify prompt input',
        completed: false,
      },
    ]);
    await testCase.waitForStore(
      (state) => state.tasks[0]?.status === 'completed'
    );

    await testCase.sendKeys(' after task update');
    const state = await testCase.waitForStore(
      (value) =>
        value.commandInputValue ===
        'task draft after file write after task update'
    );
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.tasks).toHaveLength(2);
  }, 35_000);

  it('Given task, message, and workflow tabs, when the user types through every tab and the workflow completes, then the draft never changes owners', async () => {
    testCase = await launchActivityTrayCase('tray-mix');
    await startWorkflow(testCase, 'workflow-1', 'Mixed workflow', [
      'mixed first node',
      'mixed second node',
    ]);
    await testCase.typeAndSubmit('start mixed work');
    await testCase.waitForStore((state) => state.isProcessing);
    await testCase.waitForVisibleText('Mixed workflow running', 10_000);

    await updateTasks(testCase, 'mixed-task-create', 'create', [
      {
        id: 'task-1',
        task_description: 'Mixed task',
        completed: false,
      },
    ]);
    await testCase.waitForStore((state) => state.tasks.length === 1);

    await testCase.sendKeys(CTRL_S);
    await testCase.waitForStore(
      (state) => state.activeInterruptMode === 'queue'
    );
    await testCase.typeAndSubmit('mixed queued message');
    await testCase.waitForStore((state) => state.queuedMessages.length === 1);

    await testCase.sendKeys('draft before open');
    await testCase.waitForStore(
      (state) => state.commandInputValue === 'draft before open'
    );
    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);

    await testCase.sendKeys(SHIFT_DOWN);
    await testCase.sendKeys(' after workflow node');
    await testCase.waitForStore(
      (state) =>
        state.commandInputValue === 'draft before open after workflow node'
    );

    await testCase.sendKeys(TAB);
    await testCase.waitForStore((state) => state.activityTrayTab === 'tasks');
    await testCase.sendKeys(' after tasks');

    await testCase.sendKeys(TAB);
    await testCase.waitForStore((state) => state.activityTrayTab === 'queue');
    await testCase.sendKeys(' after messages');
    expect((await testCase.getStore()).queuedMessages).toEqual([
      'mixed queued message',
    ]);

    await testCase.sendKeys(TAB);
    await testCase.waitForStore(
      (state) => state.activityTrayTab === 'workflow'
    );
    await testCase.sendKeys(' after workflow');

    await completeWorkflow(
      testCase,
      'workflow-1',
      'Mixed workflow',
      'mixed first node'
    );
    await testCase.waitForVisibleText(
      'Workflow completed "Mixed workflow"',
      10_000
    );
    await testCase.sendKeys(' after completion');

    const expected =
      'draft before open after workflow node after tasks after messages after workflow after completion';
    const state = await testCase.waitForStore(
      (value) => value.commandInputValue === expected
    );
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.tasks).toHaveLength(1);
    expect(state.queuedMessages).toEqual(['mixed queued message']);
    expect(testCase.getSnapshot().join('\n')).toContain(
      'Workflow completed "Mixed workflow"'
    );
  }, 45_000);
});
