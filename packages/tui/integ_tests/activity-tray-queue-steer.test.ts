import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase.js';
import { AgentEventType } from '../src/types/agent-events.js';
import {
  BACKSPACE,
  CTRL_S,
  CTRL_X,
  SHIFT_DOWN,
  TAB,
  eraseDraft,
  launchActivityTrayCase,
} from './helpers/activity-tray.js';

describe('activity tray queue and steering PTY stories', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    await testCase?.cleanup();
    testCase = null;
  });

  it('Given three queued messages, when typing and deletion alternate across selected rows, then the prompt remains live', async () => {
    testCase = await launchActivityTrayCase('tray-q3');
    await testCase.typeAndSubmit('start');
    await testCase.waitForStore((state) => state.isProcessing);

    await testCase.sendKeys(CTRL_S);
    await testCase.waitForStore(
      (state) => state.activeInterruptMode === 'queue'
    );
    for (const message of ['first queued', 'second queued', 'third queued']) {
      await testCase.typeAndSubmit(message);
      await testCase.waitForStore((state) =>
        state.queuedMessages.includes(message)
      );
    }

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Messages (3)', 10_000);

    await testCase.sendKeys('a');
    await testCase.waitForStore((state) => state.commandInputValue === 'a');
    await eraseDraft(testCase, 1);
    expect((await testCase.getStore()).queuedMessages).toEqual([
      'first queued',
      'second queued',
      'third queued',
    ]);

    await testCase.sendKeys(BACKSPACE);
    await testCase.waitForStore((state) => state.queuedMessages.length === 2);
    expect((await testCase.getStore()).queuedMessages).toEqual([
      'second queued',
      'third queued',
    ]);

    await testCase.sendKeys('j2k');
    await testCase.waitForStore((state) => state.commandInputValue === 'j2k');
    await eraseDraft(testCase, 3);
    await testCase.sendKeys(SHIFT_DOWN);
    await testCase.sleepMs(100);
    await testCase.sendKeys(BACKSPACE);
    await testCase.waitForStore((state) => state.queuedMessages.length === 1);
    expect((await testCase.getStore()).queuedMessages).toEqual([
      'second queued',
    ]);

    await testCase.sendKeys('z');
    await testCase.waitForStore((state) => state.commandInputValue === 'z');
    await eraseDraft(testCase, 1);
    await testCase.sendKeys(BACKSPACE);
    await testCase.waitForStore((state) => state.queuedMessages.length === 0);

    await testCase.sendKeys('input remains live');
    const state = await testCase.waitForStore(
      (value) => value.commandInputValue === 'input remains live'
    );
    expect(state.activityTrayExpanded).toBe(true);
  }, 40_000);

  it('Given a backend-held steer, when its tray row is deleted after editing a draft, then typing resumes without collapsing the retained state', async () => {
    testCase = await launchActivityTrayCase('tray-steer');
    await testCase.typeAndSubmit('start');
    await testCase.waitForStore((state) => state.isProcessing);

    const steer = 'redirect the active turn';
    await testCase.typeAndSubmit(steer);
    await testCase.mockSessionUpdate({
      type: AgentEventType.SteeringQueued,
      message: steer,
    });
    await testCase.waitForStore((state) => state.pendingSteerContent === steer);

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Messages (1)', 10_000);

    const draft = 'draft before delete';
    await testCase.sendKeys(draft);
    await testCase.waitForStore((state) => state.commandInputValue === draft);
    expect((await testCase.getStore()).pendingSteerContent).toBe(steer);

    await eraseDraft(testCase, draft.length);
    expect((await testCase.getStore()).pendingSteerContent).toBe(steer);

    await testCase.sendKeys(BACKSPACE);
    await testCase.waitForStore((state) => state.pendingSteerContent === null);

    await testCase.sendKeys('typing after steer deletion');
    const state = await testCase.waitForStore(
      (value) => value.commandInputValue === 'typing after steer deletion'
    );
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.queuedMessages).toEqual([]);
  }, 35_000);

  it('Given queued messages, when text and a destructive key arrive in one PTY write, then only the prompt changes', async () => {
    testCase = await launchActivityTrayCase('tray-queue-input-burst');
    await testCase.typeAndSubmit('start');
    await testCase.waitForStore((state) => state.isProcessing);

    await testCase.sendKeys(CTRL_S);
    await testCase.waitForStore(
      (state) => state.activeInterruptMode === 'queue'
    );
    for (const message of ['first queued', 'second queued']) {
      await testCase.typeAndSubmit(message);
      await testCase.waitForStore((state) =>
        state.queuedMessages.includes(message)
      );
    }

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Messages (2)', 10_000);

    await testCase.sendKeys(`a${BACKSPACE}`);
    const erased = await testCase.waitForStore(
      (state) => state.commandInputValue === ''
    );
    expect(erased.queuedMessages).toEqual(['first queued', 'second queued']);

    await testCase.sendKeys('b\r');
    const submitted = await testCase.waitForStore((state) =>
      state.queuedMessages.includes('b')
    );
    expect(submitted.queuedMessages.slice(0, 2)).toEqual([
      'first queued',
      'second queued',
    ]);
    expect(submitted.editingQueueIndex).toBeNull();
  }, 35_000);

  it('Given an expanded Messages tray, when prompt menus open, then overlapping keys affect only the menu', async () => {
    testCase = await launchActivityTrayCase('tray-prompt-menu-ownership');
    await testCase.typeAndSubmit('start');
    await testCase.waitForStore((state) => state.isProcessing);

    await testCase.sendKeys(CTRL_S);
    await testCase.waitForStore(
      (state) => state.activeInterruptMode === 'queue'
    );
    for (const message of ['first queued', 'second queued']) {
      await testCase.typeAndSubmit(message);
      await testCase.waitForStore((state) =>
        state.queuedMessages.includes(message)
      );
    }
    const expectedQueue = ['first queued', 'second queued'];

    await testCase.sendKeys(CTRL_X);
    await testCase.waitForStore((state) => state.activityTrayExpanded);
    await testCase.waitForVisibleText('Messages (2)', 10_000);

    await testCase.sendKeys('/');
    await testCase.waitForStore((state) => state.activeTrigger?.key === '/');
    await testCase.sendKeys(SHIFT_DOWN);
    expect((await testCase.getStore()).activityTraySelectedIndex).toBe(0);
    await testCase.pressEscape();
    let state = await testCase.waitForStore(
      (value) => value.activeTrigger == null
    );
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.queuedMessages).toEqual(expectedQueue);
    expect(state.commandInputValue).toBe('');

    await testCase.mockSessionUpdate({
      type: AgentEventType.PromptsUpdate,
      prompts: [
        {
          name: 'research',
          description: 'Research the workspace',
          arguments: [],
          source: { kind: 'workspace' },
        },
      ],
    });
    await testCase.sendKeys('@r');
    await testCase.waitForStore((value) => value.activeTrigger?.key === '@');
    await testCase.waitForVisibleText('research', 10_000);
    await testCase.sendKeys(SHIFT_DOWN);
    expect((await testCase.getStore()).activityTraySelectedIndex).toBe(0);
    await testCase.pressEscape();
    state = await testCase.waitForStore((value) => value.activeTrigger == null);
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.queuedMessages).toEqual(expectedQueue);
    expect(state.commandInputValue).toBe('@r');
    await testCase.sendKeys('z');
    state = await testCase.waitForStore(
      (value) => value.commandInputValue === '@rz'
    );
    expect(state.queuedMessages).toEqual(expectedQueue);
    for (const expectedDraft of ['@r', '@', '']) {
      await testCase.sendKeys(BACKSPACE);
      state = await testCase.waitForStore(
        (value) =>
          value.commandInputValue === expectedDraft ||
          value.queuedMessages.length !== expectedQueue.length
      );
      expect(state.queuedMessages).toEqual(expectedQueue);
      expect(state.commandInputValue).toBe(expectedDraft);
    }

    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        {
          name: '/review',
          description: 'Review the current change',
          meta: { subcommands: ['first', 'second'] },
        },
      ],
    });
    await testCase.sendKeys('/review');
    await testCase.waitForStore(
      (value) =>
        value.commandInputValue === '/review' &&
        value.activeTrigger?.key === '/'
    );
    await testCase.waitForVisibleText('Review the current change', 10_000);
    await testCase.sleepMs(150);
    await testCase.sendKeys(TAB);
    await testCase.waitForStore(
      (value) => value.activeCommand?.command.name === '/review'
    );
    await testCase.waitForVisibleText('/review first', 10_000);
    await testCase.sleepMs(150);
    await testCase.sendKeys(SHIFT_DOWN);
    expect((await testCase.getStore()).activityTraySelectedIndex).toBe(0);
    await testCase.pressEscape();
    state = await testCase.waitForStore((value) => value.activeCommand == null);
    expect(state.activityTrayExpanded).toBe(true);
    expect(state.queuedMessages).toEqual(expectedQueue);
  }, 40_000);
});
