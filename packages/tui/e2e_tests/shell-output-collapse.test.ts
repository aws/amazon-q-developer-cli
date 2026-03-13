import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Shell Output Collapse', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('collapses long shell output to last 5 lines', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-output-collapse')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    // Generate output exceeding HEAD_LINES(5) + tailLines(max(5, height-10)=14) = 19 lines
    await testCase.sendKeys('!seq 1 30');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for output to start appearing and check for collapse hint while still active
    await testCase.waitForText('lines hidden', 5000);
    await testCase.waitForText('ctrl+o', 2000);

    // Wait for command to fully complete
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('does not collapse short shell output', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-output-no-collapse')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    // Generate output with 3 lines (less than 5)
    await testCase.sendKeys('!seq 1 3');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for command to complete
    await testCase.waitForText('3', 5000);
    
    // Wait a bit for processing
    await testCase.sleepMs(500);

    // Should NOT show collapse hint
    const snapshot = testCase.getSnapshot();
    const snapshotText = snapshot.join('\n');
    expect(snapshotText).not.toContain('lines hidden');
    expect(snapshotText).not.toContain('ctrl+o');
    
    // Should return to prompt
    await testCase.waitForText('ask a question', 5000);
  }, 30000);
});