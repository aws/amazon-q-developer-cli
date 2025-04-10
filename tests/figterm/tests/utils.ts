import { describe, expect, test, beforeEach } from '@jest/globals';
import Shell, { PTYOptions } from '../src/shell';
import Config from '../src/config';
import Settings from '../src/settings';

export const makeTestsForShell = (ptyOptions: PTYOptions) => {
  // Each invocation creates one shell, that can be used across multiple tests.
  const shell = new Shell();

  beforeEach(async () => {
    Settings.reset();
    Config.reset();
    await shell.initialize(ptyOptions);
  });

  afterEach(async () => {
    await shell.kill();
  });

  test('shell environment setup', async () => {
    const env = await shell.getEnv();

    expect(env.Q_TERM).toBe('1');
    expect(env.FIG_CHECKED_PROMPTS).toBe('1');
    expect(env.PATH.includes('/.local/bin')).toBe(true);
    expect(env.TTY).not.toBeNull();
    expect(await shell.execute('tty')).toBe(env.TTY);
  });

  describe('figterm', () => {
    beforeEach(async () => {
      shell.resize({ rows: 30, cols: 80 });
      shell.write('\r');
      await shell.waitForNextPrompt();
    });

    test('Type "echo hello world"', async () => {
      await shell.execute('echo hello world!');

      // Type a prefix to make sure autosuggestions don't interfere
      await shell.type('echo hello');
      expect(shell.buffer).toBe('echo hello');
    });

    test('buffer should reset after typing a character', async () => {
      await shell.type(' \b');
      expect(shell.buffer).toBe('');
    });

    test.skip('buffer should be empty on new prompt.', async () => {
      await shell.type('\b');
      expect(shell.buffer).toBe('');
    });

    test.skip('executing basic commands works', async () => {
      const output = await shell.execute('echo hello world');
      expect(output).toBe('hello world');
    });

    test('Resize window (horizontal)', async () => {
      await shell.type('echo testing');
      shell.resize({ rows: 30, cols: 40 });
      await shell.type('11');
      expect(shell.buffer).toBe('echo testing11');
    });

    test('Resize window (vertical)', async () => {
      await shell.type('echo testing');
      shell.resize({ rows: 15, cols: 80 });
      await shell.type('111');
      expect(shell.buffer).toBe('echo testing111');
    });
  });

  describe('figterm', () => {
    test('Works after restarting macos app', async () => {
      await shell.execute('echo hello world!');
      await shell.restartFigtermListener();

      await shell.execute('echo hello');
      await shell.type('abc');
      expect(shell.buffer).toBe('abc');
    });
  });
};
