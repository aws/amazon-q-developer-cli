import { afterEach, describe, it } from 'bun:test';
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
    await testCase.waitForText('hello_shell', 10000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 10000);
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
    await testCase.waitForText('/', 10000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 10000);
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
    await testCase.waitForText('ask a question', 10000);
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
    await testCase.waitForText('ask a question', 10000);
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
    await testCase.waitForText('done', 10000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 10000);
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
    await testCase.waitForText('hello world', 10000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 10000);
  }, 30000);

  it('accepts interactive input via read', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-interactive')
      .withTerminal({ width: 120, height: 30 })
      .launch();
    await testCase.waitForText('ask a question', 15000);

    // Use bash read to prompt for input and echo it back
    await testCase.sendKeys('!read -p "Name: " name && echo "Hello $name"');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Should see the prompt from read
    await testCase.waitForText('Name:', 20000);

    // Type the response
    await testCase.sendKeys('Kiro');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Should see the echoed greeting
    await testCase.waitForText('Hello Kiro', 20000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 15000);
  }, 60000);

  it('Ctrl-C cancels a running shell escape command', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-ctrlc')
      .withTerminal({ width: 120, height: 30 })
      .launch();
    await testCase.waitForText('ask a question', 15000);

    // Start a long-running command
    await testCase.sendKeys('!sleep 30');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Wait for the command to start running
    await testCase.sleepMs(2000);

    // Press Ctrl+C to cancel
    await testCase.pressCtrlC();

    // Should return to prompt (not exit Kiro)
    await testCase.waitForText('ask a question', 20000);
  }, 60000);

  it('accepts multiple lines of interactive input', async () => {
    // Use a wider terminal to prevent the long command from wrapping and
    // interfering with prompt detection in the xterm screen buffer.
    testCase = await E2ETestCase.builder()
      .withTestName('shell-escape-multi-input')
      .withTerminal({ width: 120, height: 30 })
      .launch();
    await testCase.waitForText('ask a question', 15000);

    // Use variables for prompt strings so the literal prompt text we wait for
    // doesn't appear in the typed command (which stays visible on screen and
    // would cause waitForText to match prematurely in slow CI environments).
    await testCase.sendKeys('!P=Prompt; read -p "${P}1: " a && read -p "${P}2: " b && echo "$a and $b"');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // First prompt -- "Prompt1:" only appears when the first read actually runs.
    // Allow extra time for the PTY chain to settle (TUI -> Bun.Terminal -> bash).
    await testCase.waitForText('Prompt1:', 20000);
    await testCase.sendKeys('foo');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Second prompt -- "Prompt2:" only appears when the second read runs
    await testCase.waitForText('Prompt2:', 20000);
    await testCase.sendKeys('bar');
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Should see combined output
    await testCase.waitForText('foo and bar', 20000);

    // Should return to prompt
    await testCase.waitForText('ask a question', 15000);
  }, 60000);
});
