/**
 * E2E test: chat.greeting.enabled setting controls the welcome screen.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/**
 * Substrings that prove the welcome screen rendered. Both engines render the
 * shared what's-new body, so its lead line is a stable marker regardless of
 * engine, rollout state, or the rotating tip picked. The KAS welcome also
 * keeps its "Kiro CLI V3" heading.
 */
function welcomeMarkers(agentEngine: string): string[] {
  const shared = ["What's new: Specs", 'kiro.dev/docs/cli/v3'];
  return agentEngine === 'kas' ? ['Kiro CLI V3', ...shared] : shared;
}

function snapshotHasWelcome(snapshot: string[], agentEngine: string): boolean {
  const markers = welcomeMarkers(agentEngine);
  return snapshot.some((line) => markers.some((m) => line.includes(m)));
}

describe('greeting setting', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('shows welcome screen by default', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('greeting-enabled-default')
      .launch();

    const store = await testCase.waitForStoreCondition(
      (s) => Boolean(s.agentEngine),
      5000
    );

    // Welcome shows by default: once the TUI is ready, the welcome area has
    // rendered (a rotating tip for v2, the V3 block for kas).
    await testCase.waitForText('ask a question', 15000);
    const snapshot = testCase.getSnapshot();
    expect(snapshotHasWelcome(snapshot, store.agentEngine)).toBe(true);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  // Windows: CLI settings file path resolution differs (USERPROFILE vs HOME)
  it.skipIf(process.platform === 'win32')(
    'hides welcome screen when chat.greeting.enabled is false',
    async () => {
      testCase = await E2ETestCase.builder()
        .withTestName('greeting-disabled')
        .withGlobalSettings({ 'chat.greeting.enabled': false })
        .launch();

      // Wait for the prompt to appear (TUI is ready)
      await testCase.waitForText('ask a question', 15000);

      const store = await testCase.waitForStoreCondition(
        (s) => s.settings !== null,
        5000
      );

      // Verify the welcome screen is NOT rendered
      const snapshot = testCase.getSnapshot();
      expect(snapshotHasWelcome(snapshot, store.agentEngine)).toBe(false);

      // Verify the setting made it into the store
      expect(store.settings!['chat.greeting.enabled']).toBe(false);

      await testCase.pressCtrlCTwice();
      const exitCode = await testCase.expectExit();
      expect(exitCode).toBe(0);
    },
    30000
  );
});
