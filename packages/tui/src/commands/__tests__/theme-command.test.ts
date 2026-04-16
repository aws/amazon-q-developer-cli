import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { mkdirSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  loadUserThemePrefs,
  saveUserThemePrefs,
  bundledThemes,
} from '../../theme/user-theme';
import { createMockCommandContext } from './test-helpers.js';

const themeCmd: SlashCommand = {
  name: '/theme',
  description: 'Select a theme that looks best for your terminal',
  source: 'local',
  meta: { local: true },
};

describe('/theme command', () => {
  let testDir: string;
  let originalHome: string | undefined;

  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-theme-cmd-test-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    mkdirSync(join(testDir, '.kiro', 'settings'), { recursive: true });
    originalHome = process.env.HOME;
    process.env.HOME = testDir;
  });

  afterEach(() => {
    process.env.HOME = originalHome;
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  describe('bare /theme (no args)', () => {
    it('shows top-level options: Default, bundled themes, and Custom', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      expect(options).toHaveLength(bundledThemes.length + 2); // Default + bundled + Custom
      expect(options[0].value).toBe('bundled:default');
      expect(options[0].label).toBe('Auto');
      expect(options[options.length - 1].value).toBe('custom');
      expect(options[options.length - 1].label).toBe('Custom');
    });

    it('bundled theme options have plain labels', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, '', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      for (let i = 0; i < bundledThemes.length; i++) {
        expect(options[i + 1].value).toBe(`bundled:${bundledThemes[i]!.id}`);
        expect(options[i + 1].label).toBe(bundledThemes[i]!.label);
      }
    });
  });

  describe('bundled themes (Dark/Light)', () => {
    it('applies dark bundled theme and persists both prompt and response', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:dark', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      // Should set both prompt and response
      expect(colorCall[0]).toBeDefined(); // prompt { text, bg }
      expect(colorCall[1]).toBeDefined(); // response color

      // Should switch base theme to kiroDark
      expect(ctx._spies.setBaseTheme!).toHaveBeenCalled();
      const baseThemeCall = ctx._spies.setBaseTheme!.mock.calls[0]!;
      expect(baseThemeCall[0]).toBeDefined();
      expect(baseThemeCall[0].colors.surface.truecolor).toBe('#262626');

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('Dark');
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

      const prefs = loadUserThemePrefs();
      expect(prefs.responsePreset).toBe('light');
      expect(prefs.baseTheme).toBe('dark');
    });

    it('applies light bundled theme', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:light', ctx);

      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeDefined();
      expect(colorCall[1]).toBeDefined();

      // Should switch base theme to kiroLight
      expect(ctx._spies.setBaseTheme!).toHaveBeenCalled();
      const baseThemeCall = ctx._spies.setBaseTheme!.mock.calls[0]!;
      expect(baseThemeCall[0]).toBeDefined();
      expect(baseThemeCall[0].colors.surface.truecolor).toBe('#EEEEEE');

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBe('paper');
      expect(prefs.responsePreset).toBe('dark');
      expect(prefs.baseTheme).toBe('light');
    });

    it('shows error for unknown bundled theme', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:nonexistent', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });

    it('Default resets all overrides and clears persisted prefs', async () => {
      saveUserThemePrefs({
        promptPreset: 'purple',
        responsePreset: 'light',
        diffPreset: 'colorblind-dark',
        baseTheme: 'light',
      });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:default', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeNull();
      expect(colorCall[1]).toBeNull();
      expect(colorCall[2]).toBeNull();

      // Should reset base theme to auto-detect
      expect(ctx._spies.setBaseTheme!).toHaveBeenCalled();
      expect(ctx._spies.setBaseTheme!.mock.calls[0]![0]).toBeNull();

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('reset');
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBeUndefined();
      expect(prefs.responsePreset).toBeUndefined();
      expect(prefs.diffPreset).toBeUndefined();
      expect(prefs.baseTheme).toBeUndefined();
    });
  });

  describe('/theme custom', () => {
    it('shows prompt, response, and diff category selection', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'custom', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      expect(options).toHaveLength(3);
      expect(options[0].value).toBe('prompt');
      expect(options[1].value).toBe('response');
      expect(options[2].value).toBe('diff');
    });

    it('sets theme preview when entering custom flow', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'custom', ctx);

      expect(ctx._spies.setThemePreview!).toHaveBeenCalled();
      const previewCall = ctx._spies.setThemePreview!.mock.calls[0]!;
      expect(typeof previewCall[0]).toBe('string');
      expect(previewCall[0].length).toBeGreaterThan(0);
    });
  });

  describe('custom flow — [active] markers', () => {
    it('shows [active] on the current prompt preset', async () => {
      saveUserThemePrefs({ promptPreset: 'ocean' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      const oceanOpt = options.find((o: any) => o.value === 'prompt:ocean');
      const defaultOpt = options.find((o: any) => o.value === 'prompt:default');
      expect(oceanOpt.description).toContain('[active]');
      expect(defaultOpt.description).not.toContain('[active]');
    });

    it('shows [active] on default when no prefs set', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      const defaultOpt = options.find((o: any) => o.value === 'prompt:default');
      expect(defaultOpt.description).toContain('[active]');
    });
  });

  describe('custom flow — applying presets', () => {
    it('applies purple prompt preset and persists', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:purple', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeDefined();
      expect(colorCall[0].text.truecolor).toBe('#ffffff');
      expect(colorCall[0].bg.truecolor).toBe('#552B99');
      expect(colorCall[1]).toBeUndefined();

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('Purple');
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBe('purple');

      // Should return to custom menu (setActiveCommand called again with prompt/response/diff options)
      const lastCall = ctx._spies.setActiveCommand!.mock.calls.at(-1)!;
      const options = lastCall[0].options;
      expect(options).toHaveLength(3);
      expect(options[0].value).toBe('prompt');
      expect(options[1].value).toBe('response');
      expect(options[2].value).toBe('diff');
    });

    it('applies default preset and clears persisted value', async () => {
      saveUserThemePrefs({ promptPreset: 'purple' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:default', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBeUndefined();
    });

    it('shows error for unknown prompt preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:nonexistent', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });

    it('applies light response preset and persists', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'response:light', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeUndefined();
      expect(colorCall[1]).toBeDefined();
      expect(colorCall[1].truecolor).toBe('#FFFFFF');

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('Light');

      const prefs = loadUserThemePrefs();
      expect(prefs.responsePreset).toBe('light');
    });

    it('applies dark preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'response:dark', ctx);

      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[1].truecolor).toBe('#626262');

      const prefs = loadUserThemePrefs();
      expect(prefs.responsePreset).toBe('dark');
    });

    it('shows error for unknown response preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'response:nonexistent', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });
  });

  describe('independent persistence', () => {
    it('changing prompt does not affect response', async () => {
      saveUserThemePrefs({ responsePreset: 'dark' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:ocean', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBe('ocean');
      expect(prefs.responsePreset).toBe('dark');
    });

    it('changing response does not affect prompt', async () => {
      saveUserThemePrefs({ promptPreset: 'forest' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'response:light', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBe('forest');
      expect(prefs.responsePreset).toBe('light');
    });

    it('changing diff does not affect prompt or response', async () => {
      saveUserThemePrefs({ promptPreset: 'ocean', responsePreset: 'dark' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff:colorblind-dark', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBe('ocean');
      expect(prefs.responsePreset).toBe('dark');
      expect(prefs.diffPreset).toBe('colorblind-dark');
    });
  });

  describe('diff presets', () => {
    it('/theme diff shows diff preset options', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      expect(options.length).toBeGreaterThan(0);
      expect(
        options.find((o: any) => o.value === 'diff:default')
      ).toBeDefined();
      expect(
        options.find((o: any) => o.value === 'diff:colorblind-dark')
      ).toBeDefined();
    });

    it('applies colorblind-dark diff preset and persists', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff:colorblind-dark', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeUndefined(); // prompt unchanged
      expect(colorCall[1]).toBeUndefined(); // response unchanged
      expect(colorCall[2]).toBeDefined(); // diff preset
      expect(colorCall[2].id).toBe('colorblind-dark');

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('Accessible');
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

      const prefs = loadUserThemePrefs();
      expect(prefs.diffPreset).toBe('colorblind-dark');
    });

    it('applies default diff preset and clears persisted value', async () => {
      saveUserThemePrefs({ diffPreset: 'colorblind-dark' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff:default', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.diffPreset).toBeUndefined();
    });

    it('shows error for unknown diff preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff:nonexistent', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });

    it('bundled dark theme persists diff preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:dark', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.diffPreset).toBe('dark');
      expect(prefs.baseTheme).toBe('dark');
    });

    it('bundled light theme persists diff preset', async () => {
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'bundled:light', ctx);

      const prefs = loadUserThemePrefs();
      expect(prefs.diffPreset).toBe('light');
      expect(prefs.baseTheme).toBe('light');
    });

    it('shows [active] on current diff preset', async () => {
      saveUserThemePrefs({ diffPreset: 'colorblind-dark' });
      const ctx = createMockCommandContext({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      const activeOpt = options.find(
        (o: any) => o.value === 'diff:colorblind-dark'
      );
      const defaultOpt = options.find((o: any) => o.value === 'diff:default');
      expect(activeOpt.description).toContain('[active]');
      expect(defaultOpt.description).not.toContain('[active]');
    });
  });
});
