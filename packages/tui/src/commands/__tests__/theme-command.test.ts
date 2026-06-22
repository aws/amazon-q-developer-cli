import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
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

/** Create a mock context with getUiMode set to 'lite' (theme command-menu only fires in lite mode). */
function createLiteMockCtx(
  ...args: Parameters<typeof createMockCommandContext>
) {
  const ctx = createMockCommandContext(...args);
  (ctx as any).getUiMode = mock(() => 'lite');
  return ctx;
}

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
    it('shows top-level options: Auto + bundled themes (plain labels) + Custom', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      expect(options).toHaveLength(bundledThemes.length + 2); // Auto + bundled + Custom
      expect(options[0].value).toBe('bundled:default');
      expect(options[0].label).toBe('Auto');
      // Bundled rows in between carry plain id/label pairs.
      for (let i = 0; i < bundledThemes.length; i++) {
        expect(options[i + 1].value).toBe(`bundled:${bundledThemes[i]!.id}`);
        expect(options[i + 1].label).toBe(bundledThemes[i]!.label);
      }
      expect(options[options.length - 1].value).toBe('custom');
      expect(options[options.length - 1].label).toBe('Custom');
    });
  });

  describe('bundled themes (Dark/Light)', () => {
    // Applying a bundled theme sets prompt + response colors, switches the base
    // theme to its surface truecolor, alerts success, and persists every preset
    // slot (prompt/response/diff/baseTheme). Parameterized over the two bundled
    // themes — same flow, different fixed color/pref values.
    it.each([
      {
        id: 'dark',
        surface: '#262626',
        labelMatch: 'Dark',
        prefs: {
          responsePreset: 'light',
          diffPreset: 'dark',
          baseTheme: 'dark',
        },
      },
      {
        id: 'light',
        surface: '#EEEEEE',
        labelMatch: 'Light',
        prefs: {
          promptPreset: 'paper',
          responsePreset: 'dark',
          diffPreset: 'light',
          baseTheme: 'light',
        },
      },
    ])(
      'applies bundled $id theme: sets colors, base theme, and persists presets',
      async ({ id, surface, labelMatch, prefs: expectedPrefs }) => {
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, `bundled:${id}`, ctx);

        // Sets both prompt { text, bg } and response color.
        expect(ctx._spies.setUserColors!).toHaveBeenCalled();
        const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
        expect(colorCall[0]).toBeDefined();
        expect(colorCall[1]).toBeDefined();

        // Switches the base theme to the matching surface.
        expect(ctx._spies.setBaseTheme!).toHaveBeenCalled();
        const baseThemeCall = ctx._spies.setBaseTheme!.mock.calls[0]!;
        expect(baseThemeCall[0]).toBeDefined();
        expect(baseThemeCall[0].colors.surface.truecolor).toBe(surface);

        expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain(labelMatch);
        expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

        expect(loadUserThemePrefs()).toMatchObject(expectedPrefs);
      }
    );

    it('shows error for unknown bundled theme', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
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
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
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
    it('shows prompt/response/diff category selection and sets a preview', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'custom', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      expect(options).toHaveLength(3);
      expect(options[0].value).toBe('prompt');
      expect(options[1].value).toBe('response');
      expect(options[2].value).toBe('diff');
      // Entering custom seeds the live preview.
      const previewCall = ctx._spies.setThemePreview!.mock.calls[0]!;
      expect(typeof previewCall[0]).toBe('string');
      expect(previewCall[0].length).toBeGreaterThan(0);
    });
  });

  describe('custom flow — [active] markers', () => {
    // The seeded preset's row carries [active]; the default row does not.
    it.each([
      {
        route: 'prompt',
        seed: { promptPreset: 'ocean' },
        activeValue: 'prompt:ocean',
        defaultValue: 'prompt:default',
      },
      {
        route: 'diff',
        seed: { diffPreset: 'colorblind-dark' },
        activeValue: 'diff:colorblind-dark',
        defaultValue: 'diff:default',
      },
    ])(
      'shows [active] on the current $route preset',
      async ({ route, seed, activeValue, defaultValue }) => {
        saveUserThemePrefs(seed);
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, route, ctx);

        const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
        const options = call[0].options;
        const activeOpt = options.find((o: any) => o.value === activeValue);
        const defaultOpt = options.find((o: any) => o.value === defaultValue);
        expect(activeOpt.description).toContain('[active]');
        expect(defaultOpt.description).not.toContain('[active]');
      }
    );

    it('shows [active] on default when no prefs set', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const options = call[0].options;
      const defaultOpt = options.find((o: any) => o.value === 'prompt:default');
      expect(defaultOpt.description).toContain('[active]');
    });
  });

  describe('custom flow — applying presets', () => {
    it('applies purple prompt preset and persists', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
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
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:default', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const prefs = loadUserThemePrefs();
      expect(prefs.promptPreset).toBeUndefined();
    });

    // Applying a response preset writes only the response color slot (arg index
    // 1; prompt/diff untouched), alerts the label, and persists responsePreset.
    it.each([
      { id: 'light', truecolor: '#FFFFFF', labelMatch: 'Light' },
      { id: 'dark', truecolor: '#626262', labelMatch: undefined },
    ])(
      'applies response:$id preset, sets only the response color, and persists',
      async ({ id, truecolor, labelMatch }) => {
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, `response:${id}`, ctx);

        expect(ctx._spies.setUserColors!).toHaveBeenCalled();
        const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
        expect(colorCall[0]).toBeUndefined(); // prompt unchanged
        expect(colorCall[1]).toBeDefined();
        expect(colorCall[1].truecolor).toBe(truecolor);
        if (labelMatch) {
          expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain(
            labelMatch
          );
        }
        expect(loadUserThemePrefs().responsePreset).toBe(id);
      }
    );

    // Unknown preset id surfaces an error alert for every category.
    it.each(['prompt', 'response', 'diff'])(
      'shows error for unknown %s preset',
      async (category) => {
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, `${category}:nonexistent`, ctx);

        expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
      }
    );
  });

  describe('independent persistence', () => {
    // Applying one category preset persists it WITHOUT clobbering the others'
    // pre-saved prefs. `seed` is the unrelated pref(s) that must survive.
    it.each([
      {
        route: 'prompt:ocean',
        seed: { responsePreset: 'dark' },
        expected: { promptPreset: 'ocean', responsePreset: 'dark' },
      },
      {
        route: 'response:light',
        seed: { promptPreset: 'forest' },
        expected: { promptPreset: 'forest', responsePreset: 'light' },
      },
      {
        route: 'diff:colorblind-dark',
        seed: { promptPreset: 'ocean', responsePreset: 'dark' },
        expected: {
          promptPreset: 'ocean',
          responsePreset: 'dark',
          diffPreset: 'colorblind-dark',
        },
      },
    ])(
      '$route persists without affecting other categories',
      async ({ route, seed, expected }) => {
        saveUserThemePrefs(seed);
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, route, ctx);

        expect(loadUserThemePrefs()).toMatchObject(expected);
      }
    );
  });

  describe('diff presets', () => {
    // Unique to diff: writes only the diff color slot (arg index 2; prompt +
    // response untouched) and surfaces the preset's own label. The category's
    // option list, [active] marker, default-clear, unknown-preset error, and
    // persistence are all covered by the parameterized it.each blocks above.
    it('applies colorblind-dark diff preset, sets only the diff slot, and persists', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'diff:colorblind-dark', ctx);

      expect(ctx._spies.setUserColors!).toHaveBeenCalled();
      const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
      expect(colorCall[0]).toBeUndefined(); // prompt unchanged
      expect(colorCall[1]).toBeUndefined(); // response unchanged
      expect(colorCall[2]).toBeDefined(); // diff preset
      expect(colorCall[2].id).toBe('colorblind-dark');

      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain('Accessible');
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');

      expect(loadUserThemePrefs().diffPreset).toBe('colorblind-dark');
    });
  });

  describe('ESC navigation flag (themeReturnOnEscape)', () => {
    // Locks in the back-navigation contract for /theme menus. CommandMenu's
    // handleActiveCommandClose reads `themeReturnOnEscape` and re-dispatches
    // `/theme [route]`:
    //   - null: ESC closes the overlay (no parent above).
    //   - '':   re-dispatches `/theme`        — back to top.
    //   - 'custom': re-dispatches `/theme custom` — back to custom menu.
    // Applying a preset (prompt:<id>) re-arms to '' since the menu re-opens at
    // the custom level (else the preview-and-keep-tweaking flow feels one-shot).
    it.each([
      ['', null],
      ['custom', ''],
      ['prompt', 'custom'],
      ['response', 'custom'],
      ['diff', 'custom'],
      ['prompt:purple', ''],
    ] as const)('/theme %s sets the flag to %p', async (route, expected) => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, route, ctx);
      const calls = ctx._spies.setThemeReturnOnEscape!.mock
        .calls as unknown as unknown[][];
      const last = calls[calls.length - 1];
      expect(last?.[0]).toBe(expected);
    });
  });
});
