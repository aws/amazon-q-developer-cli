/**
 * E2E tests for context breakdown feature.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Context Breakdown', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    // Small delay to ensure sockets are fully cleaned up
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows context panel with breakdown on /context command', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('context-breakdown-panel')
      .launch();

    // Wait for TUI to render
    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /context command
    const cmd = '/context';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    // Verify context panel appears with breakdown
    await testCase.waitForText('context left', 10000);
    
    // Verify breakdown categories are shown
    await testCase.waitForText('Agent files', 5000);
    await testCase.waitForText('Tools', 2000);
    await testCase.waitForText('Kiro responses', 2000);
    await testCase.waitForText('Your prompts', 2000);

    console.log('Context Breakdown Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify store state
    const store = await testCase.getStore();
    expect(store.showContextBreakdown).toBe(true);
    expect(store.contextBreakdown).toBeDefined();

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('closes context panel on Escape', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('context-breakdown-escape')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Open context panel
    await testCase.sendKeys('/context');
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    await testCase.waitForText('context left', 10000);

    // Verify panel is open
    let store = await testCase.getStore();
    expect(store.showContextBreakdown).toBe(true);

    // Press Escape to close
    await testCase.sendKeys('\x1b'); // Escape
    await testCase.sleepMs(500);

    // Verify panel is closed
    store = await testCase.getStore();
    expect(store.showContextBreakdown).toBe(false);

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});
