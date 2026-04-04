import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  promptPresets,
  responsePresets,
  buildPromptPreview,
  buildResponsePreview,
  loadUserThemePrefs,
  saveUserThemePrefs,
  getPromptPreset,
  getResponsePreset,
  type UserThemePrefs,
} from '../user-theme';

describe('user-theme', () => {
  describe('presets', () => {
    it('prompt presets all have id, label, textColor, and bgColor', () => {
      for (const p of promptPresets) {
        expect(p.id).toBeTruthy();
        expect(p.label).toBeTruthy();
        expect(p.textColor).toBeDefined();
        expect(p.bgColor).toBeDefined();
      }
    });

    it('response presets all have id, label, and textColor', () => {
      for (const p of responsePresets) {
        expect(p.id).toBeTruthy();
        expect(p.label).toBeTruthy();
        expect(p.textColor).toBeDefined();
      }
    });

    it('prompt presets include a default entry', () => {
      expect(promptPresets.find((p) => p.id === 'default')).toBeDefined();
    });

    it('response presets include default, light, and dark', () => {
      expect(responsePresets.find((p) => p.id === 'default')).toBeDefined();
      expect(responsePresets.find((p) => p.id === 'light')).toBeDefined();
      expect(responsePresets.find((p) => p.id === 'dark')).toBeDefined();
    });

    it('prompt preset ids are unique', () => {
      const ids = promptPresets.map((p) => p.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('response preset ids are unique', () => {
      const ids = responsePresets.map((p) => p.id);
      expect(new Set(ids).size).toBe(ids.length);
    });
  });

  describe('getPromptPreset', () => {
    it('returns preset by id', () => {
      const preset = getPromptPreset('purple');
      expect(preset).toBeDefined();
      expect(preset!.label).toBe('Purple');
    });

    it('returns undefined for unknown id', () => {
      expect(getPromptPreset('nonexistent')).toBeUndefined();
    });

    it('returns undefined for undefined input', () => {
      expect(getPromptPreset(undefined)).toBeUndefined();
    });
  });

  describe('getResponsePreset', () => {
    it('returns preset by id', () => {
      const preset = getResponsePreset('light');
      expect(preset).toBeDefined();
      expect(preset!.label).toBe('Light');
    });

    it('returns undefined for unknown id', () => {
      expect(getResponsePreset('nonexistent')).toBeUndefined();
    });
  });

  describe('buildPromptPreview', () => {
    it('returns a non-empty string for each preset', () => {
      for (const p of promptPresets) {
        const preview = buildPromptPreview(p);
        expect(preview.length).toBeGreaterThan(0);
      }
    });

    it('includes checkmark for current preset', () => {
      const preview = buildPromptPreview(promptPresets[0]!, 'default');
      expect(preview).toContain('✓');
    });

    it('does not include checkmark for non-current preset', () => {
      const purple = promptPresets.find((p) => p.id === 'purple')!;
      const preview = buildPromptPreview(purple, 'default');
      expect(preview).not.toContain('✓');
    });

    it('uses themeSurfaceHex for default preset when provided', () => {
      const defaultPreset = promptPresets.find((p) => p.id === 'default')!;
      const preview = buildPromptPreview(defaultPreset, undefined, '#EEEEEE');
      expect(preview.length).toBeGreaterThan(0);
    });
  });

  describe('buildResponsePreview', () => {
    it('returns a non-empty string for each preset', () => {
      for (const p of responsePresets) {
        const preview = buildResponsePreview(p);
        expect(preview.length).toBeGreaterThan(0);
      }
    });

    it('includes checkmark for current preset', () => {
      const light = responsePresets.find((p) => p.id === 'light')!;
      const preview = buildResponsePreview(light, 'light');
      expect(preview).toContain('✓');
    });
  });

  describe('persistence', () => {
    let testDir: string;
    let originalHome: string | undefined;

    beforeEach(() => {
      originalHome = process.env.HOME;
      testDir = join(
        tmpdir(),
        `kiro-theme-test-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
      );
      mkdirSync(join(testDir, '.kiro', 'settings'), { recursive: true });
      process.env.HOME = testDir;
    });

    afterEach(() => {
      process.env.HOME = originalHome;
      try {
        rmSync(testDir, { recursive: true, force: true });
      } catch {}
    });

    it('loadUserThemePrefs returns empty object when no file exists', () => {
      const prefs = loadUserThemePrefs();
      expect(prefs).toEqual({});
    });

    it('saveUserThemePrefs writes and loadUserThemePrefs reads back', () => {
      const prefs: UserThemePrefs = {
        promptPreset: 'purple',
        responsePreset: 'light',
      };
      saveUserThemePrefs(prefs);
      const loaded = loadUserThemePrefs();
      expect(loaded.promptPreset).toBe('purple');
      expect(loaded.responsePreset).toBe('light');
    });

    it('saveUserThemePrefs creates directories if missing', () => {
      rmSync(join(testDir, '.kiro'), { recursive: true, force: true });
      saveUserThemePrefs({ promptPreset: 'ocean' });
      const loaded = loadUserThemePrefs();
      expect(loaded.promptPreset).toBe('ocean');
    });

    it('loadUserThemePrefs handles malformed JSON gracefully', () => {
      writeFileSync(
        join(testDir, '.kiro', 'settings', 'kiro_cli_theme.json'),
        'not json'
      );
      const prefs = loadUserThemePrefs();
      expect(prefs).toEqual({});
    });

    it('saveUserThemePrefs with undefined values omits them', () => {
      saveUserThemePrefs({ promptPreset: undefined, responsePreset: 'dark' });
      const raw = readFileSync(
        join(testDir, '.kiro', 'settings', 'kiro_cli_theme.json'),
        'utf-8'
      );
      const parsed = JSON.parse(raw);
      expect(parsed.responsePreset).toBe('dark');
      expect('promptPreset' in parsed).toBe(false);
    });
  });
});
