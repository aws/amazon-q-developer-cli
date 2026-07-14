import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { trackCleanup } from './helpers/integ-lifecycle';

/**
 * The full TUI shows one rotating startup tip (with a "Tip:" prefix) below the
 * KIRO welcome (see src/tips/tips.ts + ConversationView). The tip is passed to
 * both the live welcome AND the <Static> re-render, so it must:
 *   1. render on a fresh boot, and
 *   2. PERSIST into scrollback after the first message (not vanish).
 *
 * Boots in mock mode with chat.ui.mode='tui' (the default UI), so no rollout
 * env is needed: the featured v3/Lite tips are ineligible, so a plain SHARED
 * tip is shown — which is exactly what proves the "Tip:" surface renders.
 */
describe('TUI welcome rotating tip', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  it('renders a tip on boot and keeps it in scrollback after the first message', async () => {
    testCase = await TestCase.builder()
      .withTestName('tui-welcome-tip')
      .withGlobalSettings({ 'chat.ui.mode': 'tui' })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Boot: the old static greeting is gone, replaced by a rotating tip with a
    // "Tip:" prefix. With no rollout env the featured v3/Lite tips are
    // ineligible, so one of the plain SHARED/TUI tips renders — assert on
    // stable, slash-free fragments.
    const boot = testCase.getSnapshot().join('\n');
    expect(boot).not.toContain('Welcome to the new Kiro CLI UX!');
    expect(boot).toContain('Tip:');
    const KNOWN_TIP_PHRASES = [
      'Choose which layout opens',
      'Switch between Auto, Dark, Light',
      'interrupt the current turn',
      'Share your thoughts anytime',
      'expand a tool',
      'tune truncation, output filters',
    ];
    const shown = KNOWN_TIP_PHRASES.filter((p) => boot.includes(p));
    expect(shown.length).toBeGreaterThanOrEqual(1);

    // First message flushes the welcome into <Static>. The tip is passed to the
    // Static render too, so it must PERSIST (stay in scrollback) afterwards.
    await testCase.typeAndSubmit('hello');
    await testCase.waitForStore((s) => s.messages.length > 0, 10000);
    await testCase.sleepMs(300);

    const after = testCase.getSnapshot().join('\n');
    for (const phrase of shown) expect(after).toContain(phrase);

    // Teardown is handled by trackCleanup() (force-kills the PTY in afterEach).
    // We intentionally don't drive a Ctrl+C exit here: a mock turn may still be
    // "processing", and in that state Ctrl+C is routed to cancel-turn rather
    // than the exit sequence (see exitLiteInteg), which would hang teardown.
  }, 60000);
});
