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

    // Verify description hint shows on the same line as /effort in prompt bar
    await testCase.waitForText('Set thinking effort for this session', 5000);
    const lines = testCase.getSnapshot();
    const hintLine = lines.find(l => l.includes('Set thinking effort'));
    expect(hintLine).toBeDefined();
    expect(hintLine!).toContain('/effort');

    // Verify selection menu shows effort options with [active] marker
    await testCase.waitForText('xHigh  [active]', 5000);
    await testCase.waitForText('Low', 5000);

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

  it('/effort shows error when model does not support effort', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('effort-unsupported-model')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Switch to a model without effort support
    await testCase.sendKeys('/model amazon-nova-pro');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Try /effort — should show error since model has no schema
    await testCase.sendKeys('/effort');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    await testCase.waitForText('not available on Amazon Nova Pro', 5000);
  }, 30000);

  // Windows CI timing: model switch + effort reset sequence exceeds timeout
  it.skipIf(process.platform === 'win32')('/model switch resets effort to new model default', async () => {
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
