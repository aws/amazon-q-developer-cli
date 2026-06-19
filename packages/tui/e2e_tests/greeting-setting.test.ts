/**
 * E2E test: chat.greeting.enabled setting controls the welcome screen.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

function expectedWelcomeText(agentEngine: string): string {
  return agentEngine === 'kas'
    ? 'Welcome to the new Kiro CLI'
    : 'An early release of Kiro CLI V3';
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

    await testCase.waitForText(expectedWelcomeText(store.agentEngine), 15000);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  // Windows: CLI settings file path resolution differs (USERPROFILE vs HOME)
  it.skipIf(process.platform === 'win32')('hides welcome screen when chat.greeting.enabled is false', async () => {
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
    const welcomeText = expectedWelcomeText(store.agentEngine);

    // Verify the welcome screen is NOT rendered
    const snapshot = testCase.getSnapshot();
    const hasWelcome = snapshot.some((line) => line.includes(welcomeText));
    expect(hasWelcome).toBe(false);

    // Verify the setting made it into the store
    expect(store.settings!['chat.greeting.enabled']).toBe(false);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});
