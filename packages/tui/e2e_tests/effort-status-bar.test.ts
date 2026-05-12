/**
 * E2E tests for effort level display in the TUI status bar.
 * Covers: default effort on init, /effort change, /model switch reset.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Effort Status Bar', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows default effort level on session start (claude 4.7 → xhigh)', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('effort-default-init')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();

    // Wait for metadata notification to set effort
    const store = await testCase.waitForStoreCondition(
      (s) => s.currentEffort !== null,
      10000
    );
    expect(store.currentEffort).toBe('xhigh');
  }, 30000);

  it('/effort command changes effort and updates store', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('effort-command-change')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Wait for initial effort to be set
    await testCase.waitForStoreCondition((s) => s.currentEffort !== null, 10000);

    // Type /effort and press Enter to open selection menu
    await testCase.sendKeys('/effort');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Verify selection menu shows effort options
    await testCase.waitForText('low', 5000);

    // Select "low" (first in the list)
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Verify store updated via metadata notification
    const store = await testCase.waitForStoreCondition(
      (s) => s.currentEffort === 'low',
      5000
    );
    expect(store.currentEffort).toBe('low');
  }, 30000);

  it('/model switch resets effort to new model default', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('effort-model-switch-reset')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Wait for initial effort (xhigh for claude-opus-4.7)
    await testCase.waitForStoreCondition((s) => s.currentEffort === 'xhigh', 10000);

    // Switch to claude-sonnet-4.6 via /model command
    await testCase.sendKeys('/model claude-sonnet-4.6');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Effort should reset to "high" (4.6 default)
    const store = await testCase.waitForStoreCondition(
      (s) => s.currentEffort === 'high',
      5000
    );
    expect(store.currentEffort).toBe('high');
  }, 30000);
});
