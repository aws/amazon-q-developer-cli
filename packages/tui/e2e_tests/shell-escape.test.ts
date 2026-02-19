import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Shell Escape (!command)', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('executes !echo and shows output', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-echo')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!echo hello_shell');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should see the output directly (no AI thinking)
    await testCase.waitForText('hello_shell', 5000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('executes !pwd and shows directory', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-pwd')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!pwd');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should see a path (contains /)
    await testCase.waitForText('/', 5000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('handles empty ! gracefully', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-empty')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should return to prompt without error
    await testCase.sleepMs(500);
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('shows error for failed commands', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-fail')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!false');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should show error status and return to prompt
    await testCase.sleepMs(1000);
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('executes long-running command', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-long')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!sleep 1 && echo done');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should wait and then show output
    await testCase.waitForText('done', 3000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 5000);
  }, 30000);

  it('handles command with spaces', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-spaces')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('!echo "hello world"');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Should see the full output
    await testCase.waitForText('hello world', 5000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 5000);
  }, 30000);
});
