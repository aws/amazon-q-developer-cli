import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Bug-mine 1.1, 1.3: lite append-only contract. Once an item is flushed to
 * twinki's <Static> it NEVER re-renders; a mutation attempt silently drops the
 * row from scrollback.
 */
describe('lite append-only mutation detection [bug-mine 1.1, 1.3]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  /** Inject a Content event with `marker`, submit `prompt`, complete + settle. */
  async function pushTurn(
    tc: TestCase,
    id: string,
    marker: string,
    prompt: string
  ): Promise<void> {
    await tc.mockSessionUpdate({
      type: AgentEventType.Content,
      id,
      content: { type: ContentType.Text, text: marker },
    });
    await tc.typeAndSubmit(prompt);
    await tc.completeTurn();
    await tc.sleepMs(400);
  }

  it('committed content survives later turns: markers grow monotonically, exactly once each', async () => {
    testCase = await launchLiteInteg('lite-append-only-monotonic');

    await pushTurn(testCase, 'content-a', 'MONOTONIC_A_MARKER', 'turn1');

    // Marker A must already be on screen before turn 2 commits it to <Static>.
    expect(
      testCase.getSnapshot().findIndex((l) => l.includes('MONOTONIC_A_MARKER'))
    ).not.toBe(-1);

    await pushTurn(testCase, 'content-b', 'MONOTONIC_B_MARKER', 'turn2');
    await pushTurn(testCase, 'content-c', 'MONOTONIC_C_MARKER', 'turn3');

    const snap = testCase.getSnapshot();
    const idxA = snap.findIndex((l) => l.includes('MONOTONIC_A_MARKER'));
    const idxB = snap.findIndex((l) => l.includes('MONOTONIC_B_MARKER'));
    const idxC = snap.findIndex((l) => l.includes('MONOTONIC_C_MARKER'));

    expect(idxA).not.toBe(-1);
    expect(idxB).not.toBe(-1);
    expect(idxC).not.toBe(-1);

    // Append-only contract: markers stay in commit order, exactly once each.
    expect(idxA).toBeLessThan(idxB);
    expect(idxB).toBeLessThan(idxC);

    const countA = snap.filter((l) => l.includes('MONOTONIC_A_MARKER')).length;
    const countB = snap.filter((l) => l.includes('MONOTONIC_B_MARKER')).length;
    const countC = snap.filter((l) => l.includes('MONOTONIC_C_MARKER')).length;
    expect(countA).toBe(1);
    expect(countB).toBe(1);
    expect(countC).toBe(1);

    // The mock may merge adjacent content events into fewer model messages.
    const store = await testCase.getStore();
    const contentMessages = store.messages.filter(
      (m) => m.role === 'model' && m.content
    );
    expect(contentMessages.length).toBeGreaterThanOrEqual(2);

    await exitLiteInteg(testCase);
  }, 30000);

  it('/chat new clears scrollback — positive control for detection (bug-mine 1.1)', async () => {
    testCase = await launchLiteInteg('lite-append-only-clear-positive-ctrl');

    // Mock session doesn't send CommandsUpdate on boot; without this /chat new
    // is treated as a regular chat message rather than a command.
    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        { name: '/chat', description: 'Start or switch conversations' },
      ],
    });

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-preclear',
      content: { type: ContentType.Text, text: 'BEFORE_CLEAR_XYZ789' },
    });
    await testCase.typeAndSubmit('pre-clear');
    await testCase.completeTurn();
    await testCase.sleepMs(500);

    const snapBefore = testCase.getSnapshot();
    const beforeIdx = snapBefore.findIndex((l) =>
      l.includes('BEFORE_CLEAR_XYZ789')
    );
    expect(beforeIdx).not.toBe(-1);

    const storeBefore = await testCase.getStore();
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // Type char-by-char so CommandMenu intercepts Enter as the /chat command.
    for (const ch of '/chat new') {
      await testCase.sendKeys(ch);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');

    // Wait for the async newSession to resolve.
    const deadline = Date.now() + 5000;
    let storeAfter = await testCase.getStore();
    while (
      storeAfter.liteScrollbackClearToken <= tokenBefore &&
      Date.now() < deadline
    ) {
      await testCase.sleepMs(200);
      storeAfter = await testCase.getStore();
    }

    // Positive control: the clear-token bump + empty messages prove the clear
    // mechanism fired and state was wiped (so this harness CAN detect removal).
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.messages.length).toBe(0);

    await pushTurn(
      testCase,
      'content-postclear',
      'AFTER_CLEAR_MARKER_QRS',
      'post-clear'
    );

    const snapAfter = testCase.getSnapshot();
    const postClearIdx = snapAfter.findIndex((l) =>
      l.includes('AFTER_CLEAR_MARKER_QRS')
    );
    expect(postClearIdx).not.toBe(-1);

    // Only the new content survives — old content is gone from state.
    const storePostClear = await testCase.getStore();
    const modelMessages = storePostClear.messages.filter(
      (m) => m.role === 'model'
    );
    expect(modelMessages.length).toBe(1);
    const hasOldContent = storePostClear.messages.some(
      (m) => m.role === 'model' && JSON.stringify(m).includes('BEFORE_CLEAR')
    );
    expect(hasOldContent).toBe(false);

    await exitLiteInteg(testCase);
  }, 30000);
});
