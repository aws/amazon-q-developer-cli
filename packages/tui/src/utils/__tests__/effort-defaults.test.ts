import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  readSavedEffortDefault,
  persistEffortDefault,
  MODEL_DEFAULTS_SETTING,
} from '../effort-defaults.js';

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
    `effort-defaults-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
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

describe('effort-defaults', () => {
  describe('readSavedEffortDefault (tolerant of both schema paths)', () => {
    it('reads the Claude-family output_config.effort path', () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'claude-opus-4.7': { output_config: { effort: 'low' } },
        },
      });
      expect(readSavedEffortDefault('claude-opus-4.7')).toBe('low');
    });

    it('reads the GPT-family reasoning.effort path', () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'gpt-5.1': { reasoning: { effort: 'medium' } },
        },
      });
      expect(readSavedEffortDefault('gpt-5.1')).toBe('medium');
    });

    it('returns undefined when the model has no saved default', () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'claude-opus-4.7': { output_config: { effort: 'low' } },
        },
      });
      expect(readSavedEffortDefault('gpt-5.1')).toBeUndefined();
    });

    it('returns undefined when the setting is absent or malformed', () => {
      expect(readSavedEffortDefault('claude-opus-4.7')).toBeUndefined();
      writeCliJson({ [MODEL_DEFAULTS_SETTING]: 'not-an-object' });
      expect(readSavedEffortDefault('claude-opus-4.7')).toBeUndefined();
    });
  });

  describe('persistEffortDefault (v2-compatible nested shape)', () => {
    it('writes the Claude-family nested output_config.effort shape', async () => {
      await persistEffortDefault(
        'claude-opus-4.7',
        'low',
        'output_config.effort'
      );
      const saved = readCliJson();
      expect(saved[MODEL_DEFAULTS_SETTING]).toEqual({
        'claude-opus-4.7': { output_config: { effort: 'low' } },
      });
    });

    it('writes the GPT-family nested reasoning.effort shape', async () => {
      await persistEffortDefault('gpt-5.1', 'high', 'reasoning.effort');
      const saved = readCliJson();
      expect(saved[MODEL_DEFAULTS_SETTING]).toEqual({
        'gpt-5.1': { reasoning: { effort: 'high' } },
      });
    });

    it('preserves other models when persisting a new one', async () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'claude-opus-4.7': { output_config: { effort: 'xhigh' } },
        },
      });
      await persistEffortDefault(
        'claude-opus-4.6',
        'high',
        'output_config.effort'
      );
      const defaults = readCliJson()[MODEL_DEFAULTS_SETTING] as Record<
        string,
        unknown
      >;
      expect(defaults['claude-opus-4.7']).toEqual({
        output_config: { effort: 'xhigh' },
      });
      expect(defaults['claude-opus-4.6']).toEqual({
        output_config: { effort: 'high' },
      });
    });

    it('round-trips through readSavedEffortDefault', async () => {
      await persistEffortDefault(
        'claude-opus-4.7',
        'medium',
        'output_config.effort'
      );
      expect(readSavedEffortDefault('claude-opus-4.7')).toBe('medium');
    });
  });

  describe('persistEffortDefault honors the resolved path over model naming', () => {
    it('writes reasoning.effort for a claude-named model when resolved so', async () => {
      await persistEffortDefault('claude-opus-4.7', 'high', 'reasoning.effort');
      expect(readCliJson()[MODEL_DEFAULTS_SETTING]).toEqual({
        'claude-opus-4.7': { reasoning: { effort: 'high' } },
      });
    });

    it('writes output_config.effort for a GPT-named model when resolved so', async () => {
      await persistEffortDefault('gpt-5.1', 'low', 'output_config.effort');
      expect(readCliJson()[MODEL_DEFAULTS_SETTING]).toEqual({
        'gpt-5.1': { output_config: { effort: 'low' } },
      });
    });

    it('prunes the stale other-path leaf when the resolved path differs', async () => {
      // Pre-seed output_config for a claude-named model, then persist via the
      // reasoning path: the old leaf must be pruned so readSavedEffortDefault
      // never returns the stale value.
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'claude-opus-4.7': { output_config: { effort: 'low' } },
        },
      });
      await persistEffortDefault(
        'claude-opus-4.7',
        'medium',
        'reasoning.effort'
      );
      const node = (
        readCliJson()[MODEL_DEFAULTS_SETTING] as Record<string, unknown>
      )['claude-opus-4.7'];
      expect(node).toEqual({ reasoning: { effort: 'medium' } });
      expect(readSavedEffortDefault('claude-opus-4.7')).toBe('medium');
    });
  });

  describe('single effort leaf per model (no dual-path)', () => {
    it('prunes a stale output_config.effort when writing the GPT reasoning path', async () => {
      // Pre-seed a node that (wrongly) holds BOTH paths.
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'gpt-5.1': {
            reasoning: { effort: 'low' },
            output_config: { effort: 'high' },
          },
        },
      });
      await persistEffortDefault('gpt-5.1', 'medium', 'reasoning.effort');
      const node = (
        readCliJson()[MODEL_DEFAULTS_SETTING] as Record<string, unknown>
      )['gpt-5.1'];
      // Only the resolved (reasoning) leaf remains; the empty output_config
      // parent is pruned entirely.
      expect(node).toEqual({ reasoning: { effort: 'medium' } });
    });

    it('prunes a stale reasoning.effort when writing the Claude output_config path', async () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'claude-opus-4.7': {
            output_config: { effort: 'low' },
            reasoning: { effort: 'high' },
          },
        },
      });
      await persistEffortDefault(
        'claude-opus-4.7',
        'xhigh',
        'output_config.effort'
      );
      const node = (
        readCliJson()[MODEL_DEFAULTS_SETTING] as Record<string, unknown>
      )['claude-opus-4.7'];
      expect(node).toEqual({ output_config: { effort: 'xhigh' } });
    });

    it('readSavedEffortDefault returns the freshly-written value, never the stale other path', async () => {
      // readSavedEffortDefault checks output_config.effort BEFORE
      // reasoning.effort, so without dedup a stale output_config leaf would
      // shadow a GPT model's real reasoning value. Dedup prevents that.
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'gpt-5.1': {
            reasoning: { effort: 'low' },
            output_config: { effort: 'xhigh' },
          },
        },
      });
      await persistEffortDefault('gpt-5.1', 'high', 'reasoning.effort');
      expect(readSavedEffortDefault('gpt-5.1')).toBe('high');
    });

    it('preserves unrelated sibling fields on the same model node', async () => {
      writeCliJson({
        [MODEL_DEFAULTS_SETTING]: {
          'gpt-5.1': {
            reasoning: { effort: 'low', verbosity: 'high' },
            somethingElse: 1,
          },
        },
      });
      await persistEffortDefault('gpt-5.1', 'medium', 'reasoning.effort');
      const node = (
        readCliJson()[MODEL_DEFAULTS_SETTING] as Record<string, unknown>
      )['gpt-5.1'];
      expect(node).toEqual({
        reasoning: { effort: 'medium', verbosity: 'high' },
        somethingElse: 1,
      });
    });
  });
});
