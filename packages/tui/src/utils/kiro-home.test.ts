import { describe, test, expect, afterEach } from 'bun:test';
import { homedir } from 'os';
import { join } from 'path';

import { kiroHomeDir, kiroHomePath } from './kiro-home.js';

/**
 * These tests mutate `process.env.KIRO_HOME` and must therefore run
 * sequentially (Bun's default test runner is single-threaded per file, so
 * intra-file ordering is deterministic). We restore the previous value after
 * each test so other test files that run later see the original environment.
 */
describe('kiroHomeDir', () => {
  const originalKiroHome = process.env.KIRO_HOME;

  afterEach(() => {
    if (originalKiroHome === undefined) {
      delete process.env.KIRO_HOME;
    } else {
      process.env.KIRO_HOME = originalKiroHome;
    }
  });

  test('falls back to $HOME/.kiro when KIRO_HOME is unset', () => {
    delete process.env.KIRO_HOME;
    const home = process.env.HOME || process.env.USERPROFILE || homedir();
    expect(kiroHomeDir()).toBe(join(home, '.kiro'));
  });

  test('falls back to $HOME/.kiro when KIRO_HOME is empty', () => {
    process.env.KIRO_HOME = '';
    const home = process.env.HOME || process.env.USERPROFILE || homedir();
    expect(kiroHomeDir()).toBe(join(home, '.kiro'));
  });

  test('returns KIRO_HOME verbatim when set', () => {
    process.env.KIRO_HOME = '/custom/kiro';
    expect(kiroHomeDir()).toBe('/custom/kiro');
  });

  test('kiroHomePath joins segments under the home dir', () => {
    process.env.KIRO_HOME = '/custom/kiro';
    expect(kiroHomePath('settings', 'cli.json')).toBe(
      join('/custom/kiro', 'settings', 'cli.json')
    );
  });

  test('KIRO_HOME override works on platform-native absolute paths', () => {
    // Use a posix-style path under both platforms — `join` normalizes to
    // native separators, which keeps this test portable.
    process.env.KIRO_HOME = '/opt/team/kiro';
    expect(kiroHomePath('agents')).toBe(join('/opt/team/kiro', 'agents'));
  });
});
