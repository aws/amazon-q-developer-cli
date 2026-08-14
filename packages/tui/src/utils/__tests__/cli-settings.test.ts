import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  readCliSettings,
  readBoolSetting,
  readOptionalStringSetting,
  writeCliSettings,
  updateCliSetting,
  toggleBoolSetting,
} from '../cli-settings.js';

let testDir: string;
let originalHome: string | undefined;

function cliJsonPath(): string {
  return join(testDir, '.kiro', 'settings', 'cli.json');
}

function writeCliJson(data: Record<string, unknown>): void {
  const dir = join(testDir, '.kiro', 'settings');
  mkdirSync(dir, { recursive: true });
  writeFileSync(cliJsonPath(), JSON.stringify(data, null, 2), 'utf-8');
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `cli-settings-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  originalHome = process.env.HOME;
  process.env.HOME = testDir;
});

afterEach(() => {
  process.env.HOME = originalHome;
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

describe('cli-settings', () => {
  describe('readCliSettings', () => {
    it('returns empty object when cli.json does not exist', () => {
      expect(readCliSettings()).toEqual({});
    });

    it('parses existing cli.json', () => {
      writeCliJson({
        'chat.disableWrap': true,
        'chat.defaultModel': 'sonnet',
      });
      expect(readCliSettings()).toEqual({
        'chat.disableWrap': true,
        'chat.defaultModel': 'sonnet',
      });
    });

    it('returns empty object when cli.json is malformed', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'not valid json', 'utf-8');
      expect(readCliSettings()).toEqual({});
    });

    it('returns empty object when cli.json is an array (not an object)', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), '[1, 2, 3]', 'utf-8');
      expect(readCliSettings()).toEqual({});
    });

    it('returns empty object when cli.json is null', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'null', 'utf-8');
      expect(readCliSettings()).toEqual({});
    });
  });

  describe('readBoolSetting', () => {
    it('returns fallback when key missing', () => {
      expect(readBoolSetting('chat.disableWrap', false)).toBe(false);
      expect(readBoolSetting('chat.disableWrap', true)).toBe(true);
    });

    it('returns true when value is true', () => {
      writeCliJson({ 'chat.disableWrap': true });
      expect(readBoolSetting('chat.disableWrap', false)).toBe(true);
    });

    it('returns false when value is false', () => {
      writeCliJson({ 'chat.disableWrap': false });
      expect(readBoolSetting('chat.disableWrap', true)).toBe(false);
    });

    it('returns fallback when value is not boolean', () => {
      writeCliJson({ 'chat.disableWrap': 'yes' });
      expect(readBoolSetting('chat.disableWrap', false)).toBe(false);
    });
  });

  describe('writeCliSettings', () => {
    it('creates the settings directory if it does not exist', () => {
      expect(existsSync(cliJsonPath())).toBe(false);
      writeCliSettings({ 'chat.disableWrap': true });
      expect(existsSync(cliJsonPath())).toBe(true);
      expect(JSON.parse(readFileSync(cliJsonPath(), 'utf-8'))).toEqual({
        'chat.disableWrap': true,
      });
    });

    it('overwrites existing cli.json', () => {
      writeCliJson({ 'chat.defaultModel': 'claude' });
      writeCliSettings({ 'chat.disableWrap': true });
      expect(JSON.parse(readFileSync(cliJsonPath(), 'utf-8'))).toEqual({
        'chat.disableWrap': true,
      });
    });

    it('round-trips with readCliSettings', () => {
      writeCliSettings({ a: 1, b: 'two', c: true });
      expect(readCliSettings()).toEqual({ a: 1, b: 'two', c: true });
    });
  });

  describe('updateCliSetting', () => {
    it('creates file and sets key when cli.json does not exist', async () => {
      await updateCliSetting('chat.defaultModel', 'opus');
      expect(readCliSettings()).toEqual({ 'chat.defaultModel': 'opus' });
    });

    it('merges key into existing settings', async () => {
      writeCliJson({ 'chat.theme': 'dark', 'chat.compact': true });
      await updateCliSetting('chat.defaultModel', 'sonnet');
      expect(readCliSettings()).toEqual({
        'chat.theme': 'dark',
        'chat.compact': true,
        'chat.defaultModel': 'sonnet',
      });
    });

    it('overwrites existing key', async () => {
      writeCliJson({ 'chat.defaultModel': 'old' });
      await updateCliSetting('chat.defaultModel', 'new');
      expect(readCliSettings()).toEqual({ 'chat.defaultModel': 'new' });
    });

    it('serializes concurrent calls (no lost updates)', async () => {
      writeCliJson({});
      await Promise.all([
        updateCliSetting('a', 1),
        updateCliSetting('b', 2),
        updateCliSetting('c', 3),
      ]);
      const result = readCliSettings();
      expect(result.a).toBe(1);
      expect(result.b).toBe(2);
      expect(result.c).toBe(3);
    });

    it('rejects on corrupt file without wiping', async () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'corrupt{{{', 'utf-8');
      await expect(updateCliSetting('key', 'val')).rejects.toThrow();
      expect(readFileSync(cliJsonPath(), 'utf-8')).toBe('corrupt{{{');
    });

    it('recovers after a failed write (queue not poisoned)', async () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'corrupt', 'utf-8');
      await updateCliSetting('a', 1).catch(() => {});
      writeCliJson({ existing: true });
      await updateCliSetting('b', 2);
      expect(readCliSettings()).toEqual({ existing: true, b: 2 });
    });
  });

  describe('toggleBoolSetting', () => {
    it('uses the fallback when the key is missing or malformed', async () => {
      writeCliJson({});
      await expect(toggleBoolSetting('chat.flag', false)).resolves.toBe(true);

      writeCliJson({ 'chat.flag': 'not-a-bool' });
      await expect(toggleBoolSetting('chat.flag', false)).resolves.toBe(true);
    });

    it('returns the committed value, not the value it read', async () => {
      writeCliJson({ 'chat.flag': true });
      await expect(toggleBoolSetting('chat.flag')).resolves.toBe(false);
      expect(readCliSettings()['chat.flag']).toBe(false);
    });

    // Two rapid Space presses. Computing the inverse outside the queue makes
    // both observe `false` and write `true`, so one press is silently lost.
    it('two concurrent toggles restore the original value', async () => {
      writeCliJson({ 'chat.flag': false });
      const [first, second] = await Promise.all([
        toggleBoolSetting('chat.flag', false),
        toggleBoolSetting('chat.flag', false),
      ]);
      expect([first, second]).toEqual([true, false]);
      expect(readCliSettings()['chat.flag']).toBe(false);
    });
  });

  describe('readOptionalStringSetting', () => {
    it('returns undefined when key is missing', () => {
      writeCliJson({});
      expect(readOptionalStringSetting('nope')).toBeUndefined();
    });

    it('returns the string when non-empty', () => {
      writeCliJson({ name: 'hello' });
      expect(readOptionalStringSetting('name')).toBe('hello');
    });

    it('returns undefined for empty string', () => {
      writeCliJson({ name: '' });
      expect(readOptionalStringSetting('name')).toBeUndefined();
    });

    it('returns undefined for non-string value', () => {
      writeCliJson({ name: 42 });
      expect(readOptionalStringSetting('name')).toBeUndefined();
    });
  });
});
