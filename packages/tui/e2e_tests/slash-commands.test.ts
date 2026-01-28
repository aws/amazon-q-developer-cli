/**
 * E2E tests for slash command execution.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Slash Commands', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('executes /clear command', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-clear')
      .launch();

    // Wait for TUI to render
    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type /clear command
    const cmd = '/clear';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    
    await testCase.sendKeys('\r');
    await testCase.sleepMs(500);

    // /clear is a local command that should work without backend
    // Just verify the TUI is still responsive
    const store = await testCase.getStore();
    expect(store.messages).toBeDefined();

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('shows autocomplete dropdown when typing /', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-autocomplete')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(500);

    // Type just "/" without pressing enter
    await testCase.sendKeys('/');
    await testCase.sleepMs(500);

    // Verify autocomplete dropdown shows /model
    await testCase.waitForText('/model', 5000);

    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());
  }, 30000);

  it('shows selection UI for /model without args', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-model-sel')
      .launch();

    // Wait for TUI to render
    await testCase.waitForText('ask a question', 10000);
    await testCase.sleepMs(5000);

    // Type /model command
    await testCase.sendKeys('/model');
    await testCase.sleepMs(200);
    
    // Verify autocomplete menu appears
    await testCase.waitForText('Select or list available models', 5000);
    
    // Press Enter to execute command
    await testCase.pressEnter();
    await testCase.sleepMs(1000);
    
    // Verify model selection list appears with multiple models
    await testCase.waitForText('Auto', 10000);
    await testCase.waitForText('Claude Sonnet 4.5', 5000);
    await testCase.waitForText('Claude Opus 4.5', 2000);
    await testCase.waitForText('Claude Haiku 4.5', 2000);
  }, 30000);
});
