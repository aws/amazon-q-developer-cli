import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  promptPresets,
  responsePresets,
  diffPresets,
  bundledThemes,
  buildPromptPreview,
  buildResponsePreview,
  buildDiffPreview,
  buildBundledPreview,
  buildCurrentPreview,
  buildFallbackDiff,
  loadUserThemePrefs,
  saveUserThemePrefs,
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
  getBundledTheme,
  PROMPT_PREVIEW,
  RESPONSE_PREVIEW,
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
      } catch {
        /* cleanup */
      }
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

  describe('diffPresets', () => {
    it('all have id, label, added, and removed', () => {
      for (const p of diffPresets) {
        expect(p.id).toBeTruthy();
        expect(p.label).toBeTruthy();
        expect(p.added).toBeDefined();
        expect(p.removed).toBeDefined();
      }
    });

    it('ids are unique', () => {
      const ids = diffPresets.map((p) => p.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('includes default, dark, light, colorblind-dark, colorblind-light', () => {
      const ids = diffPresets.map((p) => p.id);
      expect(ids).toContain('default');
      expect(ids).toContain('dark');
      expect(ids).toContain('light');
      expect(ids).toContain('colorblind-dark');
      expect(ids).toContain('colorblind-light');
    });
  });

  describe('bundledThemes', () => {
    it('has dark and light entries', () => {
      const ids = bundledThemes.map((t) => t.id);
      expect(ids).toContain('dark');
      expect(ids).toContain('light');
    });

    it('each has prompt, response, and diff with valid structure', () => {
      for (const t of bundledThemes) {
        expect(t.prompt).toBeDefined();
        expect(t.prompt.id).toBeTruthy();
        expect(t.prompt.textColor).toBeDefined();
        expect(t.prompt.bgColor).toBeDefined();
        expect(t.response).toBeDefined();
        expect(t.response.id).toBeTruthy();
        expect(t.response.textColor).toBeDefined();
        expect(t.diff).toBeDefined();
        expect(t.diff.added).toBeDefined();
        expect(t.diff.removed).toBeDefined();
      }
    });
  });

  describe('getDiffPreset', () => {
    it('returns preset by id', () => {
      const preset = getDiffPreset('dark');
      expect(preset).toBeDefined();
      expect(preset!.id).toBe('dark');
    });

    it('returns undefined for unknown id', () => {
      expect(getDiffPreset('nonexistent')).toBeUndefined();
    });

    it('returns undefined for undefined input', () => {
      expect(getDiffPreset(undefined)).toBeUndefined();
    });
  });

  describe('buildDiffPreview', () => {
    it('returns non-empty string for each diffPreset', () => {
      for (const p of diffPresets) {
        const preview = buildDiffPreview(p);
        expect(preview.length).toBeGreaterThan(0);
      }
    });

    it('includes checkmark for current id', () => {
      const dark = diffPresets.find((p) => p.id === 'dark')!;
      const preview = buildDiffPreview(dark, 'dark');
      expect(preview).toContain('\u2713');
    });

    it('works with fallbackDiff parameter', () => {
      const defaultPreset = diffPresets.find((p) => p.id === 'default')!;
      const darkPreset = diffPresets.find((p) => p.id === 'dark')!;
      const preview = buildDiffPreview(defaultPreset, undefined, darkPreset);
      expect(preview.length).toBeGreaterThan(0);
    });
  });

  describe('buildBundledPreview', () => {
    // Brand colour required by the new signature; using kiroDark's brand
    // hex keeps the test independent of theme registration.
    const TEST_BRAND = { truecolor: '#C19AFF', color256: 141 } as const;

    it('returns non-empty string for each bundledTheme', () => {
      for (const t of bundledThemes) {
        const preview = buildBundledPreview(t, undefined, TEST_BRAND);
        expect(preview.length).toBeGreaterThan(0);
      }
    });

    it('contains prompt and response text', () => {
      for (const t of bundledThemes) {
        const preview = buildBundledPreview(t, undefined, TEST_BRAND);
        expect(preview).toContain(PROMPT_PREVIEW);
        expect(preview).toContain(RESPONSE_PREVIEW);
      }
    });

    it('emits prompt and response on separate lines, diff block underneath', () => {
      const dark = bundledThemes.find((t) => t.id === 'dark')!;
      const preview = buildBundledPreview(dark, undefined, TEST_BRAND);
      // Strip ANSI escape sequences to compare layout structure rather
      // than colour codes — those are environment dependent and tested
      // implicitly via `toContain` checks.
      // eslint-disable-next-line no-control-regex -- intentional ANSI strip
      const plain = preview.replace(/\u001b\[[0-9;]*m/g, '');
      const lines = plain.split('\n');
      // Row 0: "▌ <PROMPT_PREVIEW>" — first non-bar character is the
      // bar's trailing space, then the prompt text.
      expect(lines[0]).toContain(PROMPT_PREVIEW);
      // Row 1: "▌ <RESPONSE_PREVIEW>"
      expect(lines[1]).toContain(RESPONSE_PREVIEW);
      // Diff block has its own header and +/- lines further down.
      expect(plain).toContain('Code diff:');
      expect(plain).toContain('+');
      expect(plain).toContain('-');
    });

    it('uses brand colour for the ▌ bar (not prompt/response colors)', () => {
      // The brand colour appears as a truecolor SGR (e.g. \x1b[48;2;193;154;255m)
      // — match it specifically. Prompt/response colors of the dark
      // bundle are different so we can prove the bar is brand-coloured.
      const dark = bundledThemes.find((t) => t.id === 'dark')!;
      const preview = buildBundledPreview(dark, undefined, TEST_BRAND);
      // C19AFF -> 193;154;255 in decimal RGB
      expect(preview).toContain('48;2;193;154;255');
    });
  });

  describe('getBundledTheme', () => {
    it('returns theme by id', () => {
      const theme = getBundledTheme('dark');
      expect(theme).toBeDefined();
      expect(theme!.id).toBe('dark');
    });

    it('returns undefined for unknown id', () => {
      expect(getBundledTheme('nonexistent')).toBeUndefined();
    });

    it('returns undefined for undefined input', () => {
      expect(getBundledTheme(undefined)).toBeUndefined();
    });
  });

  describe('buildCurrentPreview', () => {
    const TEST_BRAND = { truecolor: '#C19AFF', color256: 141 } as const;

    it('returns non-empty string with default prefs {}', () => {
      const preview = buildCurrentPreview({}, undefined, TEST_BRAND);
      expect(preview.length).toBeGreaterThan(0);
    });

    it('returns non-empty string with specific prefs', () => {
      const preview = buildCurrentPreview(
        {
          promptPreset: 'purple',
          responsePreset: 'light',
          diffPreset: 'dark',
        },
        undefined,
        TEST_BRAND
      );
      expect(preview.length).toBeGreaterThan(0);
    });
  });

  describe('buildFallbackDiff', () => {
    it('returns valid DiffPreset from TerminalColor inputs', () => {
      const result = buildFallbackDiff({
        added: {
          background: { truecolor: '#112233', color256: 22 },
          bar: { truecolor: '#aabbcc', color256: 146 },
          highlight: { truecolor: '#223344' },
        },
        removed: {
          background: { truecolor: '#443322', color256: 52 },
          bar: { truecolor: '#ff0000', color256: 196 },
          highlight: { truecolor: '#332211' },
        },
      });
      expect(result.id).toBe('_fallback');
      expect(result.label).toBeTruthy();
      expect(result.added.bar.truecolor).toBe('#aabbcc');
      expect(result.added.bar.color256).toBe(146);
      expect(result.removed.bar.truecolor).toBe('#ff0000');
    });

    it('has id "_fallback"', () => {
      const result = buildFallbackDiff({
        added: {
          background: { named: 'default' },
          bar: { named: 'green' },
          highlight: { named: 'default' },
        },
        removed: {
          background: { named: 'default' },
          bar: { named: 'red' },
          highlight: { named: 'default' },
        },
      });
      expect(result.id).toBe('_fallback');
    });
  });
});
