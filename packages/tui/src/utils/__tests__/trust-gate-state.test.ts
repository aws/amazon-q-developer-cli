import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  isTrustGateAccepted,
  saveTrustGateAccepted,
} from '../trust-gate-state.js';

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

function readCliJson(): Record<string, unknown> {
  return JSON.parse(readFileSync(cliJsonPath(), 'utf-8'));
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `trust-gate-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
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

describe('trust-gate-state', () => {
  describe('isTrustGateAccepted', () => {
    it('returns false when cli.json does not exist', () => {
      expect(isTrustGateAccepted()).toBe(false);
    });

    it('returns true when setting is true', () => {
      writeCliJson({ 'chat.disableTrustAllConfirmation': true });
      expect(isTrustGateAccepted()).toBe(true);
    });

    it('returns false when setting is false', () => {
      writeCliJson({ 'chat.disableTrustAllConfirmation': false });
      expect(isTrustGateAccepted()).toBe(false);
    });

    it('returns false when setting is missing', () => {
      writeCliJson({ 'chat.defaultModel': 'claude' });
      expect(isTrustGateAccepted()).toBe(false);
    });

    it('handles corrupt cli.json gracefully', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'not json!!!', 'utf-8');
      expect(isTrustGateAccepted()).toBe(false);
    });

    it('handles null cli.json gracefully', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), 'null', 'utf-8');
      expect(isTrustGateAccepted()).toBe(false);
    });

    it('handles array cli.json gracefully', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(cliJsonPath(), '[true]', 'utf-8');
      expect(isTrustGateAccepted()).toBe(false);
    });
  });

  describe('saveTrustGateAccepted', () => {
    it('creates cli.json and directories on first save', () => {
      expect(existsSync(cliJsonPath())).toBe(false);
      saveTrustGateAccepted();
      expect(existsSync(cliJsonPath())).toBe(true);
      expect(isTrustGateAccepted()).toBe(true);
    });

    it('preserves existing settings when saving', () => {
      writeCliJson({
        'chat.defaultModel': 'claude',
        'telemetry.enabled': false,
      });
      saveTrustGateAccepted();
      const settings = readCliJson();
      expect(settings['chat.disableTrustAllConfirmation']).toBe(true);
      expect(settings['chat.defaultModel']).toBe('claude');
      expect(settings['telemetry.enabled']).toBe(false);
    });

    it('overwrites false value to true', () => {
      writeCliJson({ 'chat.disableTrustAllConfirmation': false });
      expect(isTrustGateAccepted()).toBe(false);
      saveTrustGateAccepted();
      expect(isTrustGateAccepted()).toBe(true);
    });

    it('is idempotent', () => {
      saveTrustGateAccepted();
      saveTrustGateAccepted();
      expect(isTrustGateAccepted()).toBe(true);
    });
  });
});
