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
    // "Tip:" prefix. Selection is random per launch, so capture the rendered
    // tip's own text instead of matching against a hardcoded phrase list
    // (which silently rots every time a tip is added).
    const bootLines = testCase.getSnapshot();
    const boot = bootLines.join('\n');
    expect(boot).not.toContain('Welcome to the new Kiro CLI UX!');
    expect(boot).toContain('Tip:');
    const tipLine = bootLines.find((l) => l.includes('Tip:'))!;
    // A stable fragment of the tip text: after the prefix, trimmed, and short
    // enough to survive line wrapping of the tail.
    const tipFragment = (tipLine.split('Tip:')[1] ?? '').trim().slice(0, 40);
    expect(tipFragment.length).toBeGreaterThan(0);

    // First message flushes the welcome into <Static>. The tip is passed to the
    // Static render too, so it must PERSIST (stay in scrollback) afterwards.
    await testCase.typeAndSubmit('hello');
    await testCase.waitForStore((s) => s.messages.length > 0, 10000);
    await testCase.sleepMs(300);

    const after = testCase.getSnapshot().join('\n');
    expect(after).toContain(tipFragment);

    // Teardown is handled by trackCleanup() (force-kills the PTY in afterEach).
    // We intentionally don't drive a Ctrl+C exit here: a mock turn may still be
    // "processing", and in that state Ctrl+C is routed to cancel-turn rather
    // than the exit sequence (see exitLiteInteg), which would hang teardown.
  }, 60000);
});
