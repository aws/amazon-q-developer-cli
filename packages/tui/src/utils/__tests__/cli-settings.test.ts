import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  readCliSettings,
  readBoolSetting,
  writeCliSettings,
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
});
