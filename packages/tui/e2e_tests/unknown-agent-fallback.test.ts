/**
 * E2E test: launching with an unknown --agent falls back to kiro_default.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('unknown agent fallback', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('falls back to kiro_default when --agent specifies a nonexistent agent', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('unknown-agent-fallback')
      .withCliArgs('--agent', 'nonexistent-agent')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands();

    const store = await testCase.getStore();
    expect(store.currentAgent?.name).toBe('kiro_default');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});
