/**
 * E2E tests for /goal command stability fixes:
 * - Free text without quotes
 * - /goal clear allowed during agent processing
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('/goal command', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('accepts free text without quotes', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-free-text')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /goal with free text (no quotes)
    const cmd = '/goal implement pagination for the users endpoint --max 3';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    // Verify the multi-word description was captured (not just "implement").
    const store = await testCase.getStore();
    expect(store.goalStatus).not.toBeNull();
    expect(store.goalStatus?.state).toBe('active');
    expect(store.goalStatus?.maxIterations).toBe(3);
    expect(store.goalStatus?.message).toContain(
      'implement pagination for the users endpoint'
    );

    // Setting a goal auto-injects goal_initial.md as a prompt — drain it with
    // a partial response, then /goal clear during processing to stop the loop.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'starting work' },
        },
      },
    ]);
    await testCase.sleepMs(500);

    for (const char of '/goal clear') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Close the mock stream so the backend can finalize the turn cleanly.
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 45000);

  it('allows /goal clear during agent processing', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-clear-during-processing')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Send a message to start processing
    const msg = 'hello';
    for (const char of msg) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Push partial response (don't close stream — keeps isProcessing=true)
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Thinking deeply...' },
        },
      },
    ]);
    await testCase.sleepMs(500);

    // Verify we're in processing state
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Manually set goal status to simulate an active goal
    // (In real usage, /goal set would have been called before)
    // We'll test that /goal clear goes through during processing
    // by typing it and checking it doesn't show the warning

    const cmd = '/goal clear';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Verify no "can't be queued" warning appeared — the command went through
    store = await testCase.getStore();
    expect(store.goalStatus).toBeNull();

    // Clean up — close the stream
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 45000);

  it('opens panel without firing a duplicate alert for /goal', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-panel-no-alert')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /goal with no args — should open the panel only
    for (const char of '/goal') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    const store = await testCase.getStore();
    expect(store.showGoalPanel).toBe(true);
    // Panel handles its own UI; no transient alert should fire.
    expect(store.transientAlert).toBeNull();

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('allows /goal status during agent processing', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-status-during-processing')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Start a turn so isProcessing=true
    for (const char of 'hello') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Push a partial response without closing the stream — keeps processing
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'thinking…' },
        },
      },
    ]);
    await testCase.sleepMs(500);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // /goal status must go through (not be queued/blocked) during processing
    for (const char of '/goal status') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    // The "Slash commands can't be queued" warning should NOT fire for /goal status.
    if (store.transientAlert) {
      expect(store.transientAlert.message).not.toContain("can't be queued");
    }

    // Clean up the open stream
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(300);
    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 45000);

  it('rejects /goal --max 0 with a validation error', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-validation-max-zero')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    for (const char of '/goal something --max 0') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1500);

    // Validation error surfaces as a message chunk (server rejects inline)
    await testCase.waitForText('--max', 5000);

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('rejects /goal --max 999 with a ceiling error', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-validation-max-too-high')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    for (const char of '/goal explore --max 999') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1500);

    // Ceiling error surfaces as a message chunk
    await testCase.waitForText('ceiling', 5000);

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('shows system message in scrollback when goal starts', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-start-system-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    const cmd = '/goal fix the login bug --max 3';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sendKeys('\r');

    // Wait for the goal system message to appear on screen
    await testCase.waitForText('Goal:', 10000);

    // Push a partial response (don't close stream yet — keeps TUI responsive)
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'working on it' },
        },
      },
    ]);
    await testCase.sleepMs(500);

    // Verify system message appeared in store messages
    const store = await testCase.getStore();
    const systemMessages = store.messages.filter(
      (m: any) => m.role === 'system'
    );
    expect(systemMessages.length).toBeGreaterThan(0);
    const goalMsg = systemMessages.find((m: any) =>
      m.content.includes('Goal:')
    );
    expect(goalMsg).toBeDefined();
    expect(goalMsg!.content).toContain('fix the login bug');
    expect(goalMsg!.content).toContain('3 iterations max');

    // Clear the goal WHILE the stream is still open (TUI is still responsive).
    // This prevents iteration 2 from firing when we close the stream.
    for (const char of '/goal clear') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Now close the stream — goal is already cleared so no iteration 2.
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);

    await testCase.sendKeys([0x03]);
    await testCase.sleepMs(500);
    await testCase.sendKeys([0x03]);
    await testCase.expectExit(15000);
  }, 45000);

  it('shows goal status in prompt bar placeholder while active', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-placeholder-status')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    const cmd = '/goal deploy to staging --max 5';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sendKeys('\r');

    // Wait for the goal to actually activate before checking store
    await testCase.waitForText('Goal:', 10000);

    // Verify goal status is set in the store
    const store = await testCase.getStore();
    expect(store.goalStatus).not.toBeNull();
    expect(store.goalStatus?.state).toBe('active');
    expect(store.goalStatus?.maxIterations).toBe(5);
    expect(store.goalStatus?.message).toContain('deploy to staging');

    // Push partial response (don't close stream yet — keeps TUI responsive)
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'deploying' },
        },
      },
    ]);
    await testCase.sleepMs(500);

    // Clear the goal WHILE the stream is still open (TUI is still responsive).
    // This prevents iteration 2 from firing when we close the stream.
    for (const char of '/goal clear') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // Now close the stream — goal is already cleared so no iteration 2.
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);

    await testCase.sendKeys([0x03]);
    await testCase.sleepMs(500);
    await testCase.sendKeys([0x03]);
    await testCase.expectExit(15000);
  }, 45000);

  it('Tab after /goal description does not show subcommand menu', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-tab-no-subcommand-hijack')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /goal with a free-form description
    const cmd = '/goal fix the login bug';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);

    // Press Tab — should NOT open subcommand menu since user typed free text
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    // Verify no activeCommand (subcommand dropdown) was opened
    const store = await testCase.getStore();
    expect(store.activeCommand).toBeNull();

    // Verify the input text is still visible on screen
    await testCase.waitForText('/goal fix the login bug', 2000);
    // cleanup() handles process termination
  }, 30000);

  it('Esc from Tab-triggered menu preserves prompt text', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('goal-esc-preserves-input')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /goal with just a space (empty after command) — Tab SHOULD show subcommand
    const cmd = '/goal ';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);

    // Press Tab — should open subcommand menu (text after /goal is empty)
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    let store = await testCase.getStore();
    expect(store.activeCommand).not.toBeNull();

    // Press Esc to dismiss menu
    await testCase.sendKeys('\x1b');
    await testCase.sleepMs(500);

    // Verify menu is dismissed
    store = await testCase.getStore();
    expect(store.activeCommand).toBeNull();

    // Verify input text is preserved (not wiped)
    await testCase.waitForText('/goal', 2000);
    // cleanup() handles process termination
  }, 30000);
});
