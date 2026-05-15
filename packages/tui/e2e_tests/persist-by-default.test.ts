/**
 * E2E tests verifying /model and /effort persist-by-default behavior.
 * Checks that success messages include "saved" wording.
 */

import { afterEach, describe, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Persist by default', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('/model switch shows "saved as default" in snackbar', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('model-persist-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Switch model
    await testCase.sendKeys('/model claude-sonnet-4.6');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Verify "saved" appears in the notification
    await testCase.waitForText('saved as default', 5000);
  }, 30000);

  it('/effort shows "saved" in snackbar', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('effort-persist-message')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Wait for initial effort to be set
    await testCase.waitForStoreCondition((s) => s.currentEffort !== null, 10000);

    // Type /effort low directly
    await testCase.sendKeys('/effort low');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Verify "saved" appears in the notification
    await testCase.waitForText('saved', 5000);
  }, 30000);
});
