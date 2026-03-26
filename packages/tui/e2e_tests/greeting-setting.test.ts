/**
 * E2E test: chat.greeting.enabled setting controls the welcome screen.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

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

    await testCase.waitForText('Welcome to the new Kiro CLI', 15000);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  it('hides welcome screen when chat.greeting.enabled is false', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('greeting-disabled')
      .withGlobalSettings({ 'chat.greeting.enabled': false })
      .launch();

    // Wait for the prompt to appear (TUI is ready)
    await testCase.waitForText('ask a question', 15000);

    // Verify the welcome screen is NOT rendered
    const snapshot = testCase.getSnapshot();
    const hasWelcome = snapshot.some((line) =>
      line.includes('Welcome to the new Kiro CLI')
    );
    expect(hasWelcome).toBe(false);

    // Verify the setting made it into the store
    const store = await testCase.waitForStoreCondition(
      (s) => s.settings !== null,
      5000
    );
    expect(store.settings!['chat.greeting.enabled']).toBe(false);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});
