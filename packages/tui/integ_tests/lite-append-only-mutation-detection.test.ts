import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from './helpers/integ-lifecycle';

/**
 * Bug-mine 1.1, 1.3, 1.4, 1.5: lite append-only contract — the single owner of
 * the monotonic/exactly-once invariant. Once an item is flushed to twinki's
 * <Static> it NEVER re-renders; a mutation attempt silently drops the row from
 * scrollback. Markers stay in commit order, exactly once each (no trailer
 * re-emission), across subsequent turns.
 */
describe('lite append-only mutation detection [bug-mine 1.1, 1.3, 1.4, 1.5]', () => {
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

  it('append-only: monotonic exactly-once markers, then /chat new clears scrollback (positive control, bug-mine 1.1)', async () => {
    testCase = await launchLiteInteg('lite-append-only-monotonic');

    // /chat new is exercised later in this same session; register the command
    // up front since the mock session doesn't send CommandsUpdate on boot.
    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        { name: '/chat', description: 'Start or switch conversations' },
      ],
    });

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
    // Assert ordering by character offset in the joined scrollback text rather
    // than by line index: under load the lite <Static> flush can render two
    // adjacent (or mock-merged) markers onto the same wrapped row, so distinct
    // line indices aren't guaranteed. Commit order is still preserved
    // left-to-right within the text, and a genuine out-of-order flush would
    // put a later marker's offset before an earlier one's.
    const text = snap.join('\n');
    const posA = text.indexOf('MONOTONIC_A_MARKER');
    const posB = text.indexOf('MONOTONIC_B_MARKER');
    const posC = text.indexOf('MONOTONIC_C_MARKER');
    expect(posA).toBeLessThan(posB);
    expect(posB).toBeLessThan(posC);

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

    // Positive control for the detection harness: /chat new must wipe the
    // committed A/B/C scrollback so we know removal IS observable here.
    const tokenBefore = store.lite.scrollbackClearToken;

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
      storeAfter.lite.scrollbackClearToken <= tokenBefore &&
      Date.now() < deadline
    ) {
      await testCase.sleepMs(200);
      storeAfter = await testCase.getStore();
    }

    // The clear-token bump + empty messages prove the clear mechanism fired.
    expect(storeAfter.lite.scrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.messages.length).toBe(0);

    await pushTurn(
      testCase,
      'content-postclear',
      'AFTER_CLEAR_MARKER_QRS',
      'post-clear'
    );

    const snapAfter = testCase.getSnapshot();
    expect(
      snapAfter.findIndex((l) => l.includes('AFTER_CLEAR_MARKER_QRS'))
    ).not.toBe(-1);

    // Only the new content survives — old content is gone from state.
    const storePostClear = await testCase.getStore();
    const modelMessages = storePostClear.messages.filter(
      (m) => m.role === 'model'
    );
    expect(modelMessages.length).toBe(1);
    const hasOldContent = storePostClear.messages.some(
      (m) => m.role === 'model' && JSON.stringify(m).includes('MONOTONIC_')
    );
    expect(hasOldContent).toBe(false);

    await exitLiteInteg(testCase);
  }, 30000);
});
