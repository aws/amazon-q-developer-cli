import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

/**
 * Bug-mine 1.1, 1.3: Lite append-only contract — positive control tests.
 *
 * The append-only contract says: once an item is flushed to twinki's <Static>,
 * it NEVER re-renders. If something tries to mutate it, twinki silently drops
 * the row from scrollback.
 *
 * These tests form the first positive control layer — they prove the test
 * infrastructure can DETECT both:
 *   (a) content presence after it has been committed to <Static>
 *   (b) content absence after a clear operation (/chat new)
 *
 * This makes the rest of the append-only suite trustworthy: if our tests see
 * content persisting, it's because the contract is holding, not because the
 * test infrastructure can't detect changes.
 */
describe('lite append-only mutation detection [bug-mine 1.1, 1.3]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('content committed to <Static> survives a new turn', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-append-only-survives-turn')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject content for turn 1
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-turn1',
      content: { type: ContentType.Text, text: 'ORIGINAL_CONTENT_ABC123' },
    });
    await testCase.typeAndSubmit('t0');
    await testCase.completeTurn();
    await testCase.sleepMs(500);

    // Verify content is visible after turn 1 completes
    const snap1 = testCase.getSnapshot();
    const originalLine = snap1.findIndex((l) =>
      l.includes('ORIGINAL_CONTENT_ABC123')
    );
    expect(originalLine).not.toBe(-1);

    // Start turn 2 — this forces turn 1's content into committed <Static>
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-turn2',
      content: { type: ContentType.Text, text: 'SECOND_TURN_CONTENT_DEF456' },
    });
    await testCase.typeAndSubmit('t1');
    await testCase.completeTurn();
    await testCase.sleepMs(500);

    // Verify BOTH markers are visible — the original survived the new turn
    const snap2 = testCase.getSnapshot();
    const originalLine2 = snap2.findIndex((l) =>
      l.includes('ORIGINAL_CONTENT_ABC123')
    );
    const secondLine2 = snap2.findIndex((l) =>
      l.includes('SECOND_TURN_CONTENT_DEF456')
    );
    expect(originalLine2).not.toBe(-1);
    expect(secondLine2).not.toBe(-1);
    // Original content still comes before second content (monotonic)
    expect(originalLine2).toBeLessThan(secondLine2);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('staticItems grow monotonically across three turns', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-append-only-monotonic')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // --- Turn 1: marker A ---
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-a',
      content: { type: ContentType.Text, text: 'MONOTONIC_A_MARKER' },
    });
    await testCase.typeAndSubmit('turn1');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // --- Turn 2: marker B ---
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-b',
      content: { type: ContentType.Text, text: 'MONOTONIC_B_MARKER' },
    });
    await testCase.typeAndSubmit('turn2');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // --- Turn 3: marker C ---
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-c',
      content: { type: ContentType.Text, text: 'MONOTONIC_C_MARKER' },
    });
    await testCase.typeAndSubmit('turn3');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // Get final snapshot
    const snap = testCase.getSnapshot();

    // Find line indices for each marker
    const idxA = snap.findIndex((l) => l.includes('MONOTONIC_A_MARKER'));
    const idxB = snap.findIndex((l) => l.includes('MONOTONIC_B_MARKER'));
    const idxC = snap.findIndex((l) => l.includes('MONOTONIC_C_MARKER'));

    // All three markers must be present
    expect(idxA).not.toBe(-1);
    expect(idxB).not.toBe(-1);
    expect(idxC).not.toBe(-1);

    // Monotonic ordering: A < B < C
    expect(idxA).toBeLessThan(idxB);
    expect(idxB).toBeLessThan(idxC);

    // No duplicates of any marker
    const countA = snap.filter((l) => l.includes('MONOTONIC_A_MARKER')).length;
    const countB = snap.filter((l) => l.includes('MONOTONIC_B_MARKER')).length;
    const countC = snap.filter((l) => l.includes('MONOTONIC_C_MARKER')).length;
    expect(countA).toBe(1);
    expect(countB).toBe(1);
    expect(countC).toBe(1);

    // Verify the store's messages array has content entries. Each turn
    // produces at least a user + model message pair, though the mock may
    // merge adjacent content events into fewer model messages.
    const store = await testCase.getStore();
    const contentMessages = store.messages.filter(
      (m) => m.role === 'model' && m.content
    );
    expect(contentMessages.length).toBeGreaterThanOrEqual(2);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('/chat new clears scrollback — positive control for detection (bug-mine 1.1)', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-append-only-clear-positive-ctrl')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject a CommandsUpdate event so the store knows about /chat.
    // Without this, /chat new is treated as a regular chat message since
    // the mock session doesn't send CommandsUpdate on boot like the real
    // backend does.
    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        { name: '/chat', description: 'Start or switch conversations' },
      ],
    });

    // Inject content and complete a turn
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-preclear',
      content: { type: ContentType.Text, text: 'BEFORE_CLEAR_XYZ789' },
    });
    await testCase.typeAndSubmit('pre-clear');
    await testCase.completeTurn();
    await testCase.sleepMs(500);

    // Verify content is visible before the clear
    const snapBefore = testCase.getSnapshot();
    const beforeIdx = snapBefore.findIndex((l) =>
      l.includes('BEFORE_CLEAR_XYZ789')
    );
    expect(beforeIdx).not.toBe(-1);

    // Verify the store has the content
    const storeBefore = await testCase.getStore();
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // Send /chat new — this bumps liteScrollbackClearToken and resets messages.
    // Type char-by-char (like the e2e chat-command test) and press Enter.
    // The CommandMenu intercepts the Enter when it sees a matching command.
    for (const ch of '/chat new') {
      await testCase.sendKeys(ch);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');

    // Wait for the async newSession to resolve
    const deadline = Date.now() + 5000;
    let storeAfter = await testCase.getStore();
    while (
      storeAfter.liteScrollbackClearToken <= tokenBefore &&
      Date.now() < deadline
    ) {
      await testCase.sleepMs(200);
      storeAfter = await testCase.getStore();
    }

    // Verify the store was reset — this IS the positive control.
    // The liteScrollbackClearToken bump proves the clear mechanism fired.
    // The empty messages array proves state was wiped.
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.messages.length).toBe(0);

    // POSITIVE CONTROL: Inject new content after the clear and verify it
    // renders in the terminal independently of old content. This proves
    // our test infrastructure can detect that the application state was
    // reset and new content is being rendered fresh.
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-postclear',
      content: { type: ContentType.Text, text: 'AFTER_CLEAR_MARKER_QRS' },
    });
    await testCase.typeAndSubmit('post-clear');
    await testCase.completeTurn();
    await testCase.sleepMs(500);

    // Verify new content is visible
    const snapAfter = testCase.getSnapshot();
    const postClearIdx = snapAfter.findIndex((l) =>
      l.includes('AFTER_CLEAR_MARKER_QRS')
    );
    expect(postClearIdx).not.toBe(-1);

    // Verify the store has ONLY the new content — old content is gone.
    // This is the core positive control: we can detect that old content
    // was removed from the application state, proving that if our other
    // tests see content persisting, it's because the append-only contract
    // holds, not because we can't detect removal.
    const storePostClear = await testCase.getStore();
    const modelMessages = storePostClear.messages.filter(
      (m) => m.role === 'model'
    );
    expect(modelMessages.length).toBe(1);
    const hasOldContent = storePostClear.messages.some(
      (m) => m.role === 'model' && JSON.stringify(m).includes('BEFORE_CLEAR')
    );
    expect(hasOldContent).toBe(false);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
