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
      // First bundled row carries a plain id/label pair (the rest follow suit).
      expect(options[1].value).toBe(`bundled:${bundledThemes[0]!.id}`);
      expect(options[1].label).toBe(bundledThemes[0]!.label);
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
    // The no-seed row pins the inverse: with no prefs, default carries [active].
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
      {
        route: 'prompt',
        seed: undefined,
        activeValue: 'prompt:default',
        defaultValue: undefined,
      },
    ])(
      'marks the active $route preset (seed $seed)',
      async ({ route, seed, activeValue, defaultValue }) => {
        if (seed) saveUserThemePrefs(seed);
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, route, ctx);

        const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
        const options = call[0].options;
        const activeOpt = options.find((o: any) => o.value === activeValue);
        expect(activeOpt.description).toContain('[active]');
        if (defaultValue) {
          const defaultOpt = options.find((o: any) => o.value === defaultValue);
          expect(defaultOpt.description).not.toContain('[active]');
        }
      }
    );
  });

  describe('custom flow — applying presets', () => {
    // setUserColors is called with (prompt, response, diff) slots; applying one
    // category fills ONLY its slot and leaves the others undefined. Each row
    // pins the affected slot's color, an unchanged-slot guard, the alert label,
    // and the persisted pref. Covers prompt/response/diff in one table. Rows
    // with `seedOther` also pin independent persistence: a pre-saved pref in a
    // DIFFERENT category must survive the write (toMatchObject(pref) includes
    // the seed), so applying one slot never clobbers another's persisted value.
    it.each([
      {
        route: 'prompt:purple',
        slot: 0,
        check: (c: any) => {
          expect(c.text.truecolor).toBe('#ffffff');
          expect(c.bg.truecolor).toBe('#552B99');
        },
        untouched: [1],
        labelMatch: 'Purple',
        seedOther: { responsePreset: 'dark' },
        pref: { promptPreset: 'purple', responsePreset: 'dark' },
        // Applying a preset re-opens the custom (prompt/response/diff) menu.
        reopensCustomMenu: true,
      },
      {
        route: 'response:light',
        slot: 1,
        check: (c: any) => expect(c.truecolor).toBe('#FFFFFF'),
        untouched: [0],
        labelMatch: 'Light',
        seedOther: { promptPreset: 'forest' },
        pref: { responsePreset: 'light', promptPreset: 'forest' },
      },
      {
        route: 'response:dark',
        slot: 1,
        check: (c: any) => expect(c.truecolor).toBe('#626262'),
        untouched: [0],
        pref: { responsePreset: 'dark' },
      },
      {
        route: 'diff:colorblind-dark',
        slot: 2,
        check: (c: any) => expect(c.id).toBe('colorblind-dark'),
        untouched: [0, 1],
        labelMatch: 'Accessible',
        seedOther: { promptPreset: 'ocean', responsePreset: 'dark' },
        pref: {
          diffPreset: 'colorblind-dark',
          promptPreset: 'ocean',
          responsePreset: 'dark',
        },
      },
      // The default preset clears its persisted slot rather than persisting one.
      {
        route: 'prompt:default',
        seedOther: { promptPreset: 'purple' },
        clears: ['promptPreset'] as const,
      },
    ])(
      'applies $route into its own slot and persists (independent)',
      async ({
        route,
        slot,
        check,
        untouched,
        labelMatch,
        seedOther,
        pref,
        clears,
        reopensCustomMenu,
      }) => {
        if (seedOther) saveUserThemePrefs(seedOther);
        const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
        await dispatch(themeCmd, route, ctx);

        expect(ctx._spies.setUserColors!).toHaveBeenCalled();
        const colorCall = ctx._spies.setUserColors!.mock.calls[0]!;
        if (slot !== undefined) {
          expect(colorCall[slot]).toBeDefined();
          check!(colorCall[slot]);
        }
        for (const u of untouched ?? []) expect(colorCall[u]).toBeUndefined();
        if (labelMatch) {
          expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toContain(
            labelMatch
          );
          expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');
        }
        if (pref) expect(loadUserThemePrefs()).toMatchObject(pref);
        for (const k of clears ?? [])
          expect((loadUserThemePrefs() as any)[k]).toBeUndefined();
        if (reopensCustomMenu) {
          const options =
            ctx._spies.setActiveCommand!.mock.calls.at(-1)![0].options;
          expect(options.map((o: any) => o.value)).toEqual([
            'prompt',
            'response',
            'diff',
          ]);
        }
      }
    );

    // Unknown preset id surfaces an error alert (one category — the handler
    // routes all three through the same lookup).
    it('shows error for an unknown preset', async () => {
      const ctx = createLiteMockCtx({ slashCommands: [themeCmd] });
      await dispatch(themeCmd, 'prompt:nonexistent', ctx);
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });
  });
});
