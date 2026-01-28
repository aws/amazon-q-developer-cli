import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

describe('Basic App Lifecycle', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('should start and exit with ctrl+c twice', async () => {
    testCase = await TestCase.builder()
      .withTestName('basic-lifecycle')
      .withTimeout(15000)
      .launch();

    console.log('TestCase launched, waiting for prompt...');

    // Wait for app to initialize and show prompt
    await testCase.waitForVisibleText('ask a question');

    console.log('Prompt found, typing "hi"...');

    // Type some text
    await testCase.sendKeys('hi');
    await testCase.sleepMs(50); // Allow state to update

    // Verify we have input on screen
    await testCase.waitForVisibleText('hi');
    const initialStore = await testCase.getStore();
    expect(initialStore.messages).toHaveLength(0);
    expect(initialStore.exitSequence).toBe(0);

    console.log('Initial state verified, sending first Ctrl+C (clear input)...');

    // First Ctrl+C - clears input when there's text
    await testCase.sendKeys([0x03]); // Ctrl+C
    await testCase.sleepMs(50);

    console.log('Input cleared, sending second Ctrl+C (start exit sequence)...');

    // Second Ctrl+C - should start exit sequence
    await testCase.sendKeys([0x03]); // Ctrl+C
    await testCase.waitForVisibleText('Press Ctrl+C again to exit');

    console.log('Exit sequence started, sending third Ctrl+C (exit)...');

    // Third Ctrl+C - should exit the process
    await testCase.sendKeys([0x03]); // Ctrl+C

    console.log('Third Ctrl+C sent, waiting for exit...');

    // Wait for process to exit (also saves snapshot)
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);

    console.log('Process exited successfully');
  }, 20000);
});
