import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

async function exitCleanly(tc: E2ETestCase) {
  await tc.sendKeys('\x01'); // Ctrl+A
  await tc.sleepMs(50);
  await tc.sendKeys('\x0b'); // Ctrl+K
  await tc.sleepMs(100);
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}

/** Send a line of text followed by Enter. */
async function submit(tc: E2ETestCase, text: string) {
  await tc.sendKeys(text);
  await tc.sleepMs(80);
  await tc.pressEnter();
  await tc.sleepMs(300);
}

/** Start a turn and leave the stream open so the agent stays processing. */
async function startOpenTurn(tc: E2ETestCase) {
  await tc.pushSendMessageResponse(
    [
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Working on it...' },
        },
      },
    ],
    { silent: true }
  );
  await submit(tc, 'do something slow');
  await tc.waitForStoreCondition((state) => state.isProcessing, 10000);
}

describe('Mid-turn slash commands', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('queues /model mid-turn and dispatches it at turn-end', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-model-queue')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await startOpenTurn(testCase);

    // The command that used to be refused outright.
    await submit(testCase, '/model');

    let store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['/model']);
    // Held, not refused: no rejection warning.
    expect(store.transientAlert?.message ?? '').not.toContain(
      "can't be queued"
    );

    // Close the stream — the turn ends and the drain runs the command.
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(1500);

    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual([]);
    // The model picker is the visible result of the queued command firing.
    expect(store.activeCommand?.command?.name).toBe('/model');

    await exitCleanly(testCase);
  }, 45000);

  it('queues /settings mid-turn and opens it at turn-end', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-settings-queues')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await startOpenTurn(testCase);

    await submit(testCase, '/settings');

    let store = await testCase.getStore();
    expect(store.showSettingsPanel).toBe(false);
    expect(store.queuedMessages).toEqual(['/settings']);
    expect(store.isProcessing).toBe(true);

    await testCase.pushSendMessageResponse(null);
    store = await testCase.waitForStoreCondition(
      (state) => state.showSettingsPanel,
      10000
    );
    expect(store.queuedMessages).toEqual([]);

    await testCase.pressEscape();
    await exitCleanly(testCase);
  }, 45000);

  it('holds an unknown settings subcommand until turn-end', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-unknown-settings-lite')
      .withUiMode('lite')
      .withCliArgs('--trust-tools=shell')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'slow-shell',
            name: 'shell',
            input: JSON.stringify({
              command:
                process.platform === 'win32' ? 'Start-Sleep 5' : 'sleep 5',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await submit(testCase, 'run a slow command');
    await testCase.waitForStoreCondition(
      (state) =>
        state.messages.some(
          (message) =>
            message.role === 'tool_use' &&
            message.id === 'slow-shell' &&
            !message.isFinished
        ),
      10000
    );

    await submit(testCase, '/settings badsub');

    let store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['/settings badsub']);
    expect(
      store.messages.some(
        (message) =>
          message.role === 'system' &&
          message.content.includes('Unknown settings subcommand: badsub')
      )
    ).toBe(false);
    expect(store.isProcessing).toBe(true);
    expect(store.transientAlert).toBeNull();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'TURN_DONE' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.waitForText('TURN_DONE', 10000);
    store = await testCase.waitForStoreCondition(
      (state) =>
        state.messages.some(
          (message) =>
            message.role === 'system' &&
            message.content.includes('Unknown settings subcommand: badsub')
        ),
      10000
    );
    expect(store.queuedMessages).toEqual([]);
    await testCase.waitForText('Unknown settings subcommand: badsub', 10000);
    const screen = testCase.getSnapshot().join('\n');
    expect(screen.split('Unknown settings subcommand: badsub').length - 1).toBe(
      1
    );
    await exitCleanly(testCase);
  }, 45000);

  for (const uiMode of ['tui', 'lite'] as const) {
    it(`closes an immediate command panel without cancelling the turn in ${uiMode}`, async () => {
      testCase = await E2ETestCase.builder()
        .withTestName(`mid-turn-help-escape-${uiMode}`)
        .withUiMode(uiMode)
        .launch();

      await testCase.waitForText('ask a question', 10000);
      await testCase.getSessionId();
      await testCase.waitForSlashCommands();
      await startOpenTurn(testCase);

      await submit(testCase, '/help');
      let store = await testCase.waitForStoreCondition(
        (state) => state.showHelpPanel,
        10000
      );
      expect(store.isProcessing).toBe(true);

      await testCase.pressEscape();
      store = await testCase.waitForStoreCondition(
        (state) => !state.showHelpPanel,
        10000
      );
      expect(store.isProcessing).toBe(true);

      await testCase.pushSendMessageResponse(null);
      await exitCleanly(testCase);
    }, 45000);
  }

  it('pauses queue draining while a queued Settings panel is open', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-settings-pauses-queue')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.waitForSlashCommands();
    await startOpenTurn(testCase);

    await submit(testCase, '/settings');
    await submit(testCase, '/model');

    let store = await testCase.getStore();
    expect(store.showSettingsPanel).toBe(false);
    expect(store.queuedMessages).toEqual(['/settings', '/model']);
    expect(store.isProcessing).toBe(true);

    await testCase.pushSendMessageResponse(null);
    store = await testCase.waitForStoreCondition(
      (state) => state.showSettingsPanel,
      10000
    );
    expect(store.showSettingsPanel).toBe(true);
    expect(store.queuedMessages).toEqual(['/model']);

    await testCase.sleepMs(500);
    store = await testCase.getStore();
    expect(store.activeCommand).toBeNull();
    expect(store.queuedMessages).toEqual(['/model']);

    await testCase.pressEscape();
    store = await testCase.waitForStoreCondition(
      (state) => state.activeCommand?.command?.name === '/model',
      10000
    );
    expect(store.queuedMessages).toEqual([]);

    await testCase.pressEscape();
    await exitCleanly(testCase);
  }, 45000);

  it('runs a backend read-only command while the turn remains open', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-context-show-runs')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.waitForSlashCommands();
    await startOpenTurn(testCase);

    await submit(testCase, '/context show');

    const store = await testCase.waitForStoreCondition(
      (state) => state.showContextBreakdown,
      10000
    );
    expect(store.queuedMessages).toEqual([]);
    expect(store.isProcessing).toBe(true);

    await testCase.pushSendMessageResponse(null);
    await exitCleanly(testCase);
  }, 45000);

  it('refuses an unknown command without sending or queueing it', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-unknown-command-refused')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await startOpenTurn(testCase);

    await submit(testCase, '/foozle');

    const store = await testCase.getStore();
    expect(store.transientAlert?.message).toContain(
      'Unrecognized command: /foozle'
    );
    expect(store.queuedMessages).toEqual([]);
    expect(store.pendingSteerContent ?? '').not.toContain('/foozle');

    await testCase.pushSendMessageResponse(null);
    await exitCleanly(testCase);
  }, 45000);

  it('does not send a queued slash command to the model as text', async () => {
    // The failure this guards: routing "/model" through steering would deliver
    // it to the agent as a prompt, wasting a turn and never opening the picker.
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-no-leak')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await startOpenTurn(testCase);

    await submit(testCase, '/model');

    const store = await testCase.getStore();
    expect(store.pendingSteerContent ?? '').not.toContain('/model');
    expect(store.queuedMessages).toEqual(['/model']);

    await testCase.pushSendMessageResponse(null);
    await exitCleanly(testCase);
  }, 45000);

  it('forwards a pasted path mid-turn instead of refusing it', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-turn-pasted-path')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await startOpenTurn(testCase);

    await submit(testCase, '/some/file/path');

    const store = await testCase.getStore();
    expect(store.transientAlert?.message ?? '').not.toContain(
      "can't be queued"
    );
    // Not a command, so it must not occupy the slash-command queue.
    expect(store.queuedMessages).toEqual([]);

    // The steered text feeds a fresh request that the mock holds open, so the
    // turn never ends on its own. Cancel it first: Ctrl+C while processing
    // cancels rather than quitting, which would hang teardown.
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);
    await testCase.sendKeys('\x03');
    await testCase.sleepMs(1000);
    await exitCleanly(testCase);
  }, 45000);
});
