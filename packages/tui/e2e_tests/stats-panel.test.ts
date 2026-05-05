/**
 * E2E tests for /stats panel command.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Stats Panel', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows stats panel on /stats command', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('stats-panel-show')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Type /stats command
    for (const char of '/stats') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showStatsPanel) break;
      await testCase.sleepMs(100);
    }

    const store = await testCase.getStore();
    expect(store.showStatsPanel).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('closes stats panel on Escape', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('stats-panel-escape')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Open stats panel
    for (const char of '/stats') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showStatsPanel) break;
      await testCase.sleepMs(100);
    }

    let store = await testCase.getStore();
    expect(store.showStatsPanel).toBe(true);

    // Press Escape to close
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    await testCase.pressEscape();
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    expect(store.showStatsPanel).toBe(false);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});
