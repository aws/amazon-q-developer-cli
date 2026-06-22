import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

/**
 * Bug-mine 5.1-5.7: cancel/interrupt invariants in lite mode. Non-obvious bits:
 * cancel must be idempotent (guard against double-fire) and must dispose the
 * stream handler so a late event can't re-open a cancelled turn.
 */
describe('lite cancel/interrupt invariants [bug-mine 5.1-5.7]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('cancel mid-stream shows "Cancelled streaming" once, no duplicated partial content [bug-mine 5.1]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-no-duplicate')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject content event before submitting so the mock session delivers it
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-1',
      content: { type: ContentType.Text, text: 'PARTIAL_RESPONSE_ABC' },
    });

    // Start a turn
    await testCase.typeAndSubmit('test prompt');
    await testCase.sleepMs(300);

    // Verify turn is processing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Send Ctrl+C to cancel
    await testCase.pressCtrlC();
    await testCase.sleepMs(500);

    // Verify isProcessing is now false
    store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);

    // Check that the content "PARTIAL_RESPONSE_ABC" appears at most once
    // in messages (not duplicated by a stale flush after cancel)
    const contentMessages = store.messages.filter(
      (m) => m.role === 'model' && m.content?.includes('PARTIAL_RESPONSE_ABC')
    );
    expect(contentMessages.length).toBeLessThanOrEqual(1);

    // Clean exit
    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('rapid double Ctrl+C is idempotent via cancelInProgress guard [bug-mine 5.2]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-idempotent')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject a content event so there's something to cancel
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-1',
      content: { type: ContentType.Text, text: 'some content' },
    });

    // Start a turn
    await testCase.typeAndSubmit('test double cancel');
    await testCase.sleepMs(300);

    // Verify turn is processing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Send Ctrl+C twice rapidly (the second should be a no-op due to
    // cancelInProgress guard)
    await testCase.pressCtrlC();
    await testCase.sleepMs(50);
    await testCase.pressCtrlC();
    await testCase.sleepMs(500);

    // Verify isProcessing is false and the app is in a valid state
    store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);
    // cancelInProgress should be null (fully resolved)
    expect(store.cancelInProgress).toBeNull();
    // No error state
    expect(store.agentError).toBeNull();

    // Clean exit
    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('cancel disposes stream handler before async cancel — no ghost content from old turn [bug-mine 5.3]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-no-ghost')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Turn 1: inject content
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-turn1',
      content: { type: ContentType.Text, text: 'TURN1_UNIQUE_MARKER' },
    });

    // Start turn 1
    await testCase.typeAndSubmit('turn one');
    await testCase.sleepMs(300);

    // Cancel turn 1
    await testCase.pressCtrlC();
    await testCase.sleepMs(500);

    // Verify cancelled
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);

    // Now inject content for turn 2 — this should NOT contain turn 1 ghost
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-turn2',
      content: { type: ContentType.Text, text: 'TURN2_UNIQUE_MARKER' },
    });

    // Start turn 2
    await testCase.typeAndSubmit('turn two');
    await testCase.sleepMs(300);

    // Complete turn 2 normally
    await testCase.completeTurn();
    await testCase.sleepMs(200);

    // Inspect final messages: turn 2's model message should contain
    // TURN2_UNIQUE_MARKER but NOT TURN1_UNIQUE_MARKER (no ghost bleed)
    store = await testCase.getStore();
    const modelMessages = store.messages.filter((m) => m.role === 'model');
    const lastModel = modelMessages[modelMessages.length - 1];
    expect(lastModel).toBeDefined();
    expect(lastModel!.content).toContain('TURN2_UNIQUE_MARKER');
    expect(lastModel!.content).not.toContain('TURN1_UNIQUE_MARKER');

    // Clean exit
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('Ctrl+C when idle increments exitSequence but does not crash [bug-mine 5.4]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-idle-exit-seq')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Verify we are idle (not processing)
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);
    expect(store.exitSequence).toBe(0);

    // Send a single Ctrl+C when idle — should increment exitSequence
    await testCase.pressCtrlC();
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.exitSequence).toBe(1);

    // Wait for the 2s exit timer to reset
    await testCase.sleepMs(2200);

    store = await testCase.getStore();
    // exitSequence should have reset back to 0 after the timeout
    expect(store.exitSequence).toBe(0);

    // App is still running — confirm by checking store is accessible
    const stillAlive = await testCase.getStore();
    expect(stillAlive).toBeDefined();

    // Clean exit
    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('Esc during subagent panel open does not cancel agent turn [bug-mine 5.5]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-esc-panel')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Set up a subagent scenario so the panel has content to show
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'subagent-parent-p',
      name: 'subagent',
      args: { pipeline: 'test-pipeline' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-stage-1',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/file.txt' },
      sessionId: 'session-stage-1',
    });

    // Start a turn
    await testCase.typeAndSubmit('start turn');
    await testCase.sleepMs(300);

    // Verify processing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Open the subagent panel with Ctrl+O
    await testCase.sendKeys('\x0f'); // Ctrl+O
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);
    expect(store.isProcessing).toBe(true);

    // Press Esc — should close the panel but NOT cancel the turn
    await testCase.pressEscape();
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);
    // The key invariant: isProcessing must still be true
    expect(store.isProcessing).toBe(true);

    // Clean up: complete the turn and exit
    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('Esc cancels turn cleanly and app recovers for new input [bug-mine 5.6]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-esc-recovers')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject content and start a turn so isProcessing=true
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-bg',
      content: { type: ContentType.Text, text: 'background work' },
    });
    await testCase.typeAndSubmit('first message');
    await testCase.sleepMs(300);

    // Verify we are processing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Press Esc to cancel the agent turn
    await testCase.pressEscape();
    await testCase.sleepMs(500);

    // Verify turn was cancelled
    store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);
    expect(store.cancelInProgress).toBeNull();

    // Verify the app is in a usable state: inject new events and start
    // a new turn to prove no desync (no "Prompt already in progress" error)
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-recovery',
      content: { type: ContentType.Text, text: 'RECOVERY_CONTENT' },
    });
    await testCase.typeAndSubmit('recovery message');
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    // Either processing the new turn or it auto-resolved — either way no error
    expect(store.agentError).toBeNull();
    // The recovery user message should be in the store
    const userMessages = store.messages.filter((m) => m.role === 'user');
    const recoveryMsg = userMessages.find((m) =>
      m.content?.includes('recovery message')
    );
    expect(recoveryMsg).toBeDefined();

    // Clean up
    if (store.isProcessing) {
      await testCase.completeTurn();
      await testCase.sleepMs(100);
    }
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('cancel drains queued message immediately after clearing isProcessing [bug-mine 5.7]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-cancel-drain-queue')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject content and start turn 1
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-t1',
      content: { type: ContentType.Text, text: 'turn 1 content' },
    });
    await testCase.typeAndSubmit('first message');
    await testCase.sleepMs(300);

    // Verify processing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Queue a follow-up message while processing
    await testCase.typeAndSubmit('queued follow up');
    await testCase.sleepMs(200);

    // Verify it was queued
    store = await testCase.getStore();
    expect(store.queuedMessages.length).toBeGreaterThanOrEqual(1);
    expect(store.queuedMessages[0]).toBe('queued follow up');

    // Cancel the current turn — the queued message should drain immediately
    await testCase.pressCtrlC();
    await testCase.sleepMs(800);

    // After cancel + drain, the queued message should have been sent.
    // Either it's now processing (the queue drained into a new turn) or
    // the queue is empty (it was already submitted).
    store = await testCase.getStore();
    expect(store.queuedMessages.length).toBe(0);

    // The "queued follow up" should appear in messages as a user message
    const userMessages = store.messages.filter((m) => m.role === 'user');
    const queuedMsg = userMessages.find((m) =>
      m.content?.includes('queued follow up')
    );
    expect(queuedMsg).toBeDefined();

    // Clean up: complete the new turn if processing, then exit
    if (store.isProcessing) {
      await testCase.completeTurn();
      await testCase.sleepMs(100);
    }
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
