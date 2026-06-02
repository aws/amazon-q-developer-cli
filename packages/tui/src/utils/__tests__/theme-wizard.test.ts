import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  loadUserThemePrefs,
  saveUserThemePrefs,
} from '../../theme/user-theme.js';
import {
  applyAuto,
  applyBundled,
  applyWizardStep,
  buildTopLevelItems,
  getStep,
  nextStep,
  prevStep,
  wizardSteps,
} from '../theme-wizard.js';

/**
 * Pure-function tests for the wizard logic. Visual rendering and user
 * input live in `<ThemePanel>` and are exercised by integration tests.
 */
describe('theme-wizard', () => {
  let testDir: string;
  let originalHome: string | undefined;

  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-theme-wizard-test-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
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

  describe('buildTopLevelItems', () => {
    it('returns Auto / Dark theme / Light theme / Custom in spec order', () => {
      const items = buildTopLevelItems({});
      expect(items.map((i) => i.choice)).toEqual([
        'auto',
        'bundled-dark',
        'bundled-light',
        'custom',
      ]);
      expect(items.map((i) => i.label)).toEqual([
        'Auto',
        'Dark theme',
        'Light theme',
        'Custom',
      ]);
    });

    it('marks Auto as [active] when no presets are saved', () => {
      const items = buildTopLevelItems({});
      expect(items[0]!.description).toBe('[active]');
      expect(items[1]!.description).toBe('');
      expect(items[2]!.description).toBe('');
      expect(items[3]!.description).toContain('Choose');
    });

    it('marks Dark theme as [active] when prefs match the dark bundle', () => {
      const items = buildTopLevelItems({
        responsePreset: 'light',
        diffPreset: 'dark',
        baseTheme: 'dark',
      });
      expect(items.find((i) => i.choice === 'bundled-dark')!.description).toBe(
        '[active]'
      );
      expect(items.find((i) => i.choice === 'auto')!.description).toBe('');
    });

    it('marks Custom as [active] when prefs do not match any bundle', () => {
      const items = buildTopLevelItems({ promptPreset: 'ocean' });
      expect(items.find((i) => i.choice === 'custom')!.description).toBe(
        '[active]'
      );
    });
  });

  describe('wizard step ordering', () => {
    it('advances prompt → response → diff → terminal', () => {
      expect(nextStep('prompt')).toBe('response');
      expect(nextStep('response')).toBe('diff');
      expect(nextStep('diff')).toBeNull();
    });

    it('walks back diff → response → prompt → top-level', () => {
      expect(prevStep('diff')).toBe('response');
      expect(prevStep('response')).toBe('prompt');
      expect(prevStep('prompt')).toBeNull();
    });

    it('exposes step config in the spec order', () => {
      expect(wizardSteps.map((s) => s.id)).toEqual([
        'prompt',
        'response',
        'diff',
      ]);
    });

    it('getStep throws for an unknown id', () => {
      expect(() => getStep('whatever' as any)).toThrow();
    });

    it('activeId returns the saved preset or "default" fallback', () => {
      const promptStep = getStep('prompt');
      expect(promptStep.activeId({})).toBe('default');
      expect(promptStep.activeId({ promptPreset: 'ocean' })).toBe('ocean');
    });
  });

  describe('applyAuto', () => {
    it('clears every preset and persists empty prefs', () => {
      saveUserThemePrefs({
        promptPreset: 'ocean',
        responsePreset: 'light',
        diffPreset: 'colorblind-dark',
        baseTheme: 'dark',
      });
      const result = applyAuto();
      expect(result.saved).toBe(true);
      expect(result.prefs).toEqual({});
      expect(result.delta).toEqual({
        prompt: null,
        response: null,
        diff: null,
        baseTheme: null,
      });
      expect(loadUserThemePrefs()).toEqual({});
      expect(result.message).toMatch(/reset/i);
    });
  });

  describe('applyBundled', () => {
    it('applies dark bundle and persists baseTheme + presets', () => {
      const result = applyBundled('dark')!;
      expect(result.prefs.baseTheme).toBe('dark');
      expect(result.prefs.responsePreset).toBe('light');
      expect(result.prefs.diffPreset).toBe('dark');
      expect(result.delta.baseTheme).toBeDefined();
      expect(result.delta.prompt).toBeDefined();
      expect(result.delta.response).toBeDefined();
      expect(result.delta.diff).toBeDefined();
      expect(loadUserThemePrefs().baseTheme).toBe('dark');
    });

    it('applies light bundle similarly', () => {
      const result = applyBundled('light')!;
      expect(result.prefs.baseTheme).toBe('light');
      expect(result.prefs.promptPreset).toBe('paper');
      expect(result.prefs.diffPreset).toBe('light');
    });
  });

  describe('applyWizardStep', () => {
    it('applies a prompt preset, leaves response/diff slots untouched', () => {
      saveUserThemePrefs({ responsePreset: 'dark' });
      const result = applyWizardStep('prompt', 'purple', loadUserThemePrefs())!;
      expect(result.prefs.promptPreset).toBe('purple');
      expect(result.prefs.responsePreset).toBe('dark');
      expect(result.delta.prompt).toBeDefined();
      // Other delta slots stay undefined so the runtime leaves them alone.
      expect(result.delta.response).toBeUndefined();
      expect(result.delta.diff).toBeUndefined();
      // Persisted to disk.
      expect(loadUserThemePrefs().promptPreset).toBe('purple');
    });

    it('clears the slot when the user picks the "default" preset', () => {
      saveUserThemePrefs({ promptPreset: 'purple' });
      const result = applyWizardStep(
        'prompt',
        'default',
        loadUserThemePrefs()
      )!;
      expect(result.prefs.promptPreset).toBeUndefined();
      expect(loadUserThemePrefs().promptPreset).toBeUndefined();
    });

    it('returns null on unknown preset id (caller surfaces error)', () => {
      const result = applyWizardStep('prompt', 'definitely-not-real', {});
      expect(result).toBeNull();
    });

    it('applies a response preset', () => {
      const result = applyWizardStep('response', 'light', {})!;
      expect(result.prefs.responsePreset).toBe('light');
      expect(result.delta.response).toBeDefined();
      expect(result.delta.prompt).toBeUndefined();
    });

    it('applies a diff preset', () => {
      const result = applyWizardStep(
        'diff',
        'colorblind-dark',
        loadUserThemePrefs()
      )!;
      expect(result.prefs.diffPreset).toBe('colorblind-dark');
      expect(result.delta.diff).toBeDefined();
      expect(result.delta.diff!.id).toBe('colorblind-dark');
    });

    it('changing one slot does not perturb the others', () => {
      saveUserThemePrefs({ promptPreset: 'ocean', responsePreset: 'dark' });
      const result = applyWizardStep(
        'diff',
        'colorblind-dark',
        loadUserThemePrefs()
      )!;
      expect(result.prefs.promptPreset).toBe('ocean');
      expect(result.prefs.responsePreset).toBe('dark');
      expect(result.prefs.diffPreset).toBe('colorblind-dark');
    });
  });
});
