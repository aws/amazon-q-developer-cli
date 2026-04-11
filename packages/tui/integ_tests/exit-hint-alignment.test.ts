import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

describe('Exit hint alignment', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('should show exit hint left-aligned below input without /copy hint', async () => {
    testCase = await TestCase.builder()
      .withTestName('exit-hint-alignment')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // Press Ctrl+C on empty input to start exit sequence
    await testCase.pressCtrlC();
    await testCase.waitForVisibleText('Press Ctrl+C or Ctrl+D again to exit');

    const snapshot = testCase.getSnapshot();
    const exitLine = snapshot.find((line) =>
      line.includes('Press Ctrl+C or Ctrl+D again to exit')
    );
    expect(exitLine).toBeDefined();

    // Exit hint should be left-aligned (starts within first few columns)
    const trimmedStart = exitLine!.search(/\S/);
    expect(trimmedStart).toBeLessThan(5);

    // The /copy hint should NOT be visible when exit hint is showing
    const fullScreen = snapshot.join('\n');
    expect(fullScreen).not.toContain('/copy');

    // Clean exit
    await testCase.pressCtrlC();
    await testCase.expectExit();
  }, 20000);
});
